//! Indicators of the Lean Office line (`docs/plano_linhas_especializadas.md`, 1.5),
//! computed from the per-item metrics of the kernel and the run reports, with a 95 %
//! confidence interval between replications.
//!
//! Every indicator counts the cases that finished inside the statistics window (after
//! the warm-up), except the WIP, which is a time average over that window. The role of
//! each object comes from its `preset` (or, without one, from its kind), so the
//! indicators work on any model; the time unit of the line is the minute.
//!
//! | Indicator | Definition |
//! |---|---|
//! | lead time | finish − creation of the case |
//! | process time (P/T) | work drawn for the case in work steps (posts, approvals, checks, meetings); handovers and waits for outside information are not work |
//! | touch time | part of the work done by people (operators) |
//! | activity ratio | mean touch time ÷ mean lead time |
//! | %C&A of a step | 1 − errors made by the step (`erros_<step>` label) ÷ visits to it |
//! | rolled %C&A | product of the %C&A of the steps |
//! | FPY | fraction of cases finished without any loop and without a pending error (`erro`) |
//! | case WIP | time-average number of cases in the model |
//! | % within SLA | fraction of cases with label `sla` finished in at most `sla` minutes |
//! | cases per day | finished cases ÷ days of the window (1440 min) |
//! | people's occupancy | fraction of the time working on cases (`utilize`), and of the on-shift time |
//! | waiting by cause | mean time per case in each kind of wait (inbox, batch, meeting, handover, outside information, inside a step without work) |
//! | rework loops | most visits to one object − 1, per case |

pub mod diagramas;
pub mod relatorio;

use std::collections::BTreeMap;

use utsim_core::stats::Summary;
use utsim_core::{ItemRecord, RunReport, State, TraceEvent, Value};
use utsim_model::{Model, ModelError, ObjectKind};

use crate::pacote::{Pack, Role};

/// Minutes in a day: the time unit of the Lean Office line is the minute.
pub const DAY: f64 = 1440.0;

/// Points of the WIP series.
const WIP_POINTS: usize = 200;

/// Causes of waiting, in report order.
pub const CAUSES: [&str; 8] = [
    "na caixa de entrada",
    "na etapa, sem trabalho (pessoa ocupada, fora do expediente ou bloqueio)",
    "esperando informação de fora",
    "em tramitação",
    "esperando a reunião",
    "esperando completar o lote",
    "na entrada (sem vaga na etapa seguinte)",
    "com uma pessoa (levado de um lugar a outro)",
];

/// Role of an object: its preset's, or the one its kind plays.
pub fn role(model: &Model, pack: Option<&Pack>, index: usize) -> Option<Role> {
    let obj = model.objects.get(index)?;
    if let Some(p) = pack.and_then(|p| p.preset_of(obj)) {
        return Some(p.role);
    }
    Some(match &obj.kind {
        ObjectKind::Source { .. } => Role::Entry,
        ObjectKind::Queue { batch: Some(_), .. } => Role::Batch,
        ObjectKind::Queue { .. } | ObjectKind::Rack { .. } | ObjectKind::Conveyor { .. } => {
            Role::Inbox
        }
        ObjectKind::Processor { .. }
        | ObjectKind::Combiner { .. }
        | ObjectKind::Separator { .. } => Role::Work,
        ObjectKind::Sink => Role::Done,
        ObjectKind::Transporter { .. } => Role::Person,
        ObjectKind::Dispatcher { .. } => Role::Team,
    })
}

/// Cause of the time a case waits in an object of `role` (`None`: it is not a wait).
fn cause(role: Role) -> Option<usize> {
    match role {
        Role::Inbox => Some(0),
        Role::Work | Role::Approval | Role::Check => Some(1),
        Role::WaitInfo => Some(2),
        Role::Transit => Some(3),
        Role::Meeting => Some(4),
        Role::Batch => Some(5),
        Role::Entry => Some(6),
        Role::Person | Role::Team => Some(7),
        Role::Done => None,
    }
}

/// Time a person spent in each kind of state, as fractions of the window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ocupacao {
    pub trabalhando: f64,
    pub andando: f64,
    pub ocioso: f64,
    pub fora: f64,
    /// Working ÷ on-shift time (`None` when never on shift).
    pub no_expediente: Option<f64>,
}

/// Indicators of one replication.
#[derive(Clone, Debug, Default)]
pub struct Replicacao {
    pub concluidos: usize,
    pub lead_time: f64,
    pub processo: f64,
    pub touch: f64,
    pub razao_atividade: f64,
    pub fpy: f64,
    pub rolled_ca: Option<f64>,
    pub wip: f64,
    pub sla: Option<f64>,
    pub por_dia: f64,
    pub voltas: f64,
    /// Cases finished with 0, 1, 2 and 3 or more loops.
    pub voltas_dist: [usize; 4],
    /// %C&A by step (work steps that made at least one visit).
    pub ca: BTreeMap<String, f64>,
    /// Mean work and touch time per visit, by step.
    pub pt_etapa: BTreeMap<String, (f64, f64)>,
    /// Mean waiting time per case, by cause (index of [`CAUSES`]).
    pub espera: [f64; CAUSES.len()],
    pub ocupacao: BTreeMap<String, Ocupacao>,
    /// Lead time of every finished case (histogram).
    pub leads: Vec<f64>,
    /// SLA of the finished cases that have one.
    pub slas: Vec<f64>,
}

/// The indicators of a set of replications, plus the series of the first one.
#[derive(Clone, Debug)]
pub struct Indicadores {
    pub modelo: String,
    pub linha: String,
    pub reps: Vec<Replicacao>,
    pub reports: Vec<RunReport>,
    /// Cases in the model over time, replication 0: `(t, cases)`.
    pub serie_wip: Vec<(f64, f64)>,
    /// Occupancy of each person by hour of the day (0..24) in replication 0: working ÷
    /// on-shift time in that hour (`None`: never on shift in it).
    pub ocupacao_hora: BTreeMap<String, [Option<f64>; 24]>,
    /// Mean cases waiting, by object (content of queues and steps), over the replications.
    pub parados: Vec<(String, Summary)>,
    pub stats_from: f64,
    pub end_time: f64,
}

fn num_label(r: &ItemRecord, name: &str) -> Option<f64> {
    match r.labels.get(name) {
        Some(Value::Num(v)) if v.is_finite() => Some(*v),
        _ => None,
    }
}

/// The indicators of one replication from its report and per-item metrics.
pub fn replicacao(
    model: &Model,
    pack: Option<&Pack>,
    report: &RunReport,
    items: &[ItemRecord],
) -> Replicacao {
    let from = report.stats_from;
    let end = report.end_time;
    let roles: Vec<Option<Role>> = (0..model.objects.len())
        .map(|i| role(model, pack, i))
        .collect();
    let role_of = |b: usize| roles.get(b).copied().flatten();
    let done: Vec<&ItemRecord> = items
        .iter()
        .filter(|r| r.ended_at.is_some_and(|t| t >= from))
        .collect();
    let n = done.len();
    let mean = |f: &dyn Fn(&ItemRecord) -> f64| {
        if n == 0 {
            0.0
        } else {
            done.iter().map(|r| f(r)).sum::<f64>() / n as f64
        }
    };
    let lead = |r: &ItemRecord| r.ended_at.unwrap_or(r.created_at) - r.created_at;
    let work_in = |r: &ItemRecord| {
        r.work
            .iter()
            .filter(|(b, _)| role_of(**b).is_some_and(Role::is_work))
            .map(|(_, w)| w)
            .sum::<f64>()
    };
    let lead_time = mean(&lead);
    let processo = mean(&work_in);
    let touch = mean(&|r| r.total_touch());
    let erro = |r: &ItemRecord| num_label(r, "erro").is_some_and(|e| e >= 1.0);
    let fpy = if n == 0 {
        0.0
    } else {
        done.iter().filter(|r| r.loops() == 0 && !erro(r)).count() as f64 / n as f64
    };
    // %C&A of each work step, from the errors it left in the labels.
    let mut ca = BTreeMap::new();
    let mut pt_etapa = BTreeMap::new();
    for (b, obj) in model.objects.iter().enumerate() {
        let r = role_of(b);
        let visits: u32 = done
            .iter()
            .map(|x| x.visits.get(&b).copied().unwrap_or(0))
            .sum();
        if visits == 0 || !r.is_some_and(Role::is_work) {
            continue;
        }
        let work: f64 = done.iter().filter_map(|x| x.work.get(&b)).sum();
        let tt: f64 = done.iter().filter_map(|x| x.touch.get(&b)).sum();
        pt_etapa.insert(obj.name.clone(), (work / visits as f64, tt / visits as f64));
        if r == Some(Role::Work) {
            let errors: f64 = done
                .iter()
                .filter_map(|x| num_label(x, &format!("erros_{}", obj.name)))
                .sum();
            ca.insert(
                obj.name.clone(),
                (1.0 - errors / visits as f64).clamp(0.0, 1.0),
            );
        }
    }
    let rolled_ca = (!ca.is_empty()).then(|| ca.values().product());
    // Waiting by cause: the stay that is not work in each object.
    let mut espera = [0.0; CAUSES.len()];
    for r in &done {
        for (b, stay) in &r.stay {
            let Some(c) = role_of(*b).and_then(cause) else {
                continue;
            };
            let worked = if role_of(*b).is_some_and(Role::is_work) {
                r.work.get(b).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            espera[c] += (stay - worked).max(0.0);
        }
    }
    if n > 0 {
        espera.iter_mut().for_each(|e| *e /= n as f64);
    }
    // Cases in the model over the window (time average).
    let window = (end - from).max(f64::MIN_POSITIVE);
    let wip = items
        .iter()
        .map(|r| {
            let a = r.created_at.max(from);
            let b = r.ended_at.unwrap_or(end).min(end);
            (b - a).max(0.0)
        })
        .sum::<f64>()
        / window;
    let with_sla: Vec<(&ItemRecord, f64)> = done
        .iter()
        .filter_map(|r| num_label(r, "sla").map(|s| (*r, s)))
        .collect();
    let sla = (!with_sla.is_empty()).then(|| {
        with_sla.iter().filter(|(r, s)| lead(r) <= *s).count() as f64 / with_sla.len() as f64
    });
    let mut voltas_dist = [0usize; 4];
    for r in &done {
        voltas_dist[(r.loops() as usize).min(3)] += 1;
    }
    let ocupacao = report
        .blocks
        .iter()
        .enumerate()
        .filter(|(i, _)| role_of(*i) == Some(Role::Person))
        .map(|(_, b)| {
            let f = |s: &str| b.states.get(s).copied().unwrap_or(0.0);
            let trabalhando = f("utilize");
            let fora = f("off_shift");
            let on = 1.0 - fora;
            (
                b.name.clone(),
                Ocupacao {
                    trabalhando,
                    andando: f("travel_empty") + f("travel_loaded"),
                    ocioso: f("idle") + f("resting"),
                    fora,
                    no_expediente: (on > 1e-9).then(|| trabalhando / on),
                },
            )
        })
        .collect();
    Replicacao {
        concluidos: n,
        lead_time,
        processo,
        touch,
        razao_atividade: if lead_time > 0.0 {
            touch / lead_time
        } else {
            0.0
        },
        fpy,
        rolled_ca,
        wip,
        sla,
        por_dia: n as f64 / (window / DAY),
        voltas: mean(&|r| r.loops() as f64),
        voltas_dist,
        ca,
        pt_etapa,
        espera,
        ocupacao,
        leads: done.iter().map(|r| lead(r)).collect(),
        slas: with_sla.iter().map(|(_, s)| *s).collect(),
    }
}

/// Cases in the model at `WIP_POINTS` instants of the window.
fn serie_wip(items: &[ItemRecord], from: f64, end: f64) -> Vec<(f64, f64)> {
    (0..=WIP_POINTS)
        .map(|k| {
            let t = from + (end - from) * k as f64 / WIP_POINTS as f64;
            let n = items
                .iter()
                .filter(|r| r.created_at <= t && r.ended_at.is_none_or(|e| e > t))
                .count();
            (t, n as f64)
        })
        .collect()
}

/// Occupancy by hour of the day of the people (blocks in `people`), from the state
/// changes of one replication's trace, inside `[from, end)`.
fn ocupacao_hora(
    trace: &[utsim_core::TraceRecord],
    people: &[(usize, String)],
    from: f64,
    end: f64,
) -> BTreeMap<String, [Option<f64>; 24]> {
    let mut out = BTreeMap::new();
    for (b, name) in people {
        let mut changes: Vec<(f64, State)> = trace
            .iter()
            .filter_map(|r| match r.event {
                TraceEvent::State { block, state } if block == *b => Some((r.t, state)),
                _ => None,
            })
            .collect();
        changes.push((end, State::Idle));
        let mut on = [0.0f64; 24];
        let mut work = [0.0f64; 24];
        for w in changes.windows(2) {
            let (mut t, state) = w[0];
            let stop = w[1].0.min(end);
            t = t.max(from);
            while t < stop {
                let hour = ((t.rem_euclid(DAY)) / 60.0).floor() as usize % 24;
                let next_hour = (t / 60.0).floor() * 60.0 + 60.0;
                let until = next_hour.min(stop);
                let dt = until - t;
                if state != State::OffShift {
                    on[hour] += dt;
                    if state == State::Utilize {
                        work[hour] += dt;
                    }
                }
                t = until;
            }
        }
        let mut row = [None; 24];
        for h in 0..24 {
            if on[h] > 1e-9 {
                row[h] = Some(work[h] / on[h]);
            }
        }
        out.insert(name.clone(), row);
    }
    out
}

impl Indicadores {
    /// Runs `model.run.replications` replications with the per-item metrics (and the
    /// trace of the first one) and computes the indicators.
    pub fn rodar(model: &Model, pack: Option<&Pack>) -> Result<Indicadores, ModelError> {
        model.validate()?;
        let mut reps = Vec::new();
        let mut reports = Vec::new();
        let mut serie = Vec::new();
        let mut hora = BTreeMap::new();
        let people: Vec<(usize, String)> = model
            .objects
            .iter()
            .enumerate()
            .filter(|(i, _)| role(model, pack, *i) == Some(Role::Person))
            .map(|(i, o)| (i, o.name.clone()))
            .collect();
        for r in 0..model.run.replications {
            let mut sim = model.build(r);
            sim.enable_item_log();
            if r == 0 {
                sim.enable_trace();
            }
            let report = model.drive(&mut sim)?;
            let items = sim.take_item_log();
            if r == 0 {
                serie = serie_wip(&items, report.stats_from, report.end_time);
                hora = ocupacao_hora(
                    &sim.take_trace(),
                    &people,
                    report.stats_from,
                    report.end_time,
                );
            }
            reps.push(replicacao(model, pack, &report, &items));
            reports.push(report);
        }
        let parados = model
            .objects
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                !matches!(
                    role(model, pack, *i),
                    Some(Role::Entry | Role::Done | Role::Person | Role::Team)
                )
            })
            .map(|(i, o)| {
                let xs: Vec<f64> = reports.iter().map(|r| r.blocks[i].content_mean).collect();
                (o.name.clone(), Summary::from_samples(&xs))
            })
            .collect();
        Ok(Indicadores {
            modelo: model.name.clone(),
            linha: pack.map_or_else(|| "Lean Office".to_string(), |p| p.library.name.clone()),
            stats_from: reports.first().map_or(0.0, |r| r.stats_from),
            end_time: reports.first().map_or(0.0, |r| r.end_time),
            reps,
            reports,
            serie_wip: serie,
            ocupacao_hora: hora,
            parados,
        })
    }

    /// Summary of one indicator over the replications.
    pub fn resumo(&self, f: impl Fn(&Replicacao) -> f64) -> Summary {
        let xs: Vec<f64> = self.reps.iter().map(f).collect();
        Summary::from_samples(&xs)
    }

    /// Like [`Indicadores::resumo`] for an indicator some replications may lack.
    pub fn resumo_opt(&self, f: impl Fn(&Replicacao) -> Option<f64>) -> Option<Summary> {
        let xs: Vec<f64> = self.reps.iter().filter_map(f).collect();
        (!xs.is_empty()).then(|| Summary::from_samples(&xs))
    }

    /// Names of the steps with a %C&A, in model order.
    pub fn etapas_ca(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for r in &self.reps {
            for k in r.ca.keys() {
                if !out.contains(k) {
                    out.push(k.clone());
                }
            }
        }
        out
    }

    /// The main indicators: `(name, value, definition)` with the value already formatted.
    pub fn tabela(&self) -> Vec<(String, String, &'static str)> {
        let t = |s: Summary| format!("{} min ({} h)", mhw(&s, 1.0), mhw(&s, 1.0 / 60.0));
        let p = |s: Summary| pct_hw(&s);
        let mut rows = vec![
            (
                "Casos concluídos".to_string(),
                mhw(&self.resumo(|r| r.concluidos as f64), 1.0),
                "casos que terminaram na janela de estatísticas (depois do aquecimento)",
            ),
            (
                "Lead time (L/T)".to_string(),
                t(self.resumo(|r| r.lead_time)),
                "da entrada do caso à conclusão, em tempo corrido",
            ),
            (
                "Tempo de processamento (P/T)".to_string(),
                t(self.resumo(|r| r.processo)),
                "trabalho nas etapas (postos, aprovações, verificações, reuniões) por caso",
            ),
            (
                "Touch time".to_string(),
                t(self.resumo(|r| r.touch)),
                "parte do trabalho feita por pessoas, por caso",
            ),
            (
                "Razão de atividade".to_string(),
                p(self.resumo(|r| r.razao_atividade)),
                "touch time médio ÷ lead time médio",
            ),
            (
                "FPY (rendimento de primeira passagem)".to_string(),
                p(self.resumo(|r| r.fpy)),
                "casos concluídos sem nenhuma volta e sem erro pendente",
            ),
        ];
        if let Some(s) = self.resumo_opt(|r| r.rolled_ca) {
            rows.push((
                "%C&A acumulado (rolled)".to_string(),
                p(s),
                "produto do %C&A das etapas",
            ));
        }
        rows.push((
            "WIP de casos".to_string(),
            mhw(&self.resumo(|r| r.wip), 1.0),
            "casos em aberto no modelo, média no tempo",
        ));
        if let Some(s) = self.resumo_opt(|r| r.sla) {
            rows.push((
                "% dentro do SLA".to_string(),
                p(s),
                "casos com rótulo sla concluídos em até sla minutos",
            ));
        }
        rows.push((
            "Casos concluídos por dia".to_string(),
            mhw(&self.resumo(|r| r.por_dia), 1.0),
            "concluídos ÷ dias corridos da janela (1440 min)",
        ));
        rows.push((
            "Voltas de retrabalho por caso".to_string(),
            mhw(&self.resumo(|r| r.voltas), 1.0),
            "maior número de visitas a um mesmo objeto − 1",
        ));
        rows
    }
}

/// `mean ± half-width`, scaled, with a decimal comma.
pub fn mhw(s: &Summary, scale: f64) -> String {
    if s.half_width.is_finite() {
        format!("{} ± {}", num(s.mean * scale), num(s.half_width * scale))
    } else {
        num(s.mean * scale)
    }
}

/// Percentage `mean ± half-width`.
pub fn pct_hw(s: &Summary) -> String {
    if s.half_width.is_finite() {
        format!("{}% ± {}", num1(100.0 * s.mean), num1(100.0 * s.half_width))
    } else {
        format!("{}%", num1(100.0 * s.mean))
    }
}

/// Number with one decimal and a decimal comma.
pub fn num1(x: f64) -> String {
    format!("{x:.1}").replace('.', ",")
}

/// Number for reports: two decimals under 100, one under 1000, none above; decimal comma.
pub fn num(x: f64) -> String {
    if !x.is_finite() {
        return "-".to_string();
    }
    let s = if x.abs() >= 1000.0 {
        format!("{x:.0}")
    } else if x.abs() >= 100.0 {
        format!("{x:.1}")
    } else {
        format!("{x:.2}")
    };
    s.replace('.', ",")
}

#[cfg(test)]
mod tests;
