//! The project schedule (`docs/plano_lean_seis_sigma.md`, 1.6): tasks with a duration in
//! working days (or three points), finish-to-start dependencies with lag, milestones at the
//! tollgates, a frozen baseline, and the dates on the working-day calendar.
//!
//! The network is built by hand with `utsim_pert::Activity::new` and computed by the
//! simulator's PERT crate: `pert::analyze` (classic PERT, which runs the CPM of
//! `cpm::schedule` on the expected durations) for dates, floats, the critical path and the
//! probability of a date, and `monte_carlo::run` for the distribution of the finish. This
//! module only converts between working days and dates ([`crate::calendar`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utsim_core::Dist;
use utsim_pert::network::{Activity, ActivityNetwork};
use utsim_pert::{McOptions, Moments, monte_carlo, pert};

use crate::calendar::{self, Calendar};
use crate::date::Date;
use crate::domain::{Project, fields as dfields, of_project};
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};

pub mod fields {
    /// On the project: first day of the schedule.
    pub const START: &str = "inicio_cronograma";
    pub const PROJECT: &str = "projeto";
    pub const NAME: &str = "nome";
    /// Working days.
    pub const DURATION: &str = "duracao";
    pub const OPTIMISTIC: &str = "otimista";
    pub const LIKELY: &str = "provavel";
    pub const PESSIMISTIC: &str = "pessimista";
    /// Array of `{"tarefa": id, "defasagem": dias}`.
    pub const PREDECESSORS: &str = "predecessoras";
    pub const WHO: &str = "responsavel";
    /// Percent done, 0 to 100.
    pub const PERCENT: &str = "percentual";
    pub const MILESTONE: &str = "marco";
    /// Phase whose tollgate the milestone marks.
    pub const PHASE: &str = "fase";
    /// Target date of a milestone (the tollgate date).
    pub const TARGET: &str = "data_alvo";
    pub const BASE_START: &str = "base_inicio";
    pub const BASE_FINISH: &str = "base_fim";
    pub const ORDER: &str = "ordem";
}

/// A finish-to-start dependency with a lag in working days.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pred {
    #[serde(rename = "tarefa")]
    pub task: Id,
    #[serde(rename = "defasagem", default)]
    pub lag: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: Id,
    pub name: String,
    pub duration: f64,
    pub optimistic: Option<f64>,
    pub likely: Option<f64>,
    pub pessimistic: Option<f64>,
    pub preds: Vec<Pred>,
    pub who: Option<Id>,
    pub percent: f64,
    pub milestone: bool,
    pub phase: Option<Id>,
    pub target: Option<Date>,
    pub baseline: Option<(Date, Date)>,
    pub order: f64,
}

impl Task {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields as f;
        let date = |field| e.opt_text(field).and_then(|s| s.parse::<Date>().ok());
        let preds: Vec<Pred> = e
            .get(f::PREDECESSORS)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        let milestone = e.boolean(f::MILESTONE);
        Self {
            id,
            name: e.text(f::NAME),
            duration: if milestone {
                0.0
            } else {
                e.number(f::DURATION).unwrap_or(1.0).max(0.0)
            },
            optimistic: e.number(f::OPTIMISTIC),
            likely: e.number(f::LIKELY),
            pessimistic: e.number(f::PESSIMISTIC),
            preds,
            who: e.id(f::WHO),
            percent: e.number(f::PERCENT).unwrap_or(0.0).clamp(0.0, 100.0),
            milestone,
            phase: e.id(f::PHASE),
            target: date(f::TARGET),
            baseline: date(f::BASE_START).zip(date(f::BASE_FINISH)),
            order: e.number(f::ORDER).unwrap_or(0.0),
        }
    }

    /// The three points `(a, m, b)` when all are given and ordered.
    pub fn three_point(&self) -> Option<(f64, f64, f64)> {
        if self.milestone {
            return None;
        }
        let (a, m, b) = (self.optimistic?, self.likely?, self.pessimistic?);
        (a >= 0.0 && a <= m && m <= b && a < b).then_some((a, m, b))
    }

    /// Duration distribution given to `utsim-pert`.
    pub fn dist(&self) -> Dist {
        match self.three_point() {
            Some((min, mode, max)) => Dist::Triangular { min, mode, max },
            None => Dist::Const(self.duration),
        }
    }

    /// Expected duration (`(a + 4m + b) / 6` with three points).
    pub fn expected(&self) -> f64 {
        self.three_point()
            .map_or(self.duration, |(a, m, b)| (a + 4.0 * m + b) / 6.0)
    }
}

/// Tasks of a project in screen order.
pub fn tasks(state: &State, project: Id) -> Vec<Task> {
    let mut v = of_project(state, Kind::Task, project, Task::read);
    v.sort_by(|a, b| a.order.total_cmp(&b.order).then(a.id.cmp(&b.id)));
    v
}

/// Everything the computation needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub tasks: Vec<Task>,
    pub start: Date,
    pub calendar: Calendar,
    /// The project deadline (from the charter).
    pub deadline: Option<Date>,
}

/// The plan of a project; the start is the charter's schedule start or else `today`.
pub fn plan(state: &State, project: Id, today: Date) -> Plan {
    let e = state.entity(project);
    let date = |f| {
        e.and_then(|e| e.opt_text(f))
            .and_then(|s| s.parse::<Date>().ok())
    };
    Plan {
        tasks: tasks(state, project),
        start: date(fields::START).unwrap_or(today),
        calendar: calendar::of_project(state, project),
        deadline: date(dfields::project::DEADLINE),
    }
}

/// Something about the plan the person should know.
#[derive(Debug, Clone, PartialEq)]
pub enum Warning {
    /// Negative lags are not supported: taken as zero.
    NegativeLag(Id),
    /// A predecessor that is not a task of the project (deleted): ignored.
    UnknownPredecessor(Id),
    /// Three points given but not `a ≤ m ≤ b`: the plain duration is used.
    BadThreePoint(Id),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    #[error("as dependências formam um ciclo; tire uma delas")]
    Cycle,
}

/// One line of the Gantt.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub task: Id,
    pub name: String,
    pub milestone: bool,
    pub percent: f64,
    pub who: Option<Id>,
    /// Working days from the start.
    pub es: f64,
    pub ef: f64,
    pub ls: f64,
    pub lf: f64,
    pub total_float: f64,
    pub critical: bool,
    pub start: Date,
    pub finish: Date,
    pub late_finish: Date,
    pub baseline: Option<(Date, Date)>,
    /// Working days the finish moved from the baseline (positive: late).
    pub slip: Option<i64>,
    /// Target date of a milestone and the probability of meeting it (classic PERT).
    pub target: Option<Date>,
    pub probability: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleResult {
    pub rows: Vec<Row>,
    /// Working days, expected.
    pub duration: f64,
    pub start: Date,
    pub finish: Date,
    /// Standard deviation of the duration on the critical path (classic PERT).
    pub sd: f64,
    pub deadline: Option<Date>,
    pub deadline_probability: Option<f64>,
    /// Tasks on the critical path(s), in network order.
    pub critical: Vec<Id>,
    pub warnings: Vec<Warning>,
}

/// The network of some tasks (all when `only` is `None`): one activity per task, one per
/// positive lag, then the start and end milestones `utsim-pert` adds. Returns the network
/// and the activity index of each task included.
fn network(
    tasks: &[Task],
    only: Option<&BTreeSet<usize>>,
    warnings: &mut Vec<Warning>,
) -> Result<(ActivityNetwork, BTreeMap<usize, usize>), ScheduleError> {
    let index: BTreeMap<Id, usize> = tasks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
    let included = |i: usize| only.is_none_or(|s| s.contains(&i));
    let mut acts = Vec::new();
    let mut at = BTreeMap::new();
    for (i, t) in tasks.iter().enumerate().filter(|(i, _)| included(*i)) {
        at.insert(i, acts.len());
        acts.push(Activity::new(t.name.clone(), t.dist()));
    }
    let mut edges = Vec::new();
    for (i, t) in tasks.iter().enumerate().filter(|(i, _)| included(*i)) {
        for p in &t.preds {
            let Some(&j) = index.get(&p.task) else {
                warnings.push(Warning::UnknownPredecessor(t.id));
                continue;
            };
            if !included(j) {
                continue;
            }
            if p.lag < 0.0 {
                warnings.push(Warning::NegativeLag(t.id));
            }
            if p.lag > 0.0 {
                let lag = acts.len();
                acts.push(Activity::new("defasagem", Dist::Const(p.lag)));
                edges.push((at[&j], lag));
                edges.push((lag, at[&i]));
            } else {
                edges.push((at[&j], at[&i]));
            }
        }
    }
    let net = ActivityNetwork::new(acts, edges).map_err(|_| ScheduleError::Cycle)?;
    Ok((net, at))
}

/// Working-day index of a date (deadline inclusive: the work may end that day).
fn days_until(plan: &Plan, d: Date) -> f64 {
    (plan.calendar.index_of(plan.start, d) + 1).max(0) as f64
}

fn start_date(plan: &Plan, es: f64) -> Date {
    plan.calendar
        .date_at(plan.start, (es + 1e-9).floor() as i64)
}

/// Last working day of the work ending at `ef`; a zero-length activity (a milestone) falls
/// on the last working day before it, the day its predecessors end.
fn finish_date(plan: &Plan, es: f64, ef: f64) -> Date {
    if ef - es < 1e-9 && es < 1e-9 {
        start_date(plan, es)
    } else {
        plan.calendar
            .date_at(plan.start, ((ef - 1e-9).ceil() as i64 - 1).max(0))
    }
}

/// Indices of the tasks `i` depends on, itself included.
fn ancestors(tasks: &[Task], i: usize) -> BTreeSet<usize> {
    let index: BTreeMap<Id, usize> = tasks.iter().enumerate().map(|(k, t)| (t.id, k)).collect();
    let mut seen = BTreeSet::new();
    let mut stack = vec![i];
    while let Some(k) = stack.pop() {
        if seen.insert(k) {
            stack.extend(tasks[k].preds.iter().filter_map(|p| index.get(&p.task)));
        }
    }
    seen
}

/// Dates, floats, the critical path, the baseline slip and the probabilities.
pub fn compute(plan: &Plan) -> Result<ScheduleResult, ScheduleError> {
    let mut warnings = Vec::new();
    for t in &plan.tasks {
        let any = t.optimistic.is_some() || t.likely.is_some() || t.pessimistic.is_some();
        if any && !t.milestone && t.three_point().is_none() {
            warnings.push(Warning::BadThreePoint(t.id));
        }
    }
    let (net, at) = network(&plan.tasks, None, &mut warnings)?;
    let deadline_wd = plan.deadline.map(|d| days_until(plan, d));
    let result = pert::analyze(&net, Moments::Classic, deadline_wd);
    let sch = &result.schedule;
    let mut rows = Vec::new();
    for (i, t) in plan.tasks.iter().enumerate() {
        let a = at[&i];
        let (es, ef) = (sch.es[a], sch.ef[a]);
        let finish = finish_date(plan, es, ef);
        let slip = t
            .baseline
            .map(|(_, bf)| plan.calendar.working_days_between(bf, finish));
        let probability = t.target.map(|target| {
            let mut w = Vec::new();
            let only = ancestors(&plan.tasks, i);
            match network(&plan.tasks, Some(&only), &mut w) {
                Ok((sub, _)) => {
                    pert::analyze(&sub, Moments::Classic, Some(days_until(plan, target)))
                        .probability
                        .unwrap_or(0.0)
                }
                Err(_) => 0.0,
            }
        });
        rows.push(Row {
            task: t.id,
            name: t.name.clone(),
            milestone: t.milestone,
            percent: t.percent,
            who: t.who,
            es,
            ef,
            ls: sch.ls[a],
            lf: sch.lf[a],
            total_float: sch.total_float[a],
            critical: sch.critical[a],
            start: if ef - es < 1e-9 {
                finish
            } else {
                start_date(plan, es)
            },
            finish,
            late_finish: finish_date(plan, sch.ls[a], sch.lf[a]),
            baseline: t.baseline,
            slip,
            target: t.target,
            probability,
        });
    }
    let critical = rows.iter().filter(|r| r.critical).map(|r| r.task).collect();
    let finish = rows
        .iter()
        .map(|r| r.finish)
        .max()
        .unwrap_or_else(|| plan.calendar.next_working(plan.start));
    Ok(ScheduleResult {
        duration: sch.project,
        start: plan.calendar.next_working(plan.start),
        finish,
        sd: result.sd(),
        deadline: plan.deadline,
        deadline_probability: result.probability,
        critical,
        rows,
        warnings,
    })
}

/// Monte Carlo of the finish (`utsim_pert::monte_carlo::run`).
#[derive(Debug, Clone, PartialEq)]
pub struct McSummary {
    pub samples: usize,
    /// `(p, date)` for 50, 80, 90 and 95 %.
    pub percentiles: Vec<(f64, Date)>,
    pub deadline_probability: Option<f64>,
    /// Fraction of the draws in which each task is critical.
    pub criticality: Vec<(Id, f64)>,
}

pub fn monte_carlo(plan: &Plan, samples: usize, seed: u64) -> Result<McSummary, ScheduleError> {
    let mut w = Vec::new();
    let (net, at) = network(&plan.tasks, None, &mut w)?;
    let options = McOptions {
        samples,
        seed,
        deadline: plan.deadline.map(|d| days_until(plan, d)),
        bins: 20,
    };
    let r = monte_carlo::run(&net, &options);
    let percentiles = r
        .percentiles
        .iter()
        .map(|(p, q)| (*p, finish_date(plan, 0.0, *q)))
        .collect();
    let criticality = plan
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id, r.criticality[at[&i]]))
        .collect();
    Ok(McSummary {
        samples: r.samples,
        percentiles,
        deadline_probability: r.deadline.map(|d| d.probability),
        criticality,
    })
}

/// Edits that freeze the current dates as the baseline of every task (nothing when the
/// plan cannot be computed).
pub fn freeze_baseline(state: &State, project: Id, today: Date) -> Vec<Edit> {
    let Ok(r) = compute(&plan(state, project, today)) else {
        return Vec::new();
    };
    r.rows
        .iter()
        .flat_map(|row| {
            let e = EntityRef::new(Kind::Task, row.task);
            [
                Edit::new(e, fields::BASE_START, row.start.to_string()),
                Edit::new(e, fields::BASE_FINISH, row.finish.to_string()),
            ]
        })
        .collect()
}

/// Edits that create a task at the end of the list.
pub fn new_task(
    state: &State,
    project: Id,
    id: Id,
    name: &str,
    duration: f64,
    milestone: bool,
) -> Vec<Edit> {
    let order = tasks(state, project)
        .iter()
        .map(|t| t.order)
        .fold(0.0, f64::max)
        + 1.0;
    let r = EntityRef::new(Kind::Task, id);
    let mut e = vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::NAME, name.trim()),
        Edit::new(r, fields::ORDER, order),
    ];
    if milestone {
        e.push(Edit::new(r, fields::MILESTONE, true));
    } else {
        e.push(Edit::new(r, fields::DURATION, duration.max(0.0)));
    }
    e
}

/// Edit that sets the predecessors of a task (itself and duplicates left out).
pub fn set_predecessors(task: Id, preds: &[Pred]) -> Edit {
    let mut seen = BTreeSet::new();
    let v: Vec<Value> = preds
        .iter()
        .filter(|p| p.task != task && seen.insert(p.task))
        .map(|p| serde_json::to_value(p).expect("pred"))
        .collect();
    Edit::new(
        EntityRef::new(Kind::Task, task),
        fields::PREDECESSORS,
        Value::from(v),
    )
}

/// Edits that add one milestone per phase without one (named after the phase, linked to
/// it, depending on the previous phase's milestone): the tollgates on the schedule.
pub fn phase_milestones(state: &State, project: Id, mut ids: impl FnMut() -> Id) -> Vec<Edit> {
    let existing = tasks(state, project);
    let mut prev: Option<Id> = None;
    let mut order = existing.iter().map(|t| t.order).fold(0.0, f64::max);
    let mut edits = Vec::new();
    for ph in crate::domain::phases(state, project) {
        if let Some(t) = existing
            .iter()
            .find(|t| t.milestone && t.phase == Some(ph.id))
        {
            prev = Some(t.id);
            continue;
        }
        let id = ids();
        order += 1.0;
        let r = EntityRef::new(Kind::Task, id);
        edits.push(Edit::new(r, fields::PROJECT, project.to_string()));
        edits.push(Edit::new(r, fields::NAME, format!("Tollgate {}", ph.name)));
        edits.push(Edit::new(r, fields::MILESTONE, true));
        edits.push(Edit::new(r, fields::PHASE, ph.id.to_string()));
        edits.push(Edit::new(r, fields::ORDER, order));
        if let Some(p) = prev {
            edits.push(set_predecessors(id, &[Pred { task: p, lag: 0.0 }]));
        }
        prev = Some(id);
    }
    edits
}

/// The project ref, for the screens.
pub fn project_ref(project: Id) -> EntityRef {
    Project::entity_ref(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    fn task(n: u8, dur: f64, preds: &[(u8, f64)]) -> Task {
        Task {
            id: Id::v7(u64::from(n), [n; 10]),
            name: format!("T{n}"),
            duration: dur,
            optimistic: None,
            likely: None,
            pessimistic: None,
            preds: preds
                .iter()
                .map(|(p, lag)| Pred {
                    task: Id::v7(u64::from(*p), [*p; 10]),
                    lag: *lag,
                })
                .collect(),
            who: None,
            percent: 0.0,
            milestone: false,
            phase: None,
            target: None,
            baseline: None,
            order: f64::from(n),
        }
    }

    fn plan_of(tasks: Vec<Task>) -> Plan {
        Plan {
            tasks,
            // A Monday.
            start: d(2026, 11, 2),
            calendar: Calendar::default(),
            deadline: None,
        }
    }

    #[test]
    fn cpm_dates_on_the_calendar() {
        // T1 (3) → T2 (2) and T1 → T3 (5, lag 1): T3 is critical.
        let mut p = plan_of(vec![
            task(1, 3.0, &[]),
            task(2, 2.0, &[(1, 0.0)]),
            task(3, 5.0, &[(1, 1.0)]),
        ]);
        let r = compute(&p).unwrap();
        assert_eq!(r.duration, 9.0);
        let t3 = &r.rows[2];
        assert!(t3.critical && r.rows[0].critical && !r.rows[1].critical);
        assert_eq!(r.rows[1].total_float, 4.0);
        assert_eq!(r.rows[0].start, d(2026, 11, 2));
        assert_eq!(r.rows[0].finish, d(2026, 11, 4), "3 days: Mon to Wed");
        // T3 starts after the lag day (Thu 5) on Fri 6 and takes 5 working days.
        assert_eq!(t3.start, d(2026, 11, 6));
        assert_eq!(t3.finish, d(2026, 11, 12));
        assert_eq!(r.finish, d(2026, 11, 12));
        assert_eq!(r.critical.len(), 2);
        // A holiday on Monday 9 pushes the finish one working day.
        p.calendar.holidays.insert(d(2026, 11, 9));
        assert_eq!(compute(&p).unwrap().finish, d(2026, 11, 13));
    }

    #[test]
    fn three_points_probability_and_cycle() {
        let mut t = task(1, 1.0, &[]);
        t.optimistic = Some(2.0);
        t.likely = Some(4.0);
        t.pessimistic = Some(12.0);
        assert_eq!(t.expected(), 5.0);
        let mut ms = task(2, 0.0, &[(1, 0.0)]);
        ms.milestone = true;
        ms.target = Some(d(2026, 11, 6)); // index 4: 5 working days available
        let mut p = plan_of(vec![t, ms]);
        p.deadline = Some(d(2026, 11, 6));
        let r = compute(&p).unwrap();
        assert!((r.duration - 5.0).abs() < 1e-9);
        let sd = (12.0f64 - 2.0) / 6.0;
        assert!((r.sd - sd).abs() < 1e-9);
        // Target equals the mean: probability one half.
        let prob = r.rows[1].probability.unwrap();
        assert!((prob - 0.5).abs() < 1e-6, "{prob}");
        assert!((r.deadline_probability.unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(r.rows[1].start, r.rows[1].finish, "milestone on one day");
        let mc = monte_carlo(&p, 2000, 7).unwrap();
        assert_eq!(mc.percentiles.len(), 4);
        assert!(mc.percentiles[0].1 <= mc.percentiles[3].1);
        assert!(mc.criticality.iter().all(|(_, c)| *c > 0.99));
        // A cycle is refused.
        let cyc = plan_of(vec![task(1, 1.0, &[(2, 0.0)]), task(2, 1.0, &[(1, 0.0)])]);
        assert_eq!(compute(&cyc), Err(ScheduleError::Cycle));
    }

    #[test]
    fn warnings_and_baseline_slip() {
        let mut t1 = task(1, 2.0, &[(9, 0.0)]);
        t1.optimistic = Some(5.0);
        t1.likely = Some(1.0);
        t1.pessimistic = Some(9.0);
        let mut t2 = task(2, 2.0, &[(1, -2.0)]);
        t2.baseline = Some((d(2026, 11, 2), d(2026, 11, 3)));
        let r = compute(&plan_of(vec![t1.clone(), t2])).unwrap();
        assert!(r.warnings.contains(&Warning::BadThreePoint(t1.id)));
        assert!(r.warnings.contains(&Warning::UnknownPredecessor(t1.id)));
        assert!(
            r.warnings
                .iter()
                .any(|w| matches!(w, Warning::NegativeLag(_)))
        );
        // T2 finishes Thu 5 against a baseline of Tue 3: two working days late.
        assert_eq!(r.rows[1].finish, d(2026, 11, 5));
        assert_eq!(r.rows[1].slip, Some(2));
    }

    #[test]
    fn tasks_in_the_journal() {
        let mut n = 100u64;
        let mut ids = || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        };
        let (me, p) = (ids(), ids());
        let mut s = State::replay(&State::new().prepare(
            me,
            0,
            Project::create(p, "P", crate::domain::Approach::Dmaic, &mut ids),
            &mut ids,
        ));
        let mut commit = |s: &mut State, e: Vec<Edit>| {
            for op in s.prepare(me, 0, e, &mut ids) {
                s.apply(&op).unwrap();
            }
        };
        let a = Id::v7(900, [9; 10]);
        let e = new_task(&s, p, a, " Coletar dados ", 4.0, false);
        commit(&mut s, e);
        let ms = phase_milestones(&s, p, {
            let mut k = 1000u64;
            move || {
                k += 1;
                Id::v7(k, [7; 10])
            }
        });
        commit(&mut s, ms);
        assert!(phase_milestones(&s, p, || Id::v7(5000, [1; 10])).is_empty());
        let all = tasks(&s, p);
        assert_eq!(all.len(), 6);
        assert_eq!(all[0].name, "Coletar dados");
        assert!(all[2].milestone && all[2].preds.len() == 1);
        let first_ms = all[1].id;
        commit(
            &mut s,
            vec![set_predecessors(
                first_ms,
                &[
                    Pred { task: a, lag: 0.0 },
                    Pred { task: a, lag: 1.0 },
                    Pred {
                        task: first_ms,
                        lag: 0.0,
                    },
                ],
            )],
        );
        assert_eq!(tasks(&s, p)[1].preds.len(), 1);
        let today = d(2026, 11, 2);
        let base = freeze_baseline(&s, p, today);
        assert_eq!(base.len(), 12);
        commit(&mut s, base);
        let r = compute(&plan(&s, p, today)).unwrap();
        assert!(r.rows.iter().all(|row| row.slip == Some(0)));
        assert_eq!(r.rows[1].finish, d(2026, 11, 5));
    }
}
