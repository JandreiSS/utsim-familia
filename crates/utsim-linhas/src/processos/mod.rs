//! Indicators of the line Processos contínuos e batelada (`docs/plano_linhas_especializadas.md`,
//! section 11): batches per day, volume produced per day, mean yield, batches out of
//! specification, batch lead time, the same by product, the time of every reactor (making,
//! in cleaning, broken, blocked by a full downstream tank, without a batch) with the
//! cleanings per day and the mean cycle of a batch, the mean level of every tank of the
//! fluid network and the occupancy of the operators.
//!
//! A batch is an item that leaves through a sink; one that ends in a sink with
//! `sink_role: Scrap` is out of specification. The packs that a continuous filler of the
//! fluid network creates (`ToItems`) are not batches: they count as units filled per day. Its labels: `produto` (the recipe), `volume`
//! (litres charged) and `rendimento` (fraction that becomes product, 1 without the label).
//! The reactors are the processors that are not inspection or rework stations; the
//! cleaning between products (CIP) is their `changeover`.

pub mod relatorio;

use std::collections::BTreeMap;

use utsim_core::stats::Summary;
use utsim_core::{ItemRecord, RunReport, Value};
use utsim_fluid::FluidKind;
use utsim_model::quality::SinkRole;
use utsim_model::{Model, ModelError, ObjectKind};

use crate::pacote::Pack;

pub use crate::registro::PROCESSOS;

/// Minutes in a day.
const DAY: f64 = 1440.0;

/// Target of the line: the yield above which the process is on target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    pub rendimento: f64,
}

impl Default for Meta {
    /// 95 %.
    fn default() -> Self {
        Meta { rendimento: 0.95 }
    }
}

/// Time of one reactor in a replication (fractions of the planned time).
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Reator {
    /// Making a batch.
    pub produzindo: f64,
    /// Cleaning between products (setup).
    pub em_limpeza: f64,
    pub quebrado: f64,
    /// Done, waiting for room downstream (a full tank).
    pub bloqueado: f64,
    /// Waiting for a batch.
    pub sem_lote: f64,
    /// Batches that left the reactor.
    pub lotes: u64,
    /// Mean time of a batch inside the reactor (cleaning, making and waiting to leave).
    pub ciclo: f64,
    /// Cleanings per day (`None` without `changeover`).
    pub limpezas_dia: Option<f64>,
}

/// Results of one product in a replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Produto {
    pub lotes: usize,
    /// Volume produced (litres: volume × yield).
    pub volume: f64,
    /// Mean lead time of a batch.
    pub atravessamento: f64,
}

/// Indicators of one replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Replicacao {
    /// Good batches that left in the window.
    pub lotes: usize,
    pub lotes_dia: f64,
    /// Batches out of specification.
    pub fora: usize,
    /// Units made by the continuous fillers of the fluid network (`ToItems`) that left
    /// through a sink, per day (`None`: no such filler).
    pub envasadas_dia: Option<f64>,
    /// Volume produced per day (`None`: no batch with the label `volume`).
    pub volume_dia: Option<f64>,
    /// Mean yield of the good batches (`None`: no batch with the label `rendimento`).
    pub rendimento: Option<f64>,
    /// Mean lead time of the good batches (creation to exit).
    pub atravessamento: f64,
    pub por_produto: BTreeMap<i64, Produto>,
    pub reatores: BTreeMap<String, Reator>,
    /// Tanks of the fluid network: mean level ÷ capacity.
    pub tanques: BTreeMap<String, f64>,
    /// People: fraction of the on-shift time busy (walking or working).
    pub ocupacao: BTreeMap<String, f64>,
}

/// The indicators of a set of replications.
#[derive(Clone, Debug)]
pub struct Processos {
    pub modelo: String,
    pub linha: String,
    pub meta: Meta,
    pub reps: Vec<Replicacao>,
    pub reports: Vec<RunReport>,
}

fn label(r: &ItemRecord, name: &str) -> Option<f64> {
    match r.labels.get(name) {
        Some(Value::Num(v)) if v.is_finite() => Some(*v),
        _ => None,
    }
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

/// Whether an object is a reactor (or another processing equipment) for the indicators.
pub fn e_reator(o: &utsim_model::ObjectDef) -> bool {
    matches!(o.kind, ObjectKind::Processor { .. })
        && o.inspection.is_none()
        && o.rework.is_none()
        && !o
            .preset
            .as_deref()
            .is_some_and(|p| p.starts_with("qualidade/"))
}

/// The indicators of one replication from its report and item log.
pub fn replicacao(model: &Model, report: &RunReport, items: &[ItemRecord]) -> Replicacao {
    let from = report.stats_from;
    let end = report.end_time;
    let window = (end - from).max(f64::MIN_POSITIVE);
    let days = window / DAY;
    let sink_role = |b: usize| -> Option<Option<SinkRole>> {
        let o = model.objects.get(b)?;
        matches!(o.kind, ObjectKind::Sink).then_some(o.sink_role)
    };

    // Objects where the continuous fillers put the units they make.
    let fillers: Vec<usize> = model
        .fluids
        .iter()
        .flat_map(|d| &d.nodes)
        .filter_map(|n| match &n.kind {
            FluidKind::ToItems { object, .. } => {
                model.objects.iter().position(|o| &o.name == object)
            }
            _ => None,
        })
        .collect();
    let mut envasadas = 0usize;
    let mut leads = Vec::new();
    let mut fora = 0usize;
    let mut volume: Option<f64> = None;
    let mut rendimentos = Vec::new();
    let mut por_produto: BTreeMap<i64, Produto> = BTreeMap::new();
    let mut prod_leads: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    for r in items {
        let Some(t_end) = r.ended_at.filter(|t| *t >= from) else {
            continue;
        };
        let Some(role) = r.block.and_then(sink_role) else {
            continue;
        };
        if fillers.iter().any(|b| r.visits.contains_key(b)) {
            if role != Some(SinkRole::Scrap) {
                envasadas += 1;
            }
            continue;
        }
        if role == Some(SinkRole::Scrap) {
            fora += 1;
            continue;
        }
        let lead = t_end - r.created_at;
        leads.push(lead);
        let rend = label(r, "rendimento");
        if let Some(y) = rend {
            rendimentos.push(y);
        }
        let v = label(r, "volume").map(|v| v * rend.unwrap_or(1.0));
        if let Some(v) = v {
            *volume.get_or_insert(0.0) += v;
        }
        if let Some(p) = label(r, "produto").map(|p| p.round() as i64) {
            let e = por_produto.entry(p).or_default();
            e.lotes += 1;
            e.volume += v.unwrap_or(0.0);
            prod_leads.entry(p).or_default().push(lead);
        }
    }
    for (p, x) in por_produto.iter_mut() {
        x.atravessamento = mean(prod_leads.get(p).map(Vec::as_slice).unwrap_or(&[]));
    }

    let mut reatores = BTreeMap::new();
    let mut ocupacao = BTreeMap::new();
    for (b, o) in model.objects.iter().enumerate() {
        let Some(rep) = report.blocks.get(b) else {
            continue;
        };
        let f = |k: &str| rep.states.get(k).copied().unwrap_or(0.0);
        let planned = 1.0 - f("off_shift");
        if planned <= 1e-9 {
            continue;
        }
        if let ObjectKind::Transporter { .. } = o.kind {
            let busy = f("utilize") + f("travel_empty") + f("travel_loaded");
            ocupacao.insert(o.name.clone(), busy / planned);
            continue;
        }
        if !e_reator(o) {
            continue;
        }
        reatores.insert(
            o.name.clone(),
            Reator {
                produzindo: f("processing") / planned,
                em_limpeza: f("setup") / planned,
                quebrado: f("down") / planned,
                bloqueado: f("blocked") / planned,
                sem_lote: f("idle") / planned,
                lotes: rep.output,
                ciclo: rep.staytime.mean,
                limpezas_dia: rep.custom.get("changeovers").map(|c| c / days),
            },
        );
    }

    let mut tanques = BTreeMap::new();
    if let Some(def) = &model.fluids
        && let Some(rep) = report.block(&def.name)
    {
        for n in &def.nodes {
            if let FluidKind::Tank { capacity, .. } = n.kind
                && capacity > 0.0
                && let Some(m) = rep.custom.get(&format!("{}.nivel_medio", n.name))
            {
                tanques.insert(n.name.clone(), m / capacity);
            }
        }
    }

    let n = leads.len();
    Replicacao {
        lotes: n,
        lotes_dia: n as f64 / days,
        fora,
        envasadas_dia: (!fillers.is_empty()).then(|| envasadas as f64 / days),
        volume_dia: volume.map(|v| v / days),
        rendimento: (!rendimentos.is_empty()).then(|| mean(&rendimentos)),
        atravessamento: mean(&leads),
        por_produto,
        reatores,
        tanques,
        ocupacao,
    }
}

impl Processos {
    /// Runs every replication with the per-item metrics.
    pub fn rodar(model: &Model, pack: Option<&Pack>, meta: Meta) -> Result<Processos, ModelError> {
        model.validate()?;
        let mut reps = Vec::new();
        let mut reports = Vec::new();
        for r in 0..model.run.replications {
            let mut sim = model.build(r);
            sim.enable_item_log();
            let report = model.drive(&mut sim)?;
            let items = sim.take_item_log();
            reps.push(replicacao(model, &report, &items));
            reports.push(report);
        }
        Ok(Processos {
            modelo: model.name.clone(),
            linha: pack.map_or_else(
                || "Processos contínuos e batelada".to_string(),
                |p| p.library.title(),
            ),
            meta,
            reps,
            reports,
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
