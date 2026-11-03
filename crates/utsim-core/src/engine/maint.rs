//! Kernel side of [`crate::maintenance`]: age, failures by age, repair crew, preventive
//! plans, saved states and metrics. Only blocks with a [`Maintenance`] come here; the
//! plain breakdown path in `engine.rs` stays untouched for every other block.

use std::collections::VecDeque;

use rand::Rng;

use super::{BREAKDOWN, Core, EXTERNAL, EventKind, KernelOp, MAINTENANCE, Simulation};
use crate::Time;
use crate::block::{BlockId, State};
use crate::engine::{Ctx, EventId};
use crate::init::BlockState;
use crate::item::Value;
use crate::maintenance::{
    AgeBasis, CORRECTIVE_PRIORITY, Maintenance, PREVENTIVE_PRIORITY, Trigger, is_working,
};
use crate::rng::SimRng;
use crate::task::{MSG_RELEASE, RequestId, TaskKind};

/// Station tag of the maintenance `Operate` requests (blocks never see them).
const MAINT_TAG: u32 = u32::MAX;

/// Maintenance events of one block.
#[derive(Clone, Copy, Debug)]
pub(super) enum MaintEvent {
    /// The age reached the age at failure.
    Fail,
    /// The corrective repair ends.
    Repaired,
    /// Preventive plan `k` is due.
    Due(usize),
    /// Window `w` of the calendar of plan `k` starts: the plan is due.
    Window(usize, usize),
    /// The preventive in progress ends.
    PmDone,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Running,
    /// Broken since `since`, technician on the way.
    WaitRepair {
        since: Time,
    },
    /// Broken since `since`, repair started at `start`.
    Repairing {
        since: Time,
        start: Time,
    },
    /// Preventive of plan `k` due, stopped since `since`, technician on the way.
    WaitPm {
        k: usize,
        since: Time,
    },
    /// Preventive of plan `k` in progress (stopped since `since`).
    Pm {
        k: usize,
        since: Time,
    },
}

impl Phase {
    fn waiting(self) -> bool {
        matches!(self, Phase::WaitRepair { .. } | Phase::WaitPm { .. })
    }
}

#[derive(Clone, Debug, Default)]
struct Plan {
    /// Working time (Hours) or cycles (Cycles) at the end of the last preventive.
    mark: f64,
    /// Queued or in progress.
    due: bool,
    /// Pending `Due` event (Hours, Cycles, Clock).
    event: Option<EventId>,
}

/// Run-time maintenance state of one block.
pub(super) struct MaintState {
    cfg: Maintenance,
    rng: SimRng,
    /// Virtual age, accrued up to `age_since` when accruing.
    age: f64,
    age_since: Option<Time>,
    /// Age at the next failure.
    next_fail: f64,
    fail_event: Option<EventId>,
    /// Working time, accrued up to `busy_since` while working.
    usage: f64,
    busy_since: Option<Time>,
    cycles: u64,
    phase: Phase,
    /// Request to the crew in progress, and the technician that came.
    request: Option<RequestId>,
    technician: Option<BlockId>,
    /// Time already repaired before `t = 0` (initial state): the repair draws its residual.
    resume_repair: Option<Time>,
    plans: Vec<Plan>,
    queue: VecDeque<usize>,
    up_since: Time,
    // Statistics of the window (cleared at the warm-up).
    from: Time,
    failures: u64,
    repairs: u64,
    preventives: u64,
    wait_sum: f64,
    wait_n: u64,
    ttr_sum: f64,
    repair_sum: f64,
    corrective_time: f64,
    preventive_time: f64,
    outputs: u64,
    rejects: u64,
}

impl MaintState {
    pub(super) fn new(cfg: Maintenance, rng: SimRng) -> Self {
        let n = cfg.preventive.len();
        MaintState {
            cfg,
            rng,
            age: 0.0,
            age_since: None,
            next_fail: f64::INFINITY,
            fail_event: None,
            usage: 0.0,
            busy_since: None,
            cycles: 0,
            phase: Phase::Running,
            request: None,
            technician: None,
            resume_repair: None,
            plans: vec![Plan::default(); n],
            queue: VecDeque::new(),
            up_since: 0.0,
            from: 0.0,
            failures: 0,
            repairs: 0,
            preventives: 0,
            wait_sum: 0.0,
            wait_n: 0,
            ttr_sum: 0.0,
            repair_sum: 0.0,
            corrective_time: 0.0,
            preventive_time: 0.0,
            outputs: 0,
            rejects: 0,
        }
    }

    /// Whether `request` is this block's request to the crew.
    pub(super) fn owns(&self, request: u64) -> bool {
        self.request.is_some_and(|r| r.0 == request)
    }

    pub(super) fn waiting(&self) -> bool {
        self.phase.waiting()
    }

    fn age_at(&self, now: Time) -> f64 {
        self.age + self.age_since.map_or(0.0, |s| now - s)
    }

    fn usage_at(&self, now: Time) -> f64 {
        self.usage + self.busy_since.map_or(0.0, |s| now - s)
    }

    /// The statistics window starts now (warm-up).
    pub(super) fn reset_stats(&mut self, now: Time) {
        self.from = now;
        self.failures = 0;
        self.repairs = 0;
        self.preventives = 0;
        self.wait_sum = 0.0;
        self.wait_n = 0;
        self.ttr_sum = 0.0;
        self.repair_sum = 0.0;
        self.corrective_time = 0.0;
        self.preventive_time = 0.0;
        self.outputs = 0;
        self.rejects = 0;
    }

    /// An item leaves for `to`: OEE counts.
    pub(super) fn count_output(
        &mut self,
        to: BlockId,
        labels: &std::collections::BTreeMap<String, Value>,
    ) {
        self.outputs += 1;
        let by_label = self
            .cfg
            .reject_label
            .as_ref()
            .is_some_and(|l| match labels.get(l) {
                Some(Value::Num(x)) => *x != 0.0,
                Some(Value::Text(s)) => !s.is_empty() && s != "false",
                None => false,
            });
        if by_label || self.cfg.reject_to.contains(&to) {
            self.rejects += 1;
        }
    }

    /// Report metrics (see [`crate::maintenance::METRICS`]). `off_shift` is the time
    /// shown off shift in the window.
    pub(super) fn metrics(&self, now: Time, off_shift: f64) -> Vec<(String, f64)> {
        let clip = |since: Time| now - since.max(self.from);
        let (mut corrective, mut preventive, mut wait_total) =
            (self.corrective_time, self.preventive_time, self.wait_sum);
        match self.phase {
            Phase::Running => {}
            Phase::WaitRepair { since } => {
                corrective += clip(since);
                wait_total += clip(since);
            }
            Phase::Repairing { since, .. } => corrective += clip(since),
            Phase::WaitPm { since, .. } => {
                preventive += clip(since);
                wait_total += clip(since);
            }
            Phase::Pm { since, .. } => preventive += clip(since),
        }
        let window = (now - self.from).max(0.0);
        let planned = (window - off_shift - preventive).max(0.0);
        let run = (planned - corrective).max(0.0);
        let availability = if planned > 0.0 { run / planned } else { 0.0 };
        let performance = match self.cfg.ideal_cycle {
            None => 1.0,
            Some(_) if run <= 0.0 => 0.0,
            Some(c) => (c * self.outputs as f64 / run).min(1.0),
        };
        let quality = if self.outputs == 0 {
            1.0
        } else {
            (self.outputs - self.rejects) as f64 / self.outputs as f64
        };
        let up = (window - corrective - preventive).max(0.0);
        let per = |sum: f64, n: u64| if n == 0 { 0.0 } else { sum / n as f64 };
        let cost = self.cfg.cost.per_time_down * (corrective + preventive)
            + self.cfg.cost.per_repair * self.repairs as f64
            + self.cfg.cost.per_preventive * self.preventives as f64;
        vec![
            ("oee".into(), availability * performance * quality),
            ("oee.availability".into(), availability),
            ("oee.performance".into(), performance),
            ("oee.quality".into(), quality),
            ("maintenance.failures".into(), self.failures as f64),
            ("maintenance.repairs".into(), self.repairs as f64),
            ("maintenance.preventives".into(), self.preventives as f64),
            (
                "maintenance.mtbf".into(),
                if self.failures == 0 {
                    up
                } else {
                    up / self.failures as f64
                },
            ),
            ("maintenance.mttr".into(), per(self.ttr_sum, self.repairs)),
            (
                "maintenance.repair_time".into(),
                per(self.repair_sum, self.repairs),
            ),
            (
                "maintenance.wait_technician".into(),
                per(self.wait_sum, self.wait_n),
            ),
            ("maintenance.wait_technician_total".into(), wait_total),
            ("maintenance.down_time".into(), corrective),
            ("maintenance.preventive_time".into(), preventive),
            ("maintenance.cost".into(), cost),
            ("maintenance.age".into(), self.age_at(now)),
            ("maintenance.rejects".into(), self.rejects as f64),
        ]
    }
}

impl Core {
    fn cancel_event(&mut self, id: Option<EventId>) {
        if let Some(id) = id {
            self.cancelled.insert(id.0);
        }
    }

    /// Brings the age and working-time clocks of `b` in line with its current state and
    /// stop reasons, rescheduling the failure and the `Hours` plans when they change.
    /// Idempotent: nothing happens when nothing changed.
    pub(super) fn maint_refresh(&mut self, b: BlockId) {
        let now = self.now;
        let rt = &mut self.blocks[b.0];
        let Some(m) = rt.maint.as_deref_mut() else {
            return;
        };
        let stops = rt.stops;
        let busy = stops == 0 && rt.stats.states.current().is_some_and(is_working);
        let running = stops & (BREAKDOWN | MAINTENANCE) == 0;
        let mut usage_changed = false;
        if busy != m.busy_since.is_some() {
            if let Some(s) = m.busy_since.take() {
                m.usage += now - s;
            }
            if busy {
                m.busy_since = Some(now);
            }
            usage_changed = true;
        }
        let accrue = match m.cfg.basis {
            AgeBasis::Clock => running,
            AgeBasis::Busy => busy,
            AgeBasis::Cycles => false,
        };
        let mut fail_at = None;
        if accrue != m.age_since.is_some() {
            if let Some(s) = m.age_since.take() {
                m.age += now - s;
            }
            let old = m.fail_event.take();
            if accrue {
                m.age_since = Some(now);
                if m.next_fail.is_finite() {
                    fail_at = Some(now + (m.next_fail - m.age).max(0.0));
                }
            }
            self.cancel_event(old);
        }
        if let Some(t) = fail_at {
            let id = self.schedule_at(t, EventKind::Maint(b, MaintEvent::Fail));
            self.maint_mut(b).fail_event = Some(id);
        }
        if usage_changed {
            let m = self.maint_mut(b);
            let usage = m.usage;
            let mut todo = Vec::new();
            for (k, p) in m.cfg.preventive.iter().enumerate() {
                if let Trigger::Hours(h) = p.trigger {
                    let plan = &mut m.plans[k];
                    if plan.due {
                        continue;
                    }
                    todo.push((k, plan.event.take(), (h - (usage - plan.mark)).max(0.0)));
                }
            }
            for (k, old, left) in todo {
                self.cancel_event(old);
                if busy {
                    let id = self.schedule_at(now + left, EventKind::Maint(b, MaintEvent::Due(k)));
                    self.maint_mut(b).plans[k].event = Some(id);
                }
            }
        }
    }

    fn maint_mut(&mut self, b: BlockId) -> &mut MaintState {
        self.blocks[b.0]
            .maint
            .as_deref_mut()
            .expect("block with maintenance")
    }

    /// An item left `b`: cycles for the age and the `Cycles` plans.
    pub(super) fn maint_cycle(&mut self, b: BlockId) {
        let now = self.now;
        let Some(m) = self.blocks[b.0].maint.as_deref_mut() else {
            return;
        };
        m.cycles += 1;
        let cycles = m.cycles as f64;
        let mut fail = false;
        if m.cfg.basis == AgeBasis::Cycles && m.phase == Phase::Running {
            m.age += 1.0;
            fail = m.age >= m.next_fail && m.fail_event.is_none();
        }
        let due: Vec<usize> = m
            .cfg
            .preventive
            .iter()
            .enumerate()
            .filter(|(k, p)| {
                let plan = &m.plans[*k];
                matches!(p.trigger, Trigger::Cycles(n) if !plan.due && plan.event.is_none()
                    && cycles - plan.mark >= n as f64)
            })
            .map(|(k, _)| k)
            .collect();
        if fail {
            let id = self.schedule_at(now, EventKind::Maint(b, MaintEvent::Fail));
            self.maint_mut(b).fail_event = Some(id);
        }
        for k in due {
            let id = self.schedule_at(now, EventKind::Maint(b, MaintEvent::Due(k)));
            self.maint_mut(b).plans[k].event = Some(id);
        }
    }

    /// The state a stopped block with maintenance shows.
    pub(super) fn maint_shown(&self, b: BlockId) -> Option<State> {
        let rt = &self.blocks[b.0];
        let m = rt.maint.as_deref()?;
        if rt.stops & BREAKDOWN != 0 {
            Some(if m.waiting() {
                State::WaitingRepair
            } else {
                State::Down
            })
        } else if rt.stops & MAINTENANCE != 0 {
            Some(if m.waiting() {
                State::WaitingRepair
            } else {
                State::Maintenance
            })
        } else {
            None
        }
    }

    /// Draws the age at the next failure from the age now.
    fn draw_failure(&mut self, b: BlockId, residual_up: Option<Time>) {
        let now = self.now;
        let rt = &mut self.blocks[b.0];
        let m = rt.maint.as_deref_mut().expect("block with maintenance");
        let age = m.age_at(now);
        m.next_fail = match (m.cfg.wear, &rt.downtime) {
            (Some(w), _) => {
                let u = rt.downtime_rng.random::<f64>();
                w.next_failure_age(age, u)
            }
            (None, Some(dt)) => {
                let t = match residual_up {
                    Some(e) => dt.mtbf.sample_residual(e, &mut rt.downtime_rng),
                    None => dt.mtbf.sample(&mut rt.downtime_rng).max(0.0),
                };
                age + t
            }
            (None, None) => f64::INFINITY,
        };
    }
}

impl Simulation {
    /// Enables maintenance on a block (see [`crate::maintenance`]). Breakdowns still come
    /// from [`Simulation::set_downtime`] (`mttr`, and `mtbf` without wear).
    pub fn set_maintenance(&mut self, block: BlockId, maintenance: Maintenance) {
        let rng = crate::rng::stream(
            self.config.seed,
            self.config.replication,
            super::MAINTENANCE_STREAMS + block.0 as u64,
        );
        self.core.blocks[block.0].maint = Some(Box::new(MaintState::new(maintenance, rng)));
    }

    fn request_crew(&mut self, b: BlockId, crew: BlockId, priority: i32) {
        let id = Ctx {
            core: &mut self.core,
            me: b,
        }
        .request_task(
            crew,
            TaskKind::Operate {
                station: b,
                tag: MAINT_TAG,
            },
            priority,
        );
        self.core.maint_mut(b).request = Some(id);
    }

    fn release_technician(&mut self, b: BlockId) {
        let m = self.core.maint_mut(b);
        if let (Some(tech), Some(id)) = (m.technician.take(), m.request.take()) {
            self.core.ops.push_back(KernelOp::Message {
                to: tech,
                from: b,
                tag: MSG_RELEASE,
                payload: id.0,
            });
        }
    }

    fn plan_crew(&self, b: BlockId, k: usize) -> Option<BlockId> {
        let m = self.core.blocks[b.0].maint.as_deref()?;
        m.cfg.preventive[k].crew.or(m.cfg.crew)
    }

    /// Run start: age, plans and the first failure (or the breakdown of an initial state).
    pub(super) fn maint_start(&mut self, b: BlockId, state: Option<&BlockState>) {
        let now = self.core.now;
        let (age, remaining, down, up) = match state {
            Some(s) => (s.age, s.preventive.clone(), s.down, s.up),
            None => (None, Vec::new(), None, None),
        };
        let calendar = self.core.calendar;
        // The blocks' `reset` already set states: start the clocks afresh from here.
        let stale: Vec<Option<EventId>> = {
            let m = self.core.maint_mut(b);
            m.age_since = None;
            m.busy_since = None;
            m.usage = 0.0;
            std::iter::once(m.fail_event.take())
                .chain(m.plans.iter_mut().map(|p| p.event.take()))
                .collect()
        };
        for id in stale {
            self.core.cancel_event(id);
        }
        let triggers: Vec<Trigger> = {
            let m = self.core.maint_mut(b);
            m.age = age.unwrap_or(0.0);
            m.up_since = now - up.unwrap_or(0.0);
            m.cfg.preventive.iter().map(|p| p.trigger.clone()).collect()
        };
        for (k, trigger) in triggers.iter().enumerate() {
            let left = remaining.get(k).copied().flatten();
            match trigger {
                Trigger::Hours(_) | Trigger::Cycles(_) => {
                    let total = match trigger {
                        Trigger::Hours(h) => *h,
                        Trigger::Cycles(n) => *n as f64,
                        _ => unreachable!(),
                    };
                    self.core.maint_mut(b).plans[k].mark = left.map_or(0.0, |r| r - total);
                    if left.is_some_and(|r| r <= 0.0) {
                        let id = self
                            .core
                            .schedule_at(now, EventKind::Maint(b, MaintEvent::Due(k)));
                        self.core.maint_mut(b).plans[k].event = Some(id);
                    }
                }
                Trigger::Clock(t) => {
                    let id = self.core.schedule_at(
                        now + left.unwrap_or(*t),
                        EventKind::Maint(b, MaintEvent::Due(k)),
                    );
                    self.core.maint_mut(b).plans[k].event = Some(id);
                }
                Trigger::Calendar(table) => {
                    let phase = calendar.rem_euclid(table.period);
                    for (w, win) in table.off.iter().enumerate() {
                        let mut off = win.start - phase;
                        if off < 0.0 {
                            off += table.period;
                        }
                        self.core
                            .schedule_at(now + off, EventKind::Maint(b, MaintEvent::Window(k, w)));
                    }
                }
            }
        }
        match down {
            Some(elapsed) => {
                let crew = self.core.maint_mut(b).cfg.crew;
                match crew {
                    Some(c) => {
                        let m = self.core.maint_mut(b);
                        m.phase = Phase::WaitRepair { since: now };
                        m.resume_repair = Some(elapsed);
                        self.stop(b, BREAKDOWN);
                        self.request_crew(b, c, CORRECTIVE_PRIORITY);
                    }
                    None => {
                        let rt = &mut self.core.blocks[b.0];
                        let repair = rt.downtime.as_ref().map_or(0.0, |dt| {
                            dt.mttr.sample_residual(elapsed, &mut rt.downtime_rng)
                        });
                        let m = self.core.maint_mut(b);
                        m.phase = Phase::Repairing {
                            since: now - elapsed,
                            start: now - elapsed,
                        };
                        self.core
                            .schedule_at(now + repair, EventKind::Maint(b, MaintEvent::Repaired));
                        self.stop(b, BREAKDOWN);
                    }
                }
            }
            None => self.core.draw_failure(b, up),
        }
        self.core.maint_refresh(b);
    }

    pub(super) fn maint_event(&mut self, b: BlockId, ev: MaintEvent) {
        match ev {
            MaintEvent::Fail => self.maint_fail(b),
            MaintEvent::Repaired => self.maint_repaired(b),
            MaintEvent::Due(k) => self.maint_due(b, k),
            MaintEvent::Window(k, w) => {
                if let Trigger::Calendar(table) = &self.core.maint_mut(b).cfg.preventive[k].trigger
                {
                    let period = table.period;
                    let now = self.core.now;
                    self.core
                        .schedule_at(now + period, EventKind::Maint(b, MaintEvent::Window(k, w)));
                }
                self.maint_due(b, k);
            }
            MaintEvent::PmDone => self.maint_pm_done(b),
        }
    }

    fn maint_fail(&mut self, b: BlockId) {
        let now = self.core.now;
        let m = self.core.maint_mut(b);
        m.fail_event = None;
        if m.phase != Phase::Running {
            return;
        }
        m.failures += 1;
        match m.cfg.crew {
            Some(crew) => {
                m.phase = Phase::WaitRepair { since: now };
                self.stop(b, BREAKDOWN);
                self.request_crew(b, crew, CORRECTIVE_PRIORITY);
            }
            None => {
                m.phase = Phase::Repairing {
                    since: now,
                    start: now,
                };
                let rt = &mut self.core.blocks[b.0];
                let repair = rt
                    .downtime
                    .as_ref()
                    .map_or(0.0, |dt| dt.mttr.sample(&mut rt.downtime_rng).max(0.0));
                self.core
                    .schedule_at(now + repair, EventKind::Maint(b, MaintEvent::Repaired));
                self.stop(b, BREAKDOWN);
            }
        }
    }

    /// A technician reached `b` (the request is the block's current one).
    pub(super) fn maint_arrived(&mut self, b: BlockId, technician: BlockId) {
        let now = self.core.now;
        let m = self.core.maint_mut(b);
        m.technician = Some(technician);
        match m.phase {
            Phase::WaitRepair { since } => {
                m.wait_sum += now - since.max(m.from);
                m.wait_n += 1;
                m.phase = Phase::Repairing { since, start: now };
                let resume = m.resume_repair.take();
                let rt = &mut self.core.blocks[b.0];
                let repair = rt.downtime.as_ref().map_or(0.0, |dt| match resume {
                    Some(e) => dt.mttr.sample_residual(e, &mut rt.downtime_rng),
                    None => dt.mttr.sample(&mut rt.downtime_rng).max(0.0),
                });
                self.core
                    .schedule_at(now + repair, EventKind::Maint(b, MaintEvent::Repaired));
            }
            Phase::WaitPm { k, since } => {
                m.wait_sum += now - since.max(m.from);
                m.wait_n += 1;
                m.phase = Phase::Pm { k, since };
                let d = m.cfg.preventive[k].duration.sample(&mut m.rng).max(0.0);
                self.core
                    .schedule_at(now + d, EventKind::Maint(b, MaintEvent::PmDone));
            }
            _ => return,
        }
        self.show_stopped(b);
    }

    fn show_stopped(&mut self, b: BlockId) {
        if let Some(state) = self.core.maint_shown(b) {
            self.core.set_block_state(b, state);
        }
    }

    fn maint_repaired(&mut self, b: BlockId) {
        let now = self.core.now;
        let m = self.core.maint_mut(b);
        let Phase::Repairing { since, start } = m.phase else {
            return;
        };
        m.repairs += 1;
        m.ttr_sum += now - since;
        m.repair_sum += now - start;
        m.corrective_time += now - since.max(m.from);
        m.age *= m.cfg.repair.kept();
        m.phase = Phase::Running;
        m.up_since = now;
        self.release_technician(b);
        self.core.draw_failure(b, None);
        self.resume(b, BREAKDOWN);
        self.maint_try_pm(b);
    }

    fn maint_due(&mut self, b: BlockId, k: usize) {
        let m = self.core.maint_mut(b);
        m.plans[k].event = None;
        if m.plans[k].due {
            return;
        }
        m.plans[k].due = true;
        m.queue.push_back(k);
        self.maint_try_pm(b);
    }

    /// Starts the next queued preventive when the block can stop for it: at once when
    /// it holds no work, else after closing the inputs and finishing the work.
    pub(super) fn maint_try_pm(&mut self, b: BlockId) {
        let rt = &self.core.blocks[b.0];
        let Some(m) = rt.maint.as_deref() else { return };
        if m.phase != Phase::Running || m.queue.is_empty() {
            return;
        }
        if rt.stops & (BREAKDOWN | MAINTENANCE | EXTERNAL) != 0 {
            return;
        }
        if !rt.draining {
            self.core.blocks[b.0].draining = true;
            self.core.draining.push(b);
        }
        self.maint_check_drain(b);
    }

    /// A draining block whose work is done (and with nothing on the way to it) stops
    /// for its preventive. Off shift it may stop too, if it holds no work.
    pub(super) fn maint_check_drain(&mut self, b: BlockId) {
        let rt = &self.core.blocks[b.0];
        if !rt.draining || rt.stops & (BREAKDOWN | MAINTENANCE | EXTERNAL) != 0 {
            return;
        }
        let busy = rt.incoming > 0 || {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[b.0].work_in_progress(&Ctx { core, me: b })
        };
        if busy {
            return;
        }
        self.core.blocks[b.0].draining = false;
        self.core.draining.retain(|&d| d != b);
        self.begin_pm(b);
        self.resolve();
    }

    pub(super) fn maint_check_drains(&mut self) {
        let list = self.core.draining.clone();
        for b in list {
            self.maint_check_drain(b);
        }
    }

    fn begin_pm(&mut self, b: BlockId) {
        let now = self.core.now;
        let Some(&k) = self.core.maint_mut(b).queue.front() else {
            return;
        };
        let crew = self.plan_crew(b, k);
        let m = self.core.maint_mut(b);
        match crew {
            Some(c) => {
                m.phase = Phase::WaitPm { k, since: now };
                self.stop(b, MAINTENANCE);
                self.show_stopped(b);
                self.request_crew(b, c, PREVENTIVE_PRIORITY);
            }
            None => {
                m.phase = Phase::Pm { k, since: now };
                let d = m.cfg.preventive[k].duration.sample(&mut m.rng).max(0.0);
                self.core
                    .schedule_at(now + d, EventKind::Maint(b, MaintEvent::PmDone));
                self.stop(b, MAINTENANCE);
                self.show_stopped(b);
            }
        }
    }

    fn maint_pm_done(&mut self, b: BlockId) {
        let now = self.core.now;
        let m = self.core.maint_mut(b);
        let Phase::Pm { k, since } = m.phase else {
            return;
        };
        m.queue.pop_front();
        m.preventives += 1;
        m.preventive_time += now - since.max(m.from);
        m.age = 0.0;
        m.up_since = now;
        let usage = m.usage_at(now);
        let cycles = m.cycles as f64;
        let trigger = m.cfg.preventive[k].trigger.clone();
        let plan = &mut m.plans[k];
        plan.due = false;
        plan.mark = match trigger {
            Trigger::Cycles(_) => cycles,
            _ => usage,
        };
        let old = plan.event.take();
        self.core.cancel_event(old);
        if let Trigger::Clock(t) = trigger {
            let id = self
                .core
                .schedule_at(now + t, EventKind::Maint(b, MaintEvent::Due(k)));
            self.core.maint_mut(b).plans[k].event = Some(id);
        }
        self.release_technician(b);
        self.core.draw_failure(b, None);
        if self.core.maint_mut(b).queue.is_empty() {
            self.core.maint_mut(b).phase = Phase::Running;
            self.resume(b, MAINTENANCE);
        } else {
            self.begin_pm(b);
        }
        self.core.maint_refresh(b);
    }

    /// The maintenance part of a saved state; returns a note about what is lost.
    pub(super) fn maint_save(&self, b: BlockId, state: &mut BlockState) -> Option<String> {
        let now = self.core.now;
        let rt = &self.core.blocks[b.0];
        let m = rt.maint.as_deref()?;
        state.down = None;
        state.up = None;
        let mut note = None;
        match m.phase {
            Phase::Running | Phase::WaitPm { .. } | Phase::Pm { .. } => {
                state.up = Some(now - m.up_since);
            }
            Phase::WaitRepair { .. } => {
                state.down = Some(m.resume_repair.unwrap_or(0.0));
                note = Some("o técnico a caminho é chamado de novo".to_string());
            }
            Phase::Repairing { since, start } => {
                if m.cfg.crew.is_some() {
                    state.down = Some(now - start);
                    note = Some(
                        "o reparo em curso continua depois que o técnico chegar de novo"
                            .to_string(),
                    );
                } else {
                    state.down = Some(now - since);
                }
            }
        }
        if matches!(m.phase, Phase::WaitPm { .. } | Phase::Pm { .. }) {
            note = Some("a preventiva em curso recomeça do início".to_string());
        }
        state.age = Some(m.age_at(now));
        let usage = m.usage_at(now);
        let cycles = m.cycles as f64;
        state.preventive = m
            .cfg
            .preventive
            .iter()
            .zip(&m.plans)
            .map(|(p, plan)| {
                if plan.due {
                    return match p.trigger {
                        Trigger::Calendar(_) => None,
                        _ => Some(0.0),
                    };
                }
                match &p.trigger {
                    Trigger::Hours(h) => Some((h - (usage - plan.mark)).max(0.0)),
                    Trigger::Cycles(n) => Some((*n as f64 - (cycles - plan.mark)).max(0.0)),
                    Trigger::Clock(_) => None,
                    Trigger::Calendar(_) => None,
                }
            })
            .collect();
        // A clock plan keeps the time left to its event.
        for (k, p) in m.cfg.preventive.iter().enumerate() {
            if let (Trigger::Clock(_), false) = (&p.trigger, m.plans[k].due) {
                state.preventive[k] = self
                    .event_time(m.plans[k].event)
                    .map(|t| (t - now).max(0.0));
            }
        }
        if state.preventive.iter().all(Option::is_none) {
            state.preventive.clear();
        }
        note
    }

    fn event_time(&self, id: Option<EventId>) -> Option<Time> {
        let id = id?;
        self.core
            .events
            .iter()
            .find(|e| e.seq == id.0)
            .map(|e| e.time)
    }

    /// Checks of the maintenance part of an initial state.
    pub(super) fn maint_check_state(&self, b: BlockId, state: &BlockState) -> Option<String> {
        let rt = &self.core.blocks[b.0];
        let uses = state.age.is_some() || !state.preventive.is_empty();
        match rt.maint.as_deref() {
            None if uses => Some("'age' e 'preventive' exigem manutenção no objeto".into()),
            Some(m) if !state.preventive.is_empty() && state.preventive.len() != m.plans.len() => {
                Some(format!(
                    "'preventive' tem {} valor(es), mas o objeto tem {} plano(s) de preventiva",
                    state.preventive.len(),
                    m.plans.len()
                ))
            }
            _ => None,
        }
    }

    /// Maintenance metrics of block `b`, if it has maintenance.
    pub(super) fn maint_metrics(&self, b: BlockId, now: Time) -> Vec<(String, f64)> {
        let rt = &self.core.blocks[b.0];
        let Some(m) = rt.maint.as_deref() else {
            return Vec::new();
        };
        let window = (now - m.from).max(0.0);
        let off = rt
            .stats
            .states
            .fractions(now)
            .get(&State::OffShift)
            .copied()
            .unwrap_or(0.0)
            * window;
        m.metrics(now, off)
    }
}
