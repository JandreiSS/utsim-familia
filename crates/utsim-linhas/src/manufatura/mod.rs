//! Indicators of the line Manufatura discreta (`docs/plano_linhas_especializadas.md`,
//! section 10): orders per day, lead time, work in process, orders on time and the mean
//! lateness of the late ones, the same by product, the OEE of every machine with its three
//! factors, changeovers per day and the occupancy of the operators.
//!
//! An order is an item that leaves through a sink; the cards of a card loop (label
//! `cartao`) are not orders, and an order that ends in a sink with `sink_role: Scrap` is
//! scrap. The due date is the label `vence` (written by the order queue) or, without it,
//! the creation plus the label `prazo`. The machines are the processors and assembly
//! stations with a process time; inspection and rework stations and the card loop are not.
//!
//! OEE (decisions M-D4 and M-D5, proposals): availability = (shift time − down − in
//! changeover) ÷ shift time; performance = ideal cycle × parts ÷ that running time, with the
//! ideal cycle the mean of `process` ÷ capacity (so a machine starved or blocked loses
//! performance); quality = good orders ÷ (good + scrap) of the whole model.

pub mod relatorio;

use std::collections::BTreeMap;

use utsim_core::stats::Summary;
use utsim_core::{ItemRecord, RunReport, Value};
use utsim_model::quality::SinkRole;
use utsim_model::{Model, ModelError, ObjectKind};

use crate::pacote::Pack;

pub use crate::registro::MANUFATURA;

/// Minutes in a day.
const DAY: f64 = 1440.0;

/// Label of the cards of a card loop.
pub const CARTAO: &str = "cartao";

/// Target of the line: the OEE above which a machine is on target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    pub oee: f64,
}

impl Default for Meta {
    /// 85 %, the usual reference for a good OEE.
    fn default() -> Self {
        Meta { oee: 0.85 }
    }
}

/// OEE of one machine in a replication (fractions).
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Oee {
    pub oee: f64,
    pub disponibilidade: f64,
    pub desempenho: f64,
    pub qualidade: f64,
    /// Fraction of the shift time in changeover (setup).
    pub em_troca: f64,
    /// Changeovers per day (`None` without `changeover`).
    pub trocas_dia: Option<f64>,
}

/// Results of one product in a replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Produto {
    /// Good orders that left.
    pub ordens: usize,
    /// Mean lead time.
    pub atravessamento: f64,
    /// Fraction on time (`None`: no order with a due date).
    pub no_prazo: Option<f64>,
}

/// Indicators of one replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Replicacao {
    /// Good orders that left in the window.
    pub ordens: usize,
    pub por_dia: f64,
    /// Orders that ended as scrap.
    pub refugo: usize,
    /// Mean lead time of the good orders (creation to exit).
    pub atravessamento: f64,
    /// Mean parts and orders in process (cards excluded).
    pub wip: f64,
    /// Fraction of the good orders with a due date that left on time.
    pub no_prazo: Option<f64>,
    /// Mean lateness of the late orders (`None`: none late).
    pub atraso: Option<f64>,
    pub por_produto: BTreeMap<i64, Produto>,
    pub maquinas: BTreeMap<String, Oee>,
    /// People: fraction of the on-shift time busy (walking or working).
    pub ocupacao: BTreeMap<String, f64>,
}

/// The indicators of a set of replications.
#[derive(Clone, Debug)]
pub struct Manufatura {
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

fn fracao(xs: &[bool]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.iter().filter(|x| **x).count() as f64 / xs.len() as f64)
}

/// The ideal cycle of a machine (mean process time per part), or `None` when the object is
/// not a machine for the OEE.
pub fn ciclo_ideal(o: &utsim_model::ObjectDef) -> Option<f64> {
    if o.inspection.is_some()
        || o.rework.is_some()
        || o.preset.as_deref().is_some_and(|p| {
            p == "manufatura/cartoes"
                || p.starts_with("qualidade/estacao")
                || p == "qualidade/inspecao_medicao"
                || p == "qualidade/retrabalho"
        })
    {
        return None;
    }
    let ideal = match &o.kind {
        ObjectKind::Processor {
            capacity, process, ..
        } => process.mean() / (*capacity).max(1) as f64,
        ObjectKind::Combiner { process, .. } => process.mean(),
        _ => return None,
    };
    (ideal.is_finite() && ideal > 0.0).then_some(ideal)
}

/// The indicators of one replication from its report and item log.
pub fn replicacao(model: &Model, report: &RunReport, items: &[ItemRecord]) -> Replicacao {
    let from = report.stats_from;
    let end = report.end_time;
    let window = (end - from).max(f64::MIN_POSITIVE);
    let sink_role = |b: usize| -> Option<Option<SinkRole>> {
        let o = model.objects.get(b)?;
        matches!(o.kind, ObjectKind::Sink).then_some(o.sink_role)
    };

    let mut wip = 0.0;
    let mut leads = Vec::new();
    let mut prazo_ok = Vec::new();
    let mut atrasos = Vec::new();
    let mut refugo = 0usize;
    let mut por_produto: BTreeMap<i64, Produto> = BTreeMap::new();
    let mut prod_leads: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    let mut prod_prazo: BTreeMap<i64, Vec<bool>> = BTreeMap::new();
    for r in items {
        if r.labels.contains_key(CARTAO) {
            continue;
        }
        // Work in process: the time the part or order was in the system inside the window.
        let t1 = r.ended_at.unwrap_or(end).min(end);
        let t0 = r.created_at.max(from);
        if t1 > t0 {
            wip += t1 - t0;
        }
        let Some(t_end) = r.ended_at.filter(|t| *t >= from) else {
            continue;
        };
        let Some(role) = r.block.and_then(sink_role) else {
            // Ended elsewhere: a component joined in an assembly.
            continue;
        };
        if role == Some(SinkRole::Scrap) {
            refugo += 1;
            continue;
        }
        let lead = t_end - r.created_at;
        leads.push(lead);
        let produto = label(r, "produto").map(|p| p.round() as i64);
        if let Some(p) = produto {
            por_produto.entry(p).or_default().ordens += 1;
            prod_leads.entry(p).or_default().push(lead);
        }
        let due = label(r, "vence").or_else(|| label(r, "prazo").map(|p| r.created_at + p));
        if let Some(due) = due {
            let ok = t_end <= due + 1e-9;
            prazo_ok.push(ok);
            if !ok {
                atrasos.push(t_end - due);
            }
            if let Some(p) = produto {
                prod_prazo.entry(p).or_default().push(ok);
            }
        }
    }
    for (p, x) in por_produto.iter_mut() {
        x.atravessamento = mean(prod_leads.get(p).map(Vec::as_slice).unwrap_or(&[]));
        x.no_prazo = prod_prazo.get(p).and_then(|v| fracao(v));
    }
    let n = leads.len();
    let qualidade = if n + refugo == 0 {
        1.0
    } else {
        n as f64 / (n + refugo) as f64
    };

    let mut maquinas = BTreeMap::new();
    let mut ocupacao = BTreeMap::new();
    for (b, o) in model.objects.iter().enumerate() {
        let Some(rep) = report.blocks.get(b) else {
            continue;
        };
        let f = |k: &str| rep.states.get(k).copied().unwrap_or(0.0);
        if let ObjectKind::Transporter { .. } = o.kind {
            let on = 1.0 - f("off_shift");
            if on > 1e-9 {
                let busy = f("utilize") + f("travel_empty") + f("travel_loaded");
                ocupacao.insert(o.name.clone(), busy / on);
            }
            continue;
        }
        let Some(ideal) = ciclo_ideal(o) else {
            continue;
        };
        let planned = 1.0 - f("off_shift");
        if planned <= 1e-9 {
            continue;
        }
        let running = (planned - f("down") - f("setup")).max(0.0);
        let disponibilidade = (running / planned).clamp(0.0, 1.0);
        let desempenho = if running > 1e-9 {
            (ideal * rep.output as f64 / (running * window)).min(1.0)
        } else {
            0.0
        };
        maquinas.insert(
            o.name.clone(),
            Oee {
                oee: disponibilidade * desempenho * qualidade,
                disponibilidade,
                desempenho,
                qualidade,
                em_troca: f("setup") / planned,
                trocas_dia: rep.custom.get("changeovers").map(|c| c / (window / DAY)),
            },
        );
    }

    Replicacao {
        ordens: n,
        por_dia: n as f64 / (window / DAY),
        refugo,
        atravessamento: mean(&leads),
        wip: wip / window,
        no_prazo: fracao(&prazo_ok),
        atraso: (!atrasos.is_empty()).then(|| mean(&atrasos)),
        por_produto,
        maquinas,
        ocupacao,
    }
}

impl Manufatura {
    /// Runs every replication with the per-item metrics.
    pub fn rodar(model: &Model, pack: Option<&Pack>, meta: Meta) -> Result<Manufatura, ModelError> {
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
        Ok(Manufatura {
            modelo: model.name.clone(),
            linha: pack.map_or_else(|| "Manufatura discreta".to_string(), |p| p.library.title()),
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
