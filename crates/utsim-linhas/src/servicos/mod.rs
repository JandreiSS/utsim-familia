//! Indicators of the line Serviços e atendimento (`docs/plano_linhas_especializadas.md`,
//! section 7): customers served and abandoned, waiting time and service time, the service
//! level (fraction served within a target wait), results by ticket type and by queue, the
//! occupancy of the attendants, the call panel of replication 0 and the staffing by hour of
//! the day (Erlang C, M/M/c without abandonment).
//!
//! A customer is an item. Its wait is the time it spent in queues (`Queue`); its service
//! time is the work drawn for it (processors). A customer that gave up has the label
//! `abandonou` (the queue's `patience`). The ticket type is the numeric label `tipo`
//! (1 normal, 2 preferencial, 3 agendado; any other number is shown as `T<n>`).

pub mod relatorio;

use std::collections::BTreeMap;

use utsim_core::stats::Summary;
use utsim_core::{ItemRecord, RunReport, TraceEvent, TraceRecord, Value};
use utsim_model::{Model, ModelError, ObjectKind};

use crate::pacote::Pack;

pub use crate::registro::SERVICOS;

/// Minutes in a day.
const DAY: f64 = 1440.0;

/// Calls shown in the call panel.
pub const PANEL_CALLS: usize = 40;

/// The service level target: a customer is served "in time" when it waited at most
/// `wait` minutes; the service is on target when at least `share` of them were.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    pub wait: f64,
    pub share: f64,
}

impl Default for Meta {
    /// 80 % within 15 minutes (decision S-D2, proposal).
    fn default() -> Self {
        Meta {
            wait: 15.0,
            share: 0.8,
        }
    }
}

/// Results of one ticket type or one queue in a replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Grupo {
    pub atendidos: usize,
    pub abandonos: usize,
    /// Mean wait of the customers served.
    pub espera: f64,
    /// Fraction of the customers served that waited at most the target.
    pub nivel: f64,
}

/// Indicators of one replication.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Replicacao {
    pub atendidos: usize,
    pub abandonos: usize,
    /// Abandoned ÷ (served + abandoned).
    pub abandono: f64,
    /// Mean wait of the customers served (TME).
    pub espera: f64,
    /// 90th percentile of that wait.
    pub espera_p90: f64,
    /// Mean service time (TMA).
    pub atendimento: f64,
    /// Fraction served within the target wait.
    pub nivel: f64,
    /// Mean time waited by those who gave up.
    pub espera_abandono: Option<f64>,
    /// Customers served per day.
    pub por_dia: f64,
    pub por_tipo: BTreeMap<i64, Grupo>,
    /// By the first queue the customer waited in.
    pub por_fila: BTreeMap<String, Grupo>,
    /// Attendants: fraction of the on-shift time spent serving (or turning between
    /// channels).
    pub ocupacao: BTreeMap<String, f64>,
    /// Waits of the customers served (histogram).
    pub esperas: Vec<f64>,
}

/// One call of the panel.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Chamada {
    pub instante: f64,
    /// `N007`: the type letter and the number of the customer among those of its type.
    pub senha: String,
    /// The desk (processor) that called it.
    pub guiche: String,
    /// How long it had waited.
    pub espera: f64,
}

/// Staffing of one hour of the day.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Hora {
    pub hora: usize,
    /// Mean arrivals in this hour, per day with arrivals in it.
    pub chegadas: f64,
    /// Attendants Erlang C asks for to meet the target (`None`: no service time known).
    pub necessarios: Option<usize>,
    /// Simulated fraction within the target of the customers that arrived in this hour.
    pub nivel: Option<f64>,
}

/// The indicators of a set of replications.
#[derive(Clone, Debug)]
pub struct Atendimento {
    pub modelo: String,
    pub linha: String,
    pub meta: Meta,
    pub reps: Vec<Replicacao>,
    pub reports: Vec<RunReport>,
    pub painel: Vec<Chamada>,
    pub horas: Vec<Hora>,
}

fn label_num(r: &ItemRecord, name: &str) -> Option<f64> {
    match r.labels.get(name) {
        Some(Value::Num(v)) if v.is_finite() => Some(*v),
        _ => None,
    }
}

fn gave_up(r: &ItemRecord) -> bool {
    r.labels.contains_key(utsim_objects::ABANDONED_LABEL)
}

fn tipo(r: &ItemRecord) -> i64 {
    label_num(r, "tipo").map_or(1, |v| v.round() as i64)
}

/// The type letter of a ticket.
pub fn letra(tipo: i64) -> String {
    match tipo {
        1 => "N".into(),
        2 => "P".into(),
        3 => "A".into(),
        n => format!("T{n}"),
    }
}

fn is_queue(model: &Model, b: usize) -> bool {
    matches!(
        model.objects.get(b).map(|o| &o.kind),
        Some(ObjectKind::Queue { .. })
    )
}

/// Time `r` waited: its time in the model minus the service drawn for it. Besides the
/// queues, it counts the wait at a desk for an attendant and the time a contact was left
/// aside by an interruption, as the customer feels them.
fn espera(_model: &Model, r: &ItemRecord) -> f64 {
    let Some(end) = r.ended_at else {
        return 0.0;
    };
    (end - r.created_at - r.total_work()).max(0.0)
}

fn percentile(xs: &[f64], p: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let k = ((p * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1;
    v[k]
}

/// The indicators of one replication.
pub fn replicacao(
    model: &Model,
    report: &RunReport,
    items: &[ItemRecord],
    meta: Meta,
) -> Replicacao {
    let from = report.stats_from;
    let ended: Vec<&ItemRecord> = items
        .iter()
        .filter(|r| r.ended_at.is_some_and(|t| t >= from))
        .collect();
    let served: Vec<&&ItemRecord> = ended.iter().filter(|r| !gave_up(r)).collect();
    let abandoned: Vec<&&ItemRecord> = ended.iter().filter(|r| gave_up(r)).collect();
    let esperas: Vec<f64> = served.iter().map(|r| espera(model, r)).collect();
    let n = served.len();
    let mean = |xs: &[f64]| {
        if xs.is_empty() {
            0.0
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    let within = |xs: &[f64]| {
        if xs.is_empty() {
            0.0
        } else {
            xs.iter().filter(|w| **w <= meta.wait + 1e-9).count() as f64 / xs.len() as f64
        }
    };
    let atend: Vec<f64> = served.iter().map(|r| r.total_work()).collect();
    let mut por_tipo: BTreeMap<i64, Grupo> = BTreeMap::new();
    let mut por_fila: BTreeMap<String, Grupo> = BTreeMap::new();
    let mut waits_tipo: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    let mut waits_fila: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let first_queue = |r: &ItemRecord| {
        r.visits
            .keys()
            .copied()
            .filter(|b| is_queue(model, *b))
            .min()
            .and_then(|b| model.objects.get(b))
            .map(|o| o.name.clone())
    };
    for r in &ended {
        let t = tipo(r);
        let q = first_queue(r);
        if gave_up(r) {
            por_tipo.entry(t).or_default().abandonos += 1;
            if let Some(q) = &q {
                por_fila.entry(q.clone()).or_default().abandonos += 1;
            }
        } else {
            let w = espera(model, r);
            por_tipo.entry(t).or_default().atendidos += 1;
            waits_tipo.entry(t).or_default().push(w);
            if let Some(q) = q {
                por_fila.entry(q.clone()).or_default().atendidos += 1;
                waits_fila.entry(q).or_default().push(w);
            }
        }
    }
    for (t, g) in por_tipo.iter_mut() {
        let ws = waits_tipo.get(t).map(Vec::as_slice).unwrap_or(&[]);
        g.espera = mean(ws);
        g.nivel = within(ws);
    }
    for (q, g) in por_fila.iter_mut() {
        let ws = waits_fila.get(q).map(Vec::as_slice).unwrap_or(&[]);
        g.espera = mean(ws);
        g.nivel = within(ws);
    }
    let ocupacao = model
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| matches!(o.kind, ObjectKind::Transporter { .. }))
        .filter_map(|(i, o)| {
            let b = report.blocks.get(i)?;
            let f = |k: &str| b.states.get(k).copied().unwrap_or(0.0);
            let on = 1.0 - f("off_shift");
            (on > 1e-9).then(|| (o.name.clone(), (f("utilize") + f("setup")) / on))
        })
        .collect();
    let window = (report.end_time - from).max(f64::MIN_POSITIVE);
    let total = n + abandoned.len();
    Replicacao {
        atendidos: n,
        abandonos: abandoned.len(),
        abandono: if total == 0 {
            0.0
        } else {
            abandoned.len() as f64 / total as f64
        },
        espera: mean(&esperas),
        espera_p90: percentile(&esperas, 0.9),
        atendimento: mean(&atend),
        nivel: within(&esperas),
        espera_abandono: (!abandoned.is_empty()).then(|| {
            mean(
                &abandoned
                    .iter()
                    .map(|r| espera(model, r))
                    .collect::<Vec<_>>(),
            )
        }),
        por_dia: n as f64 / (window / DAY),
        por_tipo,
        por_fila,
        ocupacao,
        esperas,
    }
}

/// The calls of the panel: a customer moving from a queue into a processor, in order,
/// from the start of the statistics window (the first `PANEL_CALLS`).
pub fn painel(
    model: &Model,
    trace: &[TraceRecord],
    items: &[ItemRecord],
    from: f64,
) -> Vec<Chamada> {
    // The ticket numbers: by order of arrival within each type.
    let mut sorted: Vec<&ItemRecord> = items.iter().collect();
    sorted.sort_by(|a, b| {
        a.created_at
            .total_cmp(&b.created_at)
            .then(a.key.cmp(&b.key))
    });
    let mut count: BTreeMap<i64, usize> = BTreeMap::new();
    let mut senha: BTreeMap<u64, (String, f64)> = BTreeMap::new();
    for r in sorted {
        let t = tipo(r);
        let k = count.entry(t).or_default();
        *k += 1;
        senha.insert(r.key, (format!("{}{:03}", letra(t), k), r.created_at));
    }
    trace
        .iter()
        .filter(|r| r.t >= from)
        .filter_map(|r| match r.event {
            TraceEvent::Moved { item, from: a, to }
                if is_queue(model, a)
                    && matches!(
                        model.objects.get(to).map(|o| &o.kind),
                        Some(ObjectKind::Processor { .. })
                    ) =>
            {
                let (s, created) = senha.get(&item)?.clone();
                Some(Chamada {
                    instante: r.t,
                    senha: s,
                    guiche: model.objects[to].name.clone(),
                    espera: r.t - created,
                })
            }
            _ => None,
        })
        .take(PANEL_CALLS)
        .collect()
}

/// Probability that a customer waits (Erlang C) with `c` servers and offered load `a`.
fn erlang_c(c: usize, a: f64) -> f64 {
    if c == 0 || a >= c as f64 {
        return 1.0;
    }
    // Sum of a^k / k! for k < c, and a^c / c!, built term by term.
    let mut term = 1.0;
    let mut sum = 0.0;
    for k in 0..c {
        sum += term;
        term *= a / (k + 1) as f64;
    }
    let last = term * c as f64 / (c as f64 - a);
    last / (sum + last)
}

/// Fewest servers that serve a fraction `share` within `wait`, for arrivals at `lambda` per
/// minute and mean service `tma` minutes (M/M/c).
pub fn necessarios(lambda: f64, tma: f64, meta: Meta) -> usize {
    if lambda <= 0.0 {
        return 0;
    }
    let a = lambda * tma;
    let mut c = (a.floor() as usize) + 1;
    loop {
        let level = 1.0 - erlang_c(c, a) * (-(c as f64 - a) * meta.wait / tma).exp();
        if level >= meta.share || c > 500 {
            return c;
        }
        c += 1;
    }
}

/// Arrivals and simulated service level by hour of the day over all replications, and the
/// staff Erlang C asks for.
fn horas(model: &Model, all: &[(f64, Vec<ItemRecord>)], tma: f64, meta: Meta) -> Vec<Hora> {
    let mut arrivals = [0usize; 24];
    let mut days: [std::collections::BTreeSet<(usize, i64)>; 24] = Default::default();
    let mut waits: [Vec<f64>; 24] = Default::default();
    for (r, (from, items)) in all.iter().enumerate() {
        for it in items.iter().filter(|it| it.created_at >= *from) {
            let h = ((it.created_at.rem_euclid(DAY)) / 60.0).floor() as usize % 24;
            arrivals[h] += 1;
            days[h].insert((r, (it.created_at / DAY).floor() as i64));
            if it.ended_at.is_some() && !gave_up(it) {
                waits[h].push(espera(model, it));
            }
        }
    }
    (0..24)
        .filter(|h| arrivals[*h] > 0)
        .map(|h| {
            let per_day = arrivals[h] as f64 / days[h].len().max(1) as f64;
            let ws = &waits[h];
            Hora {
                hora: h,
                chegadas: per_day,
                necessarios: (tma > 0.0).then(|| necessarios(per_day / 60.0, tma, meta)),
                nivel: (!ws.is_empty()).then(|| {
                    ws.iter().filter(|w| **w <= meta.wait + 1e-9).count() as f64 / ws.len() as f64
                }),
            }
        })
        .collect()
}

impl Atendimento {
    /// Runs every replication with the per-item metrics (and the trace in replication 0).
    pub fn rodar(
        model: &Model,
        pack: Option<&Pack>,
        meta: Meta,
    ) -> Result<Atendimento, ModelError> {
        model.validate()?;
        let mut reps = Vec::new();
        let mut reports = Vec::new();
        let mut all = Vec::new();
        let mut painel_0 = Vec::new();
        for r in 0..model.run.replications {
            let mut sim = model.build(r);
            sim.enable_item_log();
            if r == 0 {
                sim.enable_trace();
            }
            let report = model.drive(&mut sim)?;
            let items = sim.take_item_log();
            if r == 0 {
                painel_0 = painel(model, &sim.take_trace(), &items, report.stats_from);
            }
            reps.push(replicacao(model, &report, &items, meta));
            all.push((report.stats_from, items));
            reports.push(report);
        }
        let served: usize = reps.iter().map(|r| r.atendidos).sum();
        let tma = if served == 0 {
            0.0
        } else {
            reps.iter()
                .map(|r| r.atendimento * r.atendidos as f64)
                .sum::<f64>()
                / served as f64
        };
        let horas = horas(model, &all, tma, meta);
        Ok(Atendimento {
            modelo: model.name.clone(),
            linha: pack.map_or_else(
                || "Serviços e atendimento".to_string(),
                |p| p.library.title(),
            ),
            meta,
            reps,
            reports,
            painel: painel_0,
            horas,
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
