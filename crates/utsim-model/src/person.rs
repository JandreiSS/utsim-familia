//! The `person` field of a Transporter used as a person (`utsim_core::person`): fatigue,
//! learning curve, skills, walking speed with a load, individual pace and the manual
//! lifting check.
//!
//! ```text
//! (name: "ana", kind: Transporter(speed: 1.3, load_time: Const(4.0)),
//!     person: Some((
//!         fatigue: Some(Dynamic(work: 1.0e-4, recovery: 1.0e-3, limit: Some(0.5))),
//!         learning: Some((rate: 0.85, first: 2.0)),
//!         skills: ["torno"],
//!         walk_speed_loaded: Some((per_kg: 0.01, above: 5.0, min: 0.8)),
//!         variation: Some(()),
//!         lift: Some((horizontal: 40.0, vertical: 30.0, distance: 50.0)),
//!     ))),
//! ```
//!
//! Every part is optional and a model without `person` runs exactly as before. The
//! operators of a processor may require a skill with `OperatorUse(requires: "torno")`.

use std::collections::{BTreeMap, BTreeSet};

use utsim_core::person::{NIOSH_LOAD_CONSTANT, lifting_index, recommended_weight_limit};
use utsim_core::{Dist, Fatigue};

use crate::{DispatchRule, Model, ObjectDef, ObjectKind};

/// Reference limit of the CLT, art. 198 (kg per employee) [conferir no texto vigente].
pub const CLT_198_KG: f64 = 60.0;
/// CLT, art. 390: limit for women in continuous work (kg).
pub const CLT_390_CONTINUOUS_KG: f64 = 20.0;
/// CLT, art. 390: limit for women in occasional work (kg).
pub const CLT_390_OCCASIONAL_KG: f64 = 25.0;

/// Heights (m) outside this range get a warning: probably a typo (cm, feet).
pub const HEIGHT_RANGE_M: (f64, f64) = (1.40, 2.10);
/// Body masses (kg) outside this range get a warning: probably a typo (pounds).
pub const MASS_RANGE_KG: (f64, f64) = (35.0, 200.0);

/// Largest value of a distribution (its mean when unbounded above).
fn support_max(d: &Dist) -> f64 {
    match d {
        Dist::Const(c) => *c,
        Dist::Uniform { max, .. } | Dist::Triangular { max, .. } | Dist::Beta { max, .. } => *max,
        Dist::Discrete(table) => table
            .iter()
            .filter(|(_, w)| *w > 0.0)
            .map(|(v, _)| *v)
            .fold(f64::NEG_INFINITY, f64::max),
        Dist::Empirical(xs) => xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        Dist::Shifted { offset, dist } => offset + support_max(dist),
        other => other.mean(),
    }
}

impl Model {
    fn object(&self, name: &str) -> Option<&ObjectDef> {
        self.objects.iter().find(|o| o.name == name)
    }

    /// The people of a team: the transporter itself, or the members of a dispatcher.
    fn team_members(&self, team: &str) -> Vec<&ObjectDef> {
        match self.object(team).map(|o| &o.kind) {
            Some(ObjectKind::Transporter { .. }) => self.object(team).into_iter().collect(),
            Some(ObjectKind::Dispatcher { team, .. }) => {
                team.iter().filter_map(|m| self.object(m)).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Teams with more operators than the processes they operate can use at once
    /// (capacity × operators per piece): the extra ones never work, however long the queue.
    /// A team that also carries, repairs or maintains has other work and is left alone, and
    /// so is one whose members keep different shifts or whose dispatcher spreads the work
    /// (`RoundRobin`, `LeastBusy`, `LongestIdle`, `Random`): they take turns on purpose.
    pub(crate) fn idle_operator_warnings(&self, out: &mut Vec<String>) {
        let mut other: BTreeSet<&str> = BTreeSet::new();
        for o in &self.objects {
            other.extend(o.transport.as_deref());
            if let Some(d) = &o.downtime {
                other.extend(d.repair_crew.as_deref());
                other.extend(d.preventive.iter().filter_map(|p| p.crew.as_deref()));
            }
        }
        // A member named directly is counted with its dispatcher's team.
        let pool_of = |team: &'_ str| -> String {
            self.objects
                .iter()
                .find_map(|o| match &o.kind {
                    ObjectKind::Dispatcher { team: members, .. }
                        if members.iter().any(|m| m == team) =>
                    {
                        Some(o.name.clone())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| team.to_string())
        };
        // Per team: servers × operators per piece over every process it operates.
        // (node, capacity, operators per piece) of each process a team operates.
        type Uses<'a> = Vec<(&'a str, usize, usize)>;
        let mut demand: BTreeMap<String, (usize, Uses)> = BTreeMap::new();
        for o in &self.objects {
            let ObjectKind::Processor { capacity, .. } = &o.kind else {
                continue;
            };
            for op in o.kind.operator_uses() {
                let e = demand.entry(pool_of(&op.team)).or_default();
                e.0 = e.0.saturating_add(capacity.saturating_mul(op.count));
                e.1.push((o.name.as_str(), *capacity, op.count));
            }
        }
        let rotates = |team: &str| {
            self.objects.iter().any(|o| {
                o.name == team
                    && matches!(
                        &o.kind,
                        ObjectKind::Dispatcher { rule, .. }
                            if !matches!(rule, DispatchRule::FirstAvailable | DispatchRule::Nearest)
                    )
            })
        };
        for (team, (usable, uses)) in demand {
            let members = self.team_members(&team);
            let size = members.len();
            if size <= usable
                || other.contains(team.as_str())
                || rotates(&team)
                || members.iter().any(|m| other.contains(m.name.as_str()))
                || members.windows(2).any(|w| w[0].shifts != w[1].shifts)
            {
                continue;
            }
            let idle = size - usable;
            let parados = if idle == 1 {
                "1 fica parado".to_string()
            } else {
                format!("{idle} ficam parados")
            };
            out.push(match uses.as_slice() {
                [(node, capacity, count)] => format!(
                    "'{team}' tem {size} operadores, mas '{node}' usa no máximo {usable} ao mesmo \
                     tempo (capacidade {capacity} × {count} por peça): {parados} enquanto a fila \
                     cresce; aumente a capacidade de '{node}' para {} ou use {} operadores por \
                     peça (`operators.count`, com `team_speedup` se a equipe faz mais rápido)",
                    size.div_ceil(*count),
                    size.div_ceil(*capacity),
                ),
                _ => format!(
                    "'{team}' tem {size} operadores, mas {} usam no máximo {usable} ao mesmo \
                     tempo (capacidade × operadores por peça): {parados} enquanto as filas \
                     crescem; aumente a capacidade dos processos ou use mais operadores por \
                     peça (`operators.count`)",
                    uses.iter()
                        .map(|(n, _, _)| format!("'{n}'"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    }

    /// Errors in the `person` fields and in the skills required by the operators.
    pub(crate) fn validate_people(&self, errors: &mut Vec<String>) {
        for obj in &self.objects {
            if let Some(p) = &obj.person {
                if !matches!(obj.kind, ObjectKind::Transporter { .. }) {
                    errors.push(format!(
                        "{}: o perfil de pessoa (person) só vale para um Transporter",
                        obj.name
                    ));
                }
                for e in p.validate() {
                    errors.push(format!("{}.person.{e}", obj.name));
                }
            }
            if let Some(rule) = obj.operation.as_deref() {
                if !matches!(obj.kind, ObjectKind::Processor { .. }) {
                    errors.push(format!(
                        "{}: a regra de operação (operation) só vale para um Processor",
                        obj.name
                    ));
                }
                if rule.skills.iter().any(|s| s.trim().is_empty()) {
                    errors.push(format!(
                        "{}: operation.skills: os nomes das habilidades não podem ser vazios",
                        obj.name
                    ));
                }
            }
            for op in obj.kind.operator_uses() {
                if op.requires.as_deref().is_some_and(|s| s.trim().is_empty()) {
                    errors.push(format!(
                        "{}: operators.requires precisa do nome de uma habilidade",
                        obj.name
                    ));
                }
            }
        }
    }

    /// Protective equipment an operator of `obj` must wear: the station's own `ppe` and
    /// that of the risk zones its position is in, without repeats.
    pub fn required_ppe(&self, obj: &ObjectDef) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let at = [obj.position[0], obj.position[1]];
        let zones = self
            .zones
            .iter()
            .filter(|z| crate::exposure::contains(&z.polygon, at));
        for e in obj.ppe.iter().chain(zones.flat_map(|z| z.ppe.iter())) {
            if !out.contains(e) {
                out.push(e.clone());
            }
        }
        out
    }

    /// The processor's operation rule ([`crate::OperationDef`]) as the simulation uses it.
    pub(crate) fn operation_of(&self, obj: &ObjectDef) -> utsim_objects::Operation {
        let Some(rule) = obj.operation.as_deref() else {
            return utsim_objects::Operation::default();
        };
        utsim_objects::Operation {
            skills: rule.skills.clone(),
            ppe: if rule.require_ppe {
                self.required_ppe(obj)
            } else {
                Vec::new()
            },
            enter: rule.enter.clone(),
            leave: rule.leave.clone(),
        }
    }

    /// Heaviest value of the weight label `label` that the sources give their items
    /// (0 when no source sets it).
    fn max_weight(&self, label: &str) -> f64 {
        self.objects
            .iter()
            .filter_map(|o| match &o.kind {
                ObjectKind::Source { labels, .. } => labels.get(label).map(support_max),
                _ => None,
            })
            .filter(|w| w.is_finite())
            .fold(0.0, f64::max)
    }

    /// Warnings about people: a skill nobody of the team has, manual loads above the
    /// reference limits or the measured capacity, heights and masses out of range.
    pub(crate) fn people_warnings(&self, out: &mut Vec<String>) {
        for o in &self.objects {
            let Some(p) = o.person.as_deref() else {
                continue;
            };
            let (lo, hi) = HEIGHT_RANGE_M;
            if let Some(h) = p.height_m.filter(|h| *h < lo || *h > hi) {
                out.push(format!(
                    "{}: altura {h} m fora da faixa usual ({lo} a {hi} m); confira a unidade (metros)",
                    o.name
                ));
            }
            let (lo, hi) = MASS_RANGE_KG;
            if let Some(m) = p.mass_kg.filter(|m| *m < lo || *m > hi) {
                out.push(format!(
                    "{}: massa {m} kg fora da faixa usual ({lo} a {hi} kg); confira a unidade (kg)",
                    o.name
                ));
            }
            for task in p.times.keys() {
                let station = task.strip_suffix(":setup").unwrap_or(task);
                let known = task == utsim_core::person::TASK_LOAD
                    || task == utsim_core::person::TASK_UNLOAD
                    || self
                        .object(station)
                        .is_some_and(|s| matches!(s.kind, ObjectKind::Processor { .. }));
                if !known {
                    out.push(format!(
                        "{}: tempo próprio da tarefa '{task}', que não é posto (Processor) do                          modelo nem '{}' ou '{}'; ele nunca é usado",
                        o.name,
                        utsim_core::person::TASK_LOAD,
                        utsim_core::person::TASK_UNLOAD
                    ));
                }
            }
        }
        for (obj, op) in self
            .objects
            .iter()
            .flat_map(|o| o.kind.operator_uses().map(move |op| (o, op)))
        {
            let Some(skill) = op.requires.as_deref() else {
                continue;
            };
            let members = self.team_members(&op.team);
            let qualified = members
                .iter()
                .filter(|m| m.person.as_ref().is_some_and(|p| p.has_skill(skill)))
                .count();
            let direct = matches!(
                self.object(&op.team).map(|o| &o.kind),
                Some(ObjectKind::Transporter { .. })
            );
            if qualified == 0 {
                out.push(if direct {
                    format!(
                        "{}: '{}' não tem a habilidade '{skill}' exigida; sem despachante a tarefa vai para ele assim mesmo",
                        obj.name, op.team
                    )
                } else {
                    format!(
                        "{}: ninguém da equipe '{}' tem a habilidade '{skill}': a operação ficaria esperando para sempre",
                        obj.name, op.team
                    )
                });
            } else if qualified < op.count {
                out.push(format!(
                    "{}: precisa de {} operadores com '{skill}', mas a equipe '{}' só tem {qualified}",
                    obj.name, op.count, op.team
                ));
            }
        }
        // Operation rules: skills and protective equipment every operator must have.
        for obj in &self.objects {
            let rule = self.operation_of(obj);
            if rule.skills.is_empty() && rule.ppe.is_empty() {
                continue;
            }
            for op in obj.kind.operator_uses() {
                let mut skills = rule.skills.clone();
                skills.extend(op.requires.iter().cloned());
                let ok = |m: &&&ObjectDef| {
                    let has = skills.is_empty()
                        || m.person
                            .as_ref()
                            .is_some_and(|p| skills.iter().all(|s| p.has_skill(s)));
                    has && rule.ppe.iter().all(|e| m.ppe.contains(e))
                };
                let qualified = self.team_members(&op.team).iter().filter(ok).count();
                let mut need: Vec<String> = skills.iter().map(|s| format!("'{s}'")).collect();
                need.extend(rule.ppe.iter().map(|e| format!("EPI '{e}'")));
                let need = need.join(", ");
                if qualified == 0 {
                    out.push(format!(
                        "{}: ninguém da equipe '{}' atende à regra de operação ({need}): a operação ficaria esperando para sempre",
                        obj.name, op.team
                    ));
                } else if qualified < op.count {
                    out.push(format!(
                        "{}: precisa de {} operadores com {need}, mas a equipe '{}' só tem {qualified}",
                        obj.name, op.count, op.team
                    ));
                }
            }
        }
        // People who carry items (directly or as members of a dispatcher).
        let mut carriers: BTreeSet<&str> = BTreeSet::new();
        for o in &self.objects {
            if let Some(t) = &o.transport {
                for m in self.team_members(t) {
                    carriers.insert(m.name.as_str());
                }
            }
        }
        for name in carriers {
            let Some(obj) = self.object(name) else {
                continue;
            };
            let Some(p) = obj.person.as_deref() else {
                continue;
            };
            let w = self.max_weight(&p.weight_label);
            if w <= 0.0 {
                continue;
            }
            let lift = p.lift.unwrap_or_default();
            let rwl = recommended_weight_limit(&lift);
            let li = lifting_index(w, &lift);
            let what = if p.lift.is_some() {
                "limite de peso recomendado do levantamento informado"
            } else {
                "constante de carga (condições ideais)"
            };
            if li > 3.0 {
                out.push(format!(
                    "{name}: ALERTA: carrega até {w} kg; {what} pela equação revisada do NIOSH: {rwl:.1} kg, \
                     índice de levantamento {li:.2} > 3 (risco alto) [conferir no texto vigente]"
                ));
            } else if li > 1.0 {
                out.push(format!(
                    "{name}: carrega até {w} kg; {what} pela equação revisada do NIOSH: {rwl:.1} kg, \
                     índice de levantamento {li:.2} > 1 [conferir no texto vigente]"
                ));
            }
            if let Some(c) = p.capacity.and_then(|c| c.load_kg).filter(|c| w > *c) {
                out.push(format!(
                    "{name}: carrega até {w} kg, acima da capacidade medida dessa pessoa ({c} kg)"
                ));
            }
            if p.sex == Some(utsim_core::Sex::Female) && w > CLT_390_CONTINUOUS_KG {
                out.push(format!(
                    "{name}: carrega até {w} kg, acima de {CLT_390_CONTINUOUS_KG} kg, limite da CLT                      (art. 390) para o trabalho contínuo da mulher ({CLT_390_OCCASIONAL_KG} kg no                      ocasional) [conferir no texto vigente]"
                ));
            }
            if w > CLT_198_KG {
                out.push(format!(
                    "{name}: carrega até {w} kg, acima de {CLT_198_KG} kg, limite de referência da CLT \
                     (art. 198) para remoção individual de peso [conferir no texto vigente]"
                ));
            }
        }
    }

    /// One line per person profile, for `utsim check`.
    pub fn person_lines(&self) -> Vec<String> {
        self.objects
            .iter()
            .filter_map(|o| {
                let p = o.person.as_deref()?;
                let mut parts = Vec::new();
                let mut who = Vec::new();
                match p.sex {
                    Some(utsim_core::Sex::Female) => who.push("sexo feminino".to_string()),
                    Some(utsim_core::Sex::Male) => who.push("sexo masculino".to_string()),
                    None => {}
                }
                who.extend(p.height_m.map(|h| format!("{h} m")));
                who.extend(p.mass_kg.map(|m| format!("{m} kg")));
                who.extend(p.age.map(|a| format!("{a} anos")));
                who.extend(p.experience_years.map(|e| format!("{e} anos de experiência")));
                if let Some(c) = &p.capacity {
                    who.extend(c.load_kg.map(|k| format!("capacidade medida {k} kg")));
                    who.extend(c.force_n.map(|n| format!("força medida {n} N")));
                }
                if !who.is_empty() {
                    parts.push(who.join(", "));
                }
                match &p.fatigue {
                    None => {}
                    Some(Fatigue::Allowance(a)) => {
                        parts.push(format!("tolerância de fadiga {:.1} %", a * 100.0))
                    }
                    Some(Fatigue::Dynamic {
                        work,
                        walk,
                        carry,
                        recovery,
                        time_factor,
                        speed_factor,
                        limit,
                        target,
                    }) => {
                        let mut s = format!(
                            "fadiga dinâmica (λ trabalho {work}, caminhada {walk}, +{carry}/kg; \
                             μ {recovery}; tempos × (1 + {time_factor}·F), velocidade × (1 − {speed_factor}·F)"
                        );
                        if let Some(l) = limit {
                            s.push_str(&format!("; pausa em F ≥ {l} até {target}"));
                        }
                        s.push(')');
                        parts.push(s);
                    }
                }
                if let Some(l) = &p.learning {
                    parts.push(format!(
                        "aprendizado {:.0} % (1.ª repetição {}× o tempo padrão)",
                        l.rate * 100.0,
                        l.first
                    ));
                }
                if !p.times.is_empty() {
                    let own: Vec<String> = p
                        .times
                        .iter()
                        .map(|(task, t)| {
                            let method = if t.method.is_empty() {
                                String::new()
                            } else {
                                format!(", método {}", t.method)
                            };
                            format!("{task} (média {:.1}{method})", t.dist.mean())
                        })
                        .collect();
                    parts.push(format!("tempos próprios: {}", own.join("; ")));
                }
                if !p.skills.is_empty() {
                    parts.push(format!("habilidades: {}", p.skills.join(", ")));
                }
                if let Some(w) = &p.walk_speed_loaded {
                    parts.push(format!(
                        "caminhada carregado −{:.1} %/kg acima de {} kg (mín. {})",
                        w.per_kg * 100.0,
                        w.above,
                        w.min
                    ));
                }
                if let Some(v) = &p.variation {
                    parts.push(format!(
                        "ritmo individual {:?} em [{}, {}]",
                        v.dist, v.min, v.max
                    ));
                }
                if let Some(l) = &p.lift {
                    parts.push(format!(
                        "levantamento NIOSH: limite recomendado {:.1} kg",
                        recommended_weight_limit(l)
                    ));
                } else if self.max_weight(&p.weight_label) > 0.0 {
                    parts.push(format!(
                        "peso pelo rótulo '{}' (referência {NIOSH_LOAD_CONSTANT} kg)",
                        p.weight_label
                    ));
                }
                if parts.is_empty() {
                    parts.push("perfil sem efeitos".into());
                }
                Some(format!("{}: {}", o.name, parts.join(", ")))
            })
            .collect()
    }
}
