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

use std::collections::BTreeSet;

use utsim_core::person::{NIOSH_LOAD_CONSTANT, lifting_index, recommended_weight_limit};
use utsim_core::{Dist, Fatigue};

use crate::{Model, ObjectDef, ObjectKind};

/// Reference limit of the CLT, art. 198 (kg per employee) [conferir no texto vigente].
pub const CLT_198_KG: f64 = 60.0;

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
    /// reference limits.
    pub(crate) fn people_warnings(&self, out: &mut Vec<String>) {
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
