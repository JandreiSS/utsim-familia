//! Indicators of the line Alimentação e food service (`docs/plano_linhas_especializadas.md`,
//! section 12): orders delivered per day and per hour of the day (the peak), order time
//! (creation to exit, mean and 90th percentile), orders within the target time, customers
//! who gave up, food thrown away at the pickup counter, the same by channel, the time of
//! every kitchen station, the occupancy and turnover of the tables and the occupancy of the
//! staff.
//!
//! An order is an item that leaves through a sink. A comanda (`SplitBy`) splits it into
//! parts (copies with the order's creation time) that a gathering queue puts together
//! again: one part leaves as the order with the others packed inside, and those end with
//! it without a block, so each order counts once. An item whose patience ran out (label
//! `abandonou`) gave up, unless it was waiting at a pickup counter (preset
//! `alimentacao/retirada`): there the food got cold and was thrown away. Its labels:
//! `canal` (1 counter, 2 drive-thru, 3 delivery; 1 without the label) and `itens`.

pub mod relatorio;

use std::collections::BTreeMap;

use utsim_core::stats::Summary;
use utsim_core::{ItemRecord, RunReport, Value};
use utsim_model::{Model, ModelError, ObjectKind};

use crate::pacote::Pack;

pub use crate::registro::ALIMENTACAO;

/// Minutes in a day.
const DAY: f64 = 1440.0;

/// Target of the line: the order time within which an order is on time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    /// Minutes.
    pub prazo: f64,
}

impl Default for Meta {
    /// 15 minutes.
    fn default() -> Self {
        Meta { prazo: 15.0 }
    }
}

/// Orders of one channel in a replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Canal {
    pub pedidos: usize,
    /// Mean order time.
    pub tempo: f64,
    /// Fraction within the target.
    pub no_prazo: f64,
    pub desistencias: usize,
    pub descartes: usize,
}

/// Time of one station (fractions of its shift, or of the opening hours of the model, the
/// time some source takes orders, when it has no shift of its own).
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Estacao {
    /// Mean places working ÷ places (the occupancy of a station with several places).
    pub ocupacao: f64,
    /// With work in at least one place (the core state `processing`).
    pub preparando: f64,
    pub parada: f64,
    pub bloqueada: f64,
    pub quebrada: f64,
    /// Parts (or orders) that left the station.
    pub feitas: u64,
    /// Mean time of a part inside the station.
    pub tempo: f64,
}

/// A dining room: how full the tables are and how many times each one turns.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Mesas {
    pub mesas: usize,
    /// Mean tables taken ÷ tables, over its shift (or the opening hours).
    pub ocupacao: f64,
    /// Groups seated per table per day.
    pub giro_dia: f64,
    /// Mean time at the table.
    pub refeicao: f64,
}

/// Indicators of one replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Replicacao {
    /// Orders delivered in the window.
    pub pedidos: usize,
    pub pedidos_dia: f64,
    /// Mean order time (creation to exit).
    pub tempo: f64,
    /// 90th percentile of the order time.
    pub tempo_p90: f64,
    /// Fraction of the orders within the target.
    pub no_prazo: f64,
    /// Customers who gave up waiting.
    pub desistencias: usize,
    /// Gave up ÷ (delivered + gave up + thrown away).
    pub desistencia: f64,
    /// Orders thrown away at a pickup counter (cold).
    pub descartes: usize,
    /// Orders per day that arrived in each hour of the day (`(hour, orders)`, open hours).
    pub por_hora: Vec<(usize, f64)>,
    pub por_canal: BTreeMap<i64, Canal>,
    pub estacoes: BTreeMap<String, Estacao>,
    pub mesas: BTreeMap<String, Mesas>,
    /// People: fraction of the on-shift time busy (walking or working).
    pub ocupacao: BTreeMap<String, f64>,
}

impl Replicacao {
    /// The busiest hour: `(hour, orders that arrived in it per day)`.
    pub fn pico(&self) -> Option<(usize, f64)> {
        self.por_hora
            .iter()
            .copied()
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
    }
}

/// The indicators of a set of replications.
#[derive(Clone, Debug)]
pub struct Alimentacao {
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

/// The `q` quantile (nearest rank) of `xs`; 0 for none.
fn quantile(xs: &[f64], q: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let k = ((q * v.len() as f64).ceil() as usize).clamp(1, v.len());
    v[k - 1]
}

fn preset_is(o: &utsim_model::ObjectDef, id: &str) -> bool {
    o.preset.as_deref() == Some(id)
}

/// Whether a processor is a dining room for the indicators.
pub fn e_salao(o: &utsim_model::ObjectDef) -> bool {
    preset_is(o, "alimentacao/mesas")
}

/// Whether an object is a kitchen station, a cashier or a delivery for the indicators (a
/// processor that is not a dining room).
pub fn e_estacao(o: &utsim_model::ObjectDef) -> bool {
    matches!(o.kind, ObjectKind::Processor { .. }) && !e_salao(o)
}

/// The indicators of one replication from its report and item log.
pub fn replicacao(
    model: &Model,
    report: &RunReport,
    items: &[ItemRecord],
    meta: Meta,
) -> Replicacao {
    let from = report.stats_from;
    let end = report.end_time;
    let window = (end - from).max(f64::MIN_POSITIVE);
    let days = window / DAY;
    let is_sink = |b: usize| {
        model
            .objects
            .get(b)
            .is_some_and(|o| matches!(o.kind, ObjectKind::Sink))
    };
    let is_pickup = |b: usize| {
        model
            .objects
            .get(b)
            .is_some_and(|o| preset_is(o, "alimentacao/retirada"))
    };
    let canal = |r: &ItemRecord| label(r, "canal").map_or(1, |c| c.round() as i64);

    let mut tempos = Vec::new();
    let mut desistencias = 0usize;
    let mut descartes = 0usize;
    let mut por_canal: BTreeMap<i64, Canal> = BTreeMap::new();
    let mut tempos_canal: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    let mut hours = [0usize; 24];
    for r in items {
        let Some(t_end) = r.ended_at.filter(|t| *t >= from) else {
            continue;
        };
        let c = canal(r);
        if r.labels.contains_key(utsim_objects::ABANDONED_LABEL) {
            if r.block.is_some_and(is_pickup) {
                descartes += 1;
                por_canal.entry(c).or_default().descartes += 1;
            } else {
                desistencias += 1;
                por_canal.entry(c).or_default().desistencias += 1;
            }
            continue;
        }
        if !r.block.is_some_and(is_sink) {
            continue;
        }
        let t = t_end - r.created_at;
        tempos.push(t);
        por_canal.entry(c).or_default().pedidos += 1;
        tempos_canal.entry(c).or_default().push(t);
        if r.created_at >= from {
            let h = (r.created_at.rem_euclid(DAY) / 60.0).floor() as usize % 24;
            hours[h] += 1;
        }
    }
    let within = |xs: &[f64]| {
        if xs.is_empty() {
            0.0
        } else {
            xs.iter().filter(|t| **t <= meta.prazo + 1e-9).count() as f64 / xs.len() as f64
        }
    };
    for (c, x) in por_canal.iter_mut() {
        let ts = tempos_canal.get(c).map(Vec::as_slice).unwrap_or(&[]);
        x.tempo = mean(ts);
        x.no_prazo = within(ts);
    }

    // The opening hours: the share of the window in which some source takes orders. A
    // station or a dining room without a shift of its own is measured over them.
    let loja = model
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| matches!(o.kind, ObjectKind::Source { .. }))
        .filter_map(|(b, _)| report.blocks.get(b))
        .map(|r| 1.0 - r.states.get("off_shift").copied().unwrap_or(0.0))
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))))
        .filter(|x| *x > 1e-9)
        .unwrap_or(1.0);
    let mut estacoes = BTreeMap::new();
    let mut mesas = BTreeMap::new();
    let mut ocupacao = BTreeMap::new();
    for (b, o) in model.objects.iter().enumerate() {
        let Some(rep) = report.blocks.get(b) else {
            continue;
        };
        let f = |k: &str| rep.states.get(k).copied().unwrap_or(0.0);
        let own = 1.0 - f("off_shift");
        if own <= 1e-9 {
            continue;
        }
        let open = if f("off_shift") > 1e-9 { own } else { loja };
        let frac = |x: f64| (x / open).min(1.0);
        match &o.kind {
            ObjectKind::Transporter { .. } => {
                let busy = f("utilize") + f("travel_empty") + f("travel_loaded");
                ocupacao.insert(o.name.clone(), busy / open);
            }
            ObjectKind::Processor { capacity, .. } if e_salao(o) => {
                let n = (*capacity).max(1);
                mesas.insert(
                    o.name.clone(),
                    Mesas {
                        mesas: n,
                        ocupacao: frac(rep.content_mean / n as f64),
                        giro_dia: rep.output as f64 / n as f64 / days,
                        refeicao: rep.staytime.mean,
                    },
                );
            }
            ObjectKind::Processor { capacity, .. } => {
                let n = (*capacity).max(1) as f64;
                let busy = rep.custom.get("servers.busy").copied().unwrap_or(0.0);
                estacoes.insert(
                    o.name.clone(),
                    Estacao {
                        ocupacao: frac(busy / n),
                        preparando: frac(f("processing")),
                        // Idle in the open hours: what is not idle nor off shift is work.
                        parada: (1.0 - frac(own - f("idle"))).max(0.0),
                        bloqueada: frac(f("blocked")),
                        quebrada: frac(f("down")),
                        feitas: rep.output,
                        tempo: rep.staytime.mean,
                    },
                );
            }
            _ => {}
        }
    }

    let n = tempos.len();
    let total = n + desistencias + descartes;
    Replicacao {
        pedidos: n,
        pedidos_dia: n as f64 / days,
        tempo: mean(&tempos),
        tempo_p90: quantile(&tempos, 0.9),
        no_prazo: within(&tempos),
        desistencias,
        desistencia: if total == 0 {
            0.0
        } else {
            desistencias as f64 / total as f64
        },
        descartes,
        por_hora: hours
            .iter()
            .enumerate()
            .filter(|(_, k)| **k > 0)
            .map(|(h, k)| (h, *k as f64 / days))
            .collect(),
        por_canal,
        estacoes,
        mesas,
        ocupacao,
    }
}

impl Alimentacao {
    /// Runs every replication with the per-item metrics.
    pub fn rodar(
        model: &Model,
        pack: Option<&Pack>,
        meta: Meta,
    ) -> Result<Alimentacao, ModelError> {
        model.validate()?;
        let mut reps = Vec::new();
        let mut reports = Vec::new();
        for r in 0..model.run.replications {
            let mut sim = model.build(r);
            sim.enable_item_log();
            let report = model.drive(&mut sim)?;
            let items = sim.take_item_log();
            reps.push(replicacao(model, &report, &items, meta));
            reports.push(report);
        }
        Ok(Alimentacao {
            modelo: model.name.clone(),
            linha: pack.map_or_else(
                || "Alimentação e food service".to_string(),
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
