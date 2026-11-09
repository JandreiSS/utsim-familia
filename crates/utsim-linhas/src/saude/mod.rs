//! Indicators of the line Saúde e hospital (`docs/plano_linhas_especializadas.md`, section
//! 9): patients seen per day, the wait for the doctor by colour of the risk classification
//! and the fraction within the colour's target, patients that left without being seen,
//! no-shows, the stay in the service, the occupancy of the teams and of the beds, and the
//! census (patients present) by hour of the day.
//!
//! A patient is an item with the label `gravidade` (1 red … 5 blue). The wait for the
//! doctor runs from the end of the risk classification (or the arrival, without one) to the
//! first entry into a consulting room; door to doctor runs from the arrival. The objects are
//! recognised by the preset that made them (`saude/triagem`, `saude/consultorio`,
//! `saude/procedimento`, `saude/leitos`) or, without one, by their name (`triagem`,
//! `consult`, `leito`). Entries and exits come from the trace of every replication.

pub mod relatorio;

use std::collections::{BTreeMap, BTreeSet};

use utsim_core::stats::Summary;
use utsim_core::{ItemRecord, RunReport, TraceEvent, TraceRecord, Value};
use utsim_model::{Model, ModelError, ObjectKind};

use crate::pacote::Pack;

pub use crate::registro::SAUDE;

/// Minutes in a day.
const DAY: f64 = 1440.0;

/// Names of the colours of the risk classification, by `gravidade` (1 to 5).
pub const CORES: [&str; 5] = ["vermelho", "laranja", "amarelo", "verde", "azul"];

/// The name of colour `g` (`gravidade`), or `G<n>` out of 1 to 5.
pub fn cor(g: i64) -> String {
    usize::try_from(g - 1)
        .ok()
        .and_then(|i| CORES.get(i))
        .map_or_else(|| format!("G{g}"), |c| c.to_string())
}

/// Targets of the line: the longest wait for the doctor of each colour (red first) and the
/// bed occupancy above which the unit is overloaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    pub alvo: [f64; 5],
    pub ocupacao: f64,
}

impl Default for Meta {
    /// Red 0, orange 10, yellow 60, green 120, blue 240 minutes (decision H-D2, proposal)
    /// and 85 % of the beds (H-D5).
    fn default() -> Self {
        Meta {
            alvo: [0.0, 10.0, 60.0, 120.0, 240.0],
            ocupacao: 0.85,
        }
    }
}

impl Meta {
    /// The target wait of colour `g`; `None` out of 1 to 5.
    pub fn alvo_de(&self, g: i64) -> Option<f64> {
        usize::try_from(g - 1)
            .ok()
            .and_then(|i| self.alvo.get(i).copied())
    }
}

/// Results of one colour in a replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Cor {
    /// Patients that saw a doctor.
    pub atendidos: usize,
    /// Patients that left without being seen.
    pub abandonos: usize,
    /// Mean wait for the doctor (after the classification); `None` when no patient of
    /// this colour saw a doctor.
    pub espera: Option<f64>,
    /// Fraction seen within the colour's target.
    pub no_alvo: Option<f64>,
    /// Mean stay in the service of the patients that left it.
    pub permanencia: f64,
}

/// Indicators of one replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Replicacao {
    /// Patients that left the service in the window, seen (not abandoned, not absent).
    pub atendidos: usize,
    pub por_dia: f64,
    /// Patients that left without being seen.
    pub abandonos: usize,
    /// Abandoned ÷ (abandoned + seen).
    pub abandono: f64,
    /// Booked patients that did not come (`presenca` 2).
    pub faltas: usize,
    pub faltas_dia: f64,
    /// Patients that saw a doctor (entered a consulting room).
    pub consultas: usize,
    /// Mean wait for the doctor, after the classification (`None`: nobody saw one).
    pub espera: Option<f64>,
    /// Mean time from the arrival to the doctor.
    pub porta_medico: Option<f64>,
    /// Fraction seen within the target of their colour.
    pub no_alvo: Option<f64>,
    /// Mean stay in the service of the patients seen.
    pub permanencia: f64,
    pub por_cor: BTreeMap<i64, Cor>,
    /// People: fraction of the on-shift time busy (walking or working).
    pub ocupacao: BTreeMap<String, f64>,
    /// Bed units: mean fraction of the beds taken.
    pub leitos: BTreeMap<String, f64>,
    /// Mean wait for a bed (in the queue right before a bed unit).
    pub espera_leito: Option<f64>,
    /// Mean stay in a bed.
    pub internacao: Option<f64>,
}

/// The indicators of a set of replications.
#[derive(Clone, Debug)]
pub struct Saude {
    pub modelo: String,
    pub linha: String,
    pub meta: Meta,
    pub reps: Vec<Replicacao>,
    pub reports: Vec<RunReport>,
    /// Mean patients present by hour of the day, all replications.
    pub censo: Vec<(usize, f64)>,
}

/// What each object is for the indicators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Papel {
    Triagem,
    Consulta,
    Leitos,
    Outro,
}

fn papel(model: &Model, b: usize) -> Papel {
    let Some(o) = model.objects.get(b) else {
        return Papel::Outro;
    };
    match o.preset.as_deref() {
        Some("saude/triagem") => return Papel::Triagem,
        Some("saude/consultorio" | "saude/procedimento") => return Papel::Consulta,
        Some("saude/leitos") => return Papel::Leitos,
        Some(p) if p.starts_with("saude/") => return Papel::Outro,
        _ => {}
    }
    if !matches!(o.kind, ObjectKind::Processor { .. }) {
        return Papel::Outro;
    }
    let n = o.name.to_lowercase();
    if n.contains("triagem") || n.contains("classifica") {
        Papel::Triagem
    } else if n.contains("consult") {
        Papel::Consulta
    } else if n.contains("leito") {
        Papel::Leitos
    } else {
        Papel::Outro
    }
}

fn label(r: &ItemRecord, name: &str) -> Option<f64> {
    match r.labels.get(name) {
        Some(Value::Num(v)) if v.is_finite() => Some(*v),
        _ => None,
    }
}

fn gave_up(r: &ItemRecord) -> bool {
    r.labels.contains_key(utsim_objects::ABANDONED_LABEL)
}

fn absent(r: &ItemRecord) -> bool {
    label(r, "presenca").is_some_and(|p| p.round() as i64 == 2)
}

fn gravidade(r: &ItemRecord) -> Option<i64> {
    label(r, "gravidade").map(|g| g.round() as i64)
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

/// The indicators of one replication from its report, item log and trace; also the time
/// patients were present in each hour of the day and the length of that hour covered.
pub fn replicacao(
    model: &Model,
    report: &RunReport,
    items: &[ItemRecord],
    trace: &[TraceRecord],
    meta: Meta,
) -> (Replicacao, [(f64, f64); 24]) {
    let from = report.stats_from;
    let end = report.end_time;
    let window = (end - from).max(f64::MIN_POSITIVE);
    let papeis: Vec<Papel> = (0..model.objects.len()).map(|b| papel(model, b)).collect();
    let is = |b: usize, p: Papel| papeis.get(b) == Some(&p);

    // From the trace: the end of the classification and the first doctor of each patient,
    // and the census.
    let mut triado: BTreeMap<u64, f64> = BTreeMap::new();
    let mut medico: BTreeMap<u64, f64> = BTreeMap::new();
    let mut censo = [(0.0f64, 0.0f64); 24];
    let mut presentes = 0i64;
    let mut last = from;
    let add = |t0: f64, t1: f64, n: i64, censo: &mut [(f64, f64); 24]| {
        // Splits [t0, t1) by the hours of the day.
        let mut t = t0.max(from);
        while t < t1 {
            let h = ((t.rem_euclid(DAY)) / 60.0).floor() as usize % 24;
            let next = ((t / 60.0).floor() + 1.0) * 60.0;
            let stop = next.min(t1);
            censo[h].0 += n as f64 * (stop - t);
            censo[h].1 += stop - t;
            t = stop;
        }
    };
    for rec in trace {
        let entering = match rec.event {
            TraceEvent::Created { .. } | TraceEvent::Unpacked { .. } => 1,
            TraceEvent::Destroyed { .. } | TraceEvent::Packed { .. } => -1,
            _ => 0,
        };
        if entering != 0 {
            if rec.t > last {
                add(last, rec.t, presentes, &mut censo);
                last = rec.t;
            }
            presentes += entering;
        }
        if let TraceEvent::Moved { item, from: a, to } = rec.event {
            if is(a, Papel::Triagem) {
                triado.entry(item).or_insert(rec.t);
            }
            if is(to, Papel::Consulta) {
                medico.entry(item).or_insert(rec.t);
            }
        }
    }
    if end > last {
        add(last, end, presentes, &mut censo);
    }

    let ended: Vec<&ItemRecord> = items
        .iter()
        .filter(|r| r.ended_at.is_some_and(|t| t >= from))
        .collect();
    let mut esperas = Vec::new();
    let mut portas = Vec::new();
    let mut estadias = Vec::new();
    let mut alvo_ok: Vec<bool> = Vec::new();
    let mut por_cor: BTreeMap<i64, Cor> = BTreeMap::new();
    let mut cor_esperas: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    let mut cor_estadias: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    let mut cor_alvo: BTreeMap<i64, Vec<bool>> = BTreeMap::new();
    let mut faltas = 0usize;
    let mut abandonos = 0usize;
    for r in &ended {
        if absent(r) {
            faltas += 1;
            continue;
        }
        let g = gravidade(r);
        if gave_up(r) {
            abandonos += 1;
            if let Some(g) = g {
                por_cor.entry(g).or_default().abandonos += 1;
            }
            continue;
        }
        let estadia = r.ended_at.unwrap_or(r.created_at) - r.created_at;
        estadias.push(estadia);
        if let Some(g) = g {
            por_cor.entry(g).or_default().atendidos += 1;
            cor_estadias.entry(g).or_default().push(estadia);
        }
        let Some(t_med) = medico.get(&r.key).copied() else {
            continue;
        };
        let inicio = triado
            .get(&r.key)
            .copied()
            .filter(|t| *t <= t_med)
            .unwrap_or(r.created_at);
        let espera = (t_med - inicio).max(0.0);
        esperas.push(espera);
        portas.push((t_med - r.created_at).max(0.0));
        if let Some(g) = g {
            cor_esperas.entry(g).or_default().push(espera);
            if let Some(a) = meta.alvo_de(g) {
                let ok = espera <= a + 1e-9;
                alvo_ok.push(ok);
                cor_alvo.entry(g).or_default().push(ok);
            }
        }
    }
    let fracao = |xs: &[bool]| {
        (!xs.is_empty()).then(|| xs.iter().filter(|x| **x).count() as f64 / xs.len() as f64)
    };
    for (g, c) in por_cor.iter_mut() {
        c.espera = cor_esperas
            .get(g)
            .filter(|v| !v.is_empty())
            .map(|v| mean(v));
        c.permanencia = mean(cor_estadias.get(g).map(Vec::as_slice).unwrap_or(&[]));
        c.no_alvo = cor_alvo.get(g).and_then(|v| fracao(v));
    }

    let mut ocupacao = BTreeMap::new();
    let mut leitos = BTreeMap::new();
    let mut internacao = Vec::new();
    for (b, o) in model.objects.iter().enumerate() {
        let Some(rep) = report.blocks.get(b) else {
            continue;
        };
        let f = |k: &str| rep.states.get(k).copied().unwrap_or(0.0);
        match &o.kind {
            ObjectKind::Transporter { .. } => {
                let on = 1.0 - f("off_shift");
                if on > 1e-9 {
                    let busy = f("utilize") + f("travel_empty") + f("travel_loaded");
                    ocupacao.insert(o.name.clone(), busy / on);
                }
            }
            ObjectKind::Processor { capacity, .. } if is(b, Papel::Leitos) => {
                leitos.insert(o.name.clone(), rep.content_mean / (*capacity).max(1) as f64);
                if rep.staytime.count > 0 {
                    internacao.push((rep.staytime.mean, rep.staytime.count));
                }
            }
            _ => {}
        }
    }
    // The wait for a bed: queues whose every output is a bed unit.
    let mut espera_leito = Vec::new();
    for (b, o) in model.objects.iter().enumerate() {
        if !matches!(o.kind, ObjectKind::Queue { .. }) {
            continue;
        }
        let outs: BTreeSet<usize> = model
            .connections
            .iter()
            .filter(|(f, _)| *f == o.name)
            .filter_map(|(_, t)| model.objects.iter().position(|x| x.name == *t))
            .collect();
        if outs.is_empty() || !outs.iter().all(|t| is(*t, Papel::Leitos)) {
            continue;
        }
        if let Some(rep) = report.blocks.get(b).filter(|r| r.staytime.count > 0) {
            espera_leito.push((rep.staytime.mean, rep.staytime.count));
        }
    }
    let pesada = |xs: &[(f64, u64)]| {
        let n: u64 = xs.iter().map(|x| x.1).sum();
        (n > 0).then(|| xs.iter().map(|(m, c)| m * *c as f64).sum::<f64>() / n as f64)
    };

    let n = estadias.len();
    let total = n + abandonos;
    (
        Replicacao {
            atendidos: n,
            por_dia: n as f64 / (window / DAY),
            abandonos,
            abandono: if total == 0 {
                0.0
            } else {
                abandonos as f64 / total as f64
            },
            faltas,
            faltas_dia: faltas as f64 / (window / DAY),
            consultas: esperas.len(),
            espera: (!esperas.is_empty()).then(|| mean(&esperas)),
            porta_medico: (!portas.is_empty()).then(|| mean(&portas)),
            no_alvo: fracao(&alvo_ok),
            permanencia: mean(&estadias),
            por_cor,
            ocupacao,
            leitos,
            espera_leito: pesada(&espera_leito),
            internacao: pesada(&internacao),
        },
        censo,
    )
}

impl Saude {
    /// Runs every replication with the per-item metrics and the trace.
    pub fn rodar(model: &Model, pack: Option<&Pack>, meta: Meta) -> Result<Saude, ModelError> {
        model.validate()?;
        let mut reps = Vec::new();
        let mut reports = Vec::new();
        let mut censo = [(0.0f64, 0.0f64); 24];
        for r in 0..model.run.replications {
            let mut sim = model.build(r);
            sim.enable_item_log();
            sim.enable_trace();
            let report = model.drive(&mut sim)?;
            let items = sim.take_item_log();
            let trace = sim.take_trace();
            let (rep, c) = replicacao(model, &report, &items, &trace, meta);
            for h in 0..24 {
                censo[h].0 += c[h].0;
                censo[h].1 += c[h].1;
            }
            reps.push(rep);
            reports.push(report);
        }
        let censo = (0..24)
            .filter(|h| censo[*h].1 > 0.0)
            .map(|h| (h, censo[h].0 / censo[h].1))
            .collect();
        Ok(Saude {
            modelo: model.name.clone(),
            linha: pack.map_or_else(|| "Saúde e hospital".to_string(), |p| p.library.title()),
            meta,
            reps,
            reports,
            censo,
        })
    }

    pub fn resumo(&self, f: impl Fn(&Replicacao) -> f64) -> Summary {
        let xs: Vec<f64> = self.reps.iter().map(f).collect();
        Summary::from_samples(&xs)
    }

    pub fn resumo_opt(&self, f: impl Fn(&Replicacao) -> Option<f64>) -> Option<Summary> {
        let xs: Vec<f64> = self.reps.iter().filter_map(f).collect();
        (!xs.is_empty()).then(|| Summary::from_samples(&xs))
    }
}

#[cfg(test)]
mod tests;
