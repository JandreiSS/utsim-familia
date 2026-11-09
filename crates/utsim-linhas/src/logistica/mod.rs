//! Indicators of the line Logística e armazém (`docs/plano_linhas_especializadas.md`,
//! section 8): receiving (trucks at the docks: time in the yard and at the dock), picking
//! (lines per hour, also by hour of the day), shipping (orders per day, cycle time, lines
//! per order, on time), storage (rack occupancy, access time by ABC class) and the teams.
//!
//! The objects are recognised by the preset that made them (`logistica/doca`,
//! `logistica/separacao`, `logistica/expedicao`, `logistica/porta_paletes`) or, without one,
//! by their kind: a separator splitting by `paletes` is a dock, a processor that receives
//! lines (label `linha`) is picking, a sink that receives orders (label `pedido`) is
//! shipping. Dock times come from the trace of every replication.

pub mod relatorio;

use std::collections::{BTreeMap, BTreeSet};

use utsim_core::stats::Summary;
use utsim_core::{ItemRecord, RunReport, TraceEvent, TraceRecord, Value};
use utsim_model::{Model, ModelError, ObjectKind};
use utsim_objects::SeparateMode;

use crate::pacote::Pack;

pub use crate::registro::LOGISTICA;

/// Minutes in a day.
const DAY: f64 = 1440.0;

/// Target of the dock time: a truck is on target when it left the dock within `doca`
/// minutes of arriving at it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    pub doca: f64,
}

impl Default for Meta {
    /// 60 minutes (decision G-D5, proposal).
    fn default() -> Self {
        Meta { doca: 60.0 }
    }
}

/// Indicators of one replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Replicacao {
    /// Trucks that left the dock in the window.
    pub caminhoes: usize,
    /// Mean wait in the yard (arrival to dock).
    pub patio: f64,
    /// Mean time at the dock (arrival at the dock to the last pallet out).
    pub doca: f64,
    /// Fraction of the trucks within the dock target.
    pub doca_na_meta: f64,
    pub paletes: usize,
    /// Lines picked per hour of the window.
    pub linhas_hora: f64,
    pub linhas: usize,
    pub pedidos: usize,
    pub pedidos_dia: f64,
    /// Mean order cycle time (entry to shipping).
    pub ciclo: f64,
    pub linhas_por_pedido: f64,
    /// Fraction of the orders with `sla` shipped within it.
    pub no_prazo: Option<f64>,
    /// Mean rack occupancy (content ÷ positions), by rack.
    pub porta_paletes: BTreeMap<String, f64>,
    /// Mean access time by ABC class ("A", "B", "C"), over the racks.
    pub acesso: BTreeMap<String, f64>,
    /// Forklifts and pickers: fraction of the on-shift time busy (moving, loading or
    /// working).
    pub ocupacao: BTreeMap<String, f64>,
    /// Docks: fraction of the time with a truck.
    pub docas: BTreeMap<String, f64>,
}

/// The indicators of a set of replications.
#[derive(Clone, Debug)]
pub struct Armazem {
    pub modelo: String,
    pub linha: String,
    pub meta: Meta,
    pub reps: Vec<Replicacao>,
    pub reports: Vec<RunReport>,
    /// Lines picked by hour of the day (mean per day with picking in that hour), all
    /// replications.
    pub linhas_por_hora: Vec<(usize, f64)>,
}

/// What each object is for the indicators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Papel {
    Doca,
    Separacao,
    Expedicao,
    PortaPaletes,
    Outro,
}

fn papel(model: &Model, b: usize) -> Papel {
    let Some(o) = model.objects.get(b) else {
        return Papel::Outro;
    };
    match o.preset.as_deref() {
        Some("logistica/doca") => return Papel::Doca,
        Some("logistica/separacao") => return Papel::Separacao,
        Some("logistica/expedicao") => return Papel::Expedicao,
        Some("logistica/porta_paletes") => return Papel::PortaPaletes,
        Some(p) if p.starts_with("logistica/") => return Papel::Outro,
        _ => {}
    }
    match &o.kind {
        ObjectKind::Separator {
            mode: SeparateMode::SplitBy { count, .. },
            ..
        } if count == "paletes" => Papel::Doca,
        ObjectKind::Rack { .. } => Papel::PortaPaletes,
        _ => Papel::Outro,
    }
}

fn label(r: &ItemRecord, name: &str) -> Option<f64> {
    match r.labels.get(name) {
        Some(Value::Num(v)) if v.is_finite() => Some(*v),
        _ => None,
    }
}

/// The indicators of one replication from its report, item log and trace.
pub fn replicacao(
    model: &Model,
    report: &RunReport,
    items: &[ItemRecord],
    trace: &[TraceRecord],
    meta: Meta,
) -> (Replicacao, Vec<(usize, f64)>) {
    let from = report.stats_from;
    let end = report.end_time;
    let window = (end - from).max(f64::MIN_POSITIVE);
    let papeis: Vec<Papel> = (0..model.objects.len()).map(|b| papel(model, b)).collect();
    let is = |b: usize, p: Papel| papeis.get(b) == Some(&p);
    let by_key: BTreeMap<u64, &ItemRecord> = items.iter().map(|r| (r.key, r)).collect();
    // A processor receiving lines is picking even without the preset.
    let picking: BTreeSet<usize> = (0..model.objects.len())
        .filter(|b| {
            is(*b, Papel::Separacao)
                || (matches!(model.objects[*b].kind, ObjectKind::Processor { .. })
                    && model.objects[*b].preset.is_none()
                    && items
                        .iter()
                        .any(|r| r.labels.contains_key("linha") && r.visits.contains_key(b)))
        })
        .collect();

    // Trucks: entry into a dock and the last pallet out of it, from the trace.
    let mut dock_in: BTreeMap<(usize, u64), f64> = BTreeMap::new();
    let mut dock_out: BTreeMap<(usize, u64), f64> = BTreeMap::new();
    let mut picked: Vec<f64> = Vec::new();
    for rec in trace {
        if let TraceEvent::Moved { item, from: a, to } = rec.event {
            if is(to, Papel::Doca) {
                dock_in.insert((to, item), rec.t);
            }
            if is(a, Papel::Doca) {
                if let Some(truck) = by_key.get(&item).and_then(|r| label(r, "caminhao")) {
                    let k = (a, truck.to_bits());
                    let t = dock_out.entry(k).or_insert(rec.t);
                    *t = t.max(rec.t);
                }
            }
            if picking.contains(&a) && rec.t >= from {
                picked.push(rec.t);
            }
        }
    }
    let mut patio = Vec::new();
    let mut doca = Vec::new();
    let mut paletes = 0usize;
    for ((b, item), t_in) in &dock_in {
        let Some(r) = by_key.get(item) else { continue };
        let Some(truck) = label(r, "caminhao") else {
            continue;
        };
        let Some(t_out) = dock_out.get(&(*b, truck.to_bits())) else {
            continue;
        };
        if *t_out < from {
            continue;
        }
        patio.push(t_in - r.created_at);
        doca.push(t_out - t_in);
        paletes += label(r, "paletes").map_or(1, |p| p.round().max(1.0) as usize);
    }
    let mean = |xs: &[f64]| {
        if xs.is_empty() {
            0.0
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    let doca_na_meta = if doca.is_empty() {
        0.0
    } else {
        doca.iter().filter(|d| **d <= meta.doca + 1e-9).count() as f64 / doca.len() as f64
    };

    // Orders shipped: items that ended in a shipping sink with an order number.
    let shipped: Vec<&ItemRecord> = items
        .iter()
        .filter(|r| r.ended_at.is_some_and(|t| t >= from))
        .filter(|r| r.labels.contains_key("pedido"))
        .filter(|r| {
            r.block.is_some_and(|b| {
                is(b, Papel::Expedicao)
                    || (matches!(model.objects[b].kind, ObjectKind::Sink)
                        && model.objects[b].preset.is_none())
            })
        })
        .collect();
    let ciclo: Vec<f64> = shipped
        .iter()
        .map(|r| r.ended_at.unwrap_or(r.created_at) - r.created_at)
        .collect();
    let lpp: Vec<f64> = shipped
        .iter()
        .map(|r| label(r, "linhas").unwrap_or(1.0))
        .collect();
    let with_sla: Vec<bool> = shipped
        .iter()
        .filter_map(|r| {
            let sla = label(r, "sla")?;
            Some(r.ended_at? - r.created_at <= sla + 1e-9)
        })
        .collect();

    // Picked lines by hour of the day.
    let mut hours = [0usize; 24];
    let mut days: [BTreeSet<i64>; 24] = Default::default();
    for t in &picked {
        let h = ((t.rem_euclid(DAY)) / 60.0).floor() as usize % 24;
        hours[h] += 1;
        days[h].insert((t / DAY).floor() as i64);
    }
    let por_hora: Vec<(usize, f64)> = (0..24)
        .filter(|h| hours[*h] > 0)
        .map(|h| (h, hours[h] as f64 / days[h].len().max(1) as f64))
        .collect();

    let mut porta_paletes = BTreeMap::new();
    let mut acesso_n: BTreeMap<String, (f64, u64)> = BTreeMap::new();
    let mut ocupacao = BTreeMap::new();
    let mut docas = BTreeMap::new();
    for (b, o) in model.objects.iter().enumerate() {
        let Some(rep) = report.blocks.get(b) else {
            continue;
        };
        let f = |k: &str| rep.states.get(k).copied().unwrap_or(0.0);
        match &o.kind {
            ObjectKind::Rack { bays, levels, .. } => {
                let cap = (bays * levels).max(1) as f64;
                porta_paletes.insert(o.name.clone(), rep.content_mean / cap);
                for c in ["A", "B", "C"] {
                    if let Some(t) = rep.extra.get(&format!("access_time.{c}")) {
                        let e = acesso_n.entry(c.to_string()).or_default();
                        e.0 += t.mean * t.count as f64;
                        e.1 += t.count;
                    }
                }
            }
            ObjectKind::Transporter { .. } => {
                let on = 1.0 - f("off_shift");
                if on > 1e-9 {
                    let busy = f("utilize")
                        + f("travel_empty")
                        + f("travel_loaded")
                        + f("loading")
                        + f("unloading");
                    ocupacao.insert(o.name.clone(), busy / on);
                }
            }
            _ if is(b, Papel::Doca) => {
                docas.insert(o.name.clone(), 1.0 - f("idle"));
            }
            _ => {}
        }
    }
    let acesso = acesso_n
        .into_iter()
        .filter(|(_, (_, n))| *n > 0)
        .map(|(c, (s, n))| (c, s / n as f64))
        .collect();
    (
        Replicacao {
            caminhoes: doca.len(),
            patio: mean(&patio),
            doca: mean(&doca),
            doca_na_meta,
            paletes,
            linhas_hora: picked.len() as f64 / (window / 60.0),
            linhas: picked.len(),
            pedidos: shipped.len(),
            pedidos_dia: shipped.len() as f64 / (window / DAY),
            ciclo: mean(&ciclo),
            linhas_por_pedido: mean(&lpp),
            no_prazo: (!with_sla.is_empty())
                .then(|| with_sla.iter().filter(|x| **x).count() as f64 / with_sla.len() as f64),
            porta_paletes,
            acesso,
            ocupacao,
            docas,
        },
        por_hora,
    )
}

impl Armazem {
    /// Runs every replication with the per-item metrics and the trace.
    pub fn rodar(model: &Model, pack: Option<&Pack>, meta: Meta) -> Result<Armazem, ModelError> {
        model.validate()?;
        let mut reps = Vec::new();
        let mut reports = Vec::new();
        let mut horas: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
        for r in 0..model.run.replications {
            let mut sim = model.build(r);
            sim.enable_item_log();
            sim.enable_trace();
            let report = model.drive(&mut sim)?;
            let items = sim.take_item_log();
            let trace = sim.take_trace();
            let (rep, por_hora) = replicacao(model, &report, &items, &trace, meta);
            for (h, v) in por_hora {
                horas.entry(h).or_default().push(v);
            }
            reps.push(rep);
            reports.push(report);
        }
        let linhas_por_hora = horas
            .into_iter()
            .map(|(h, v)| (h, v.iter().sum::<f64>() / v.len() as f64))
            .collect();
        Ok(Armazem {
            modelo: model.name.clone(),
            linha: pack.map_or_else(|| "Logística e armazém".to_string(), |p| p.library.title()),
            meta,
            reps,
            reports,
            linhas_por_hora,
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
