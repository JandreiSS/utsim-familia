use serde::{Deserialize, Serialize};
use utsim_core::init::Accepts;
use utsim_core::stats::TimeWeighted;
use utsim_core::task::{
    MSG_ARRIVED, MSG_RELEASE, MSG_RESUME_WORK, MSG_SUSPEND, MSG_TRANSPORT_RESERVED, MSG_WITHDRAW,
    request_priority,
};
use utsim_core::{
    Behavior, BlockId, BlockState, Ctx, Dist, EventId, Item, ItemId, RequestId, State, TaskKind,
    Time, WorkPhase,
};

/// Operators a processor needs (phase 4, contract): `count` members of `team`
/// (a transporter or a dispatcher), during setup and/or processing. With `requires`,
/// only people whose profile lists that skill may operate (a dispatcher skips the others).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperatorUse {
    pub team: String,
    #[serde(default = "one")]
    pub count: usize,
    #[serde(default = "yes")]
    pub setup: bool,
    #[serde(default = "yes")]
    pub process: bool,
    /// Skill the operators must have (absent: anyone of the team).
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "exige")]
    pub requires: Option<String>,
    /// Priority of the operator requests at the team's dispatcher (higher goes first;
    /// absent: 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// Item label whose value, when the item has it, replaces `priority`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_label: Option<String>,
}

fn one() -> usize {
    1
}

fn yes() -> bool {
    true
}

const SETUP_DONE: u32 = 0;
const PROCESS_DONE: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Waiting for operators before a phase can start (see `Pending`).
    WaitingOperator,
    Setup,
    Processing,
    Done,
}

/// A running setup/process timer, kept so a breakdown can pause it.
struct Timer {
    item: ItemId,
    tag: u32,
    event: Option<EventId>,
    due: Time,
    /// Time left when paused by a breakdown.
    remaining: Time,
    /// Full length of the timer (for saved states: elapsed = duration - time left).
    duration: Time,
    /// Paused because an operator of the item turned to another case (multitasking):
    /// a repair does not restart it, only the operator's return does.
    held_off: bool,
}

/// An item waiting for all its operators to arrive before `stage` starts.
struct Pending {
    item: ItemId,
    stage: Phase,
    requests: Vec<RequestId>,
    arrived: Vec<(RequestId, BlockId)>,
    /// The stage was already running when an operator was taken away (preemption): when
    /// the crew is complete again its paused timer goes on, no new time is drawn.
    resume: bool,
}

/// Operators currently assigned to an item: (request id, operator block).
type Crew = Vec<(RequestId, BlockId)>;

/// Holds each item for an optional setup time followed by a process time,
/// then releases it. With capacity > 1 items are worked on in parallel.
/// A breakdown pauses every running timer; the remaining time resumes on repair.
///
/// With `operators`, each item asks the team for `count` operators (one request per
/// operator) before a phase that needs them; the timer starts when all have arrived.
/// The operators stay with the item from setup to process when both need them, and
/// are released when the last phase that needs them ends. Every item has its own
/// requests, so capacity > 1 works. A breakdown does not release operators: a broken
/// processor keeps its crew and the paused timers resume on repair.
///
/// **Multitasking operators.** An operator that works on several cases at once sends
/// [`MSG_SUSPEND`] when it turns to another case and [`MSG_RESUME_WORK`] when it comes
/// back (payload: its request id). The item's timer stands still while any of its
/// operators is away, so the work time drawn at the start stays the touch time; the item
/// does not count as a busy server meanwhile.
///
/// **Preempted operators.** An operator taken away for more urgent work sends
/// [`MSG_WITHDRAW`] (payload: its request id): the item's timer stops with the time left
/// and the item waits for an operator again (state `WaitingOperator`). The request goes back
/// to the team's dispatcher with the same id; when an operator arrives for it, the stage
/// goes on with the time left (the work drawn at the start stays the touch time).
pub struct Processor {
    capacity: usize,
    setup: Option<Dist>,
    process: Dist,
    phases: Vec<(ItemId, Phase)>,
    timers: Vec<Timer>,
    /// Operator team and usage.
    operators: Option<(BlockId, OperatorUse)>,
    pending: Vec<Pending>,
    held: Vec<(ItemId, Crew)>,
    /// Servers in setup or processing over time (zero while the block is stopped).
    busy: TimeWeighted,
    /// Servers holding a finished item that cannot leave yet.
    blocked: TimeWeighted,
    /// Initial state: work already done on an item's next phase; its timer draws the
    /// residual time instead of a full duration.
    resume: Vec<(ItemId, Time)>,
    /// Items whose operators turned to other cases or were taken away, with the requests
    /// of the operators that are away.
    suspended: Vec<(ItemId, Vec<RequestId>)>,
}

impl Processor {
    pub fn new(process: Dist) -> Self {
        Processor {
            capacity: 1,
            setup: None,
            process,
            phases: Vec::new(),
            timers: Vec::new(),
            operators: None,
            pending: Vec::new(),
            held: Vec::new(),
            busy: TimeWeighted::default(),
            blocked: TimeWeighted::default(),
            resume: Vec::new(),
            suspended: Vec::new(),
        }
    }

    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    pub fn setup(mut self, setup: Option<Dist>) -> Self {
        self.setup = setup;
        self
    }

    pub fn operators(mut self, operators: Option<(BlockId, OperatorUse)>) -> Self {
        self.operators = operators;
        self
    }

    fn set_phase(&mut self, item: ItemId, phase: Phase) {
        match self.phases.iter_mut().find(|(id, _)| *id == item) {
            Some(entry) => entry.1 = phase,
            None => self.phases.push((item, phase)),
        }
    }

    fn start_timer(&mut self, cx: &mut Ctx, item: ItemId, tag: u32, duration: Time) {
        let held_off = self.is_suspended(item);
        if cx.is_down() || held_off {
            // Operators arrived while stopped: start paused, resume on repair.
            self.timers.push(Timer {
                item,
                tag,
                event: None,
                due: cx.now() + duration,
                remaining: duration,
                duration,
                held_off,
            });
            return;
        }
        let event = Some(cx.schedule(duration, tag, Some(item)));
        self.timers.push(Timer {
            item,
            tag,
            event,
            due: cx.now() + duration,
            remaining: 0.0,
            duration,
            held_off: false,
        });
    }

    fn is_suspended(&self, item: ItemId) -> bool {
        self.suspended.iter().any(|(i, _)| *i == item)
    }

    /// The item an operator request works for (crew complete or still gathering).
    fn item_of_request(&self, id: RequestId) -> Option<ItemId> {
        self.held
            .iter()
            .find(|(_, crew)| crew.iter().any(|(r, _)| *r == id))
            .map(|(i, _)| *i)
            .or_else(|| {
                self.pending
                    .iter()
                    .find(|p| p.arrived.iter().any(|(r, _)| *r == id))
                    .map(|p| p.item)
            })
    }

    /// An operator of the job `id` turned to another case: the item's work stops.
    fn suspend(&mut self, cx: &mut Ctx, id: RequestId) {
        let Some(item) = self.item_of_request(id) else {
            return;
        };
        self.pause(cx, item, id);
    }

    /// Marks the operator of request `id` away from `item` and stops the item's timers.
    fn pause(&mut self, cx: &mut Ctx, item: ItemId, id: RequestId) {
        match self.suspended.iter_mut().find(|(i, _)| *i == item) {
            Some(entry) if !entry.1.contains(&id) => entry.1.push(id),
            Some(_) => {}
            None => self.suspended.push((item, vec![id])),
        }
        let now = cx.now();
        for timer in self.timers.iter_mut().filter(|t| t.item == item) {
            if timer.event.is_some() && timer.due <= now {
                continue; // the work ends at this very instant: let it finish
            }
            timer.held_off = true;
            if let Some(event) = timer.event.take() {
                cx.cancel(event);
                timer.remaining = (timer.due - now).max(0.0);
            }
        }
    }

    /// The operator is back on job `id`: the work goes on once no operator is away.
    fn resume_work(&mut self, cx: &mut Ctx, id: RequestId) {
        let Some(item) = self.item_of_request(id) else {
            return;
        };
        self.unpause(cx, item, id);
    }

    /// The operator of request `id` is back on `item`: its timers go on once no operator
    /// is away.
    fn unpause(&mut self, cx: &mut Ctx, item: ItemId, id: RequestId) {
        let Some(at) = self.suspended.iter().position(|(i, _)| *i == item) else {
            return;
        };
        self.suspended[at].1.retain(|r| *r != id);
        if !self.suspended[at].1.is_empty() {
            return;
        }
        self.suspended.remove(at);
        let now = cx.now();
        let down = cx.is_down();
        for timer in self.timers.iter_mut().filter(|t| t.item == item) {
            timer.held_off = false;
            if !down && timer.event.is_none() {
                timer.event = Some(cx.schedule(timer.remaining, timer.tag, Some(timer.item)));
                timer.due = now + timer.remaining;
            }
        }
    }

    /// The operator of request `id` was taken away for more urgent work: the item waits for
    /// an operator again, with its stage paused (see [`Processor`], preempted operators).
    fn withdraw(&mut self, cx: &mut Ctx, id: RequestId) {
        if let Some(p) = self
            .pending
            .iter_mut()
            .find(|p| p.arrived.iter().any(|(r, _)| *r == id))
        {
            // Still gathering the crew: it waits for this operator again.
            p.arrived.retain(|(r, _)| *r != id);
            return;
        }
        let Some(at) = self
            .held
            .iter()
            .position(|(_, crew)| crew.iter().any(|(r, _)| *r == id))
        else {
            return;
        };
        let (item, crew) = self.held.remove(at);
        let stage = match self.phases.iter().find(|(i, _)| *i == item).map(|p| p.1) {
            Some(Phase::Setup) => Phase::Setup,
            Some(Phase::Processing) => Phase::Processing,
            // Between stages, or done: nothing runs; the crew is simply given back.
            _ => {
                for (request, operator) in crew {
                    if request != id {
                        cx.send(operator, MSG_RELEASE, request.0);
                    }
                }
                return;
            }
        };
        self.pause(cx, item, id);
        self.pending.push(Pending {
            item,
            stage,
            requests: crew.iter().map(|(r, _)| *r).collect(),
            arrived: crew.into_iter().filter(|(r, _)| *r != id).collect(),
            resume: true,
        });
        self.set_phase(item, Phase::WaitingOperator);
    }

    fn needs_operators(&self, stage: Phase) -> bool {
        match (&self.operators, stage) {
            (Some((_, op)), Phase::Setup) => op.setup,
            (Some((_, op)), Phase::Processing) => op.process,
            _ => false,
        }
    }

    /// Starts `stage` for `item`, first getting operators if the stage needs them
    /// and the item does not already hold them.
    fn begin(&mut self, cx: &mut Ctx, item: ItemId, stage: Phase) {
        if self.needs_operators(stage) && !self.held.iter().any(|(i, _)| *i == item) {
            let (team, op) = self.operators.clone().expect("checked by needs_operators");
            let tag = if stage == Phase::Setup {
                SETUP_DONE
            } else {
                PROCESS_DONE
            };
            let me = cx.me();
            let skill = op.requires.as_deref();
            let priority = request_priority(
                cx.item(item),
                op.priority.unwrap_or(0),
                op.priority_label.as_deref(),
            );
            let requests = (0..op.count)
                .map(|_| {
                    cx.request_task_requiring(
                        team,
                        TaskKind::Operate { station: me, tag },
                        priority,
                        skill,
                    )
                })
                .collect();
            self.pending.push(Pending {
                item,
                stage,
                requests,
                arrived: Vec::new(),
                resume: false,
            });
            self.set_phase(item, Phase::WaitingOperator);
            return;
        }
        self.run_stage(cx, item, stage);
    }

    fn run_stage(&mut self, cx: &mut Ctx, item: ItemId, stage: Phase) {
        self.set_phase(item, stage);
        let elapsed = self
            .resume
            .iter()
            .position(|(i, _)| *i == item)
            .map(|at| self.resume.remove(at).1);
        let (dist, tag) = if stage == Phase::Setup {
            let setup = self.setup.as_ref().expect("setup stage needs a setup time");
            (setup, SETUP_DONE)
        } else {
            (&self.process, PROCESS_DONE)
        };
        let mut t = match elapsed {
            Some(e) => cx.sample_residual(dist, e),
            None => cx.sample_duration(dist),
        };
        // People operating: their pace (fatigue, learning, individual pace).
        if let Some((_, crew)) = self.held.iter().find(|(i, _)| *i == item) {
            let crew: Vec<BlockId> = crew.iter().map(|(_, b)| *b).collect();
            let task = if stage == Phase::Setup {
                format!("{}:setup", cx.name())
            } else {
                cx.name().to_string()
            };
            t = cx.person_time(&crew, &task, t);
        }
        let by_people = self.held.iter().any(|(i, _)| *i == item);
        cx.log_work(item, t, by_people);
        self.start_timer(cx, item, tag, t);
    }

    /// Sends MSG_RELEASE to the operators the item holds, if any.
    fn release_operators(&mut self, cx: &mut Ctx, item: ItemId) {
        self.suspended.retain(|(i, _)| *i != item);
        if let Some(at) = self.held.iter().position(|(i, _)| *i == item) {
            for (request, operator) in self.held.remove(at).1 {
                cx.send(operator, MSG_RELEASE, request.0);
            }
        }
    }

    /// With several items inside, the most "productive" phase wins:
    /// processing > setup > waiting for operators > blocked/waiting transport > idle.
    /// So `WaitingOperator` shows only while nothing is being worked on.
    fn update_state(&mut self, cx: &mut Ctx) {
        let has = |p: Phase| {
            self.phases
                .iter()
                .any(|(i, q)| *q == p && !self.is_suspended(*i))
        };
        // An item whose operator turned to another case waits for it like an operator.
        let away = self
            .phases
            .iter()
            .any(|(i, q)| matches!(q, Phase::Setup | Phase::Processing) && self.is_suspended(*i));
        let state = if has(Phase::Processing) {
            State::Processing
        } else if has(Phase::Setup) {
            State::Setup
        } else if away
            || self
                .phases
                .iter()
                .any(|(_, q)| *q == Phase::WaitingOperator)
        {
            State::WaitingOperator
        } else if has(Phase::Done) {
            let waiting = self
                .phases
                .iter()
                .any(|(id, p)| *p == Phase::Done && cx.item(*id).awaiting_transport);
            if waiting {
                State::WaitingTransport
            } else {
                State::Blocked
            }
        } else {
            State::Idle
        };
        cx.set_state(state);
        self.track_servers(cx);
    }

    /// Records how many servers are working and how many hold a blocked item, so the
    /// utilisation of a processor with capacity above 1 is a mean over servers.
    fn track_servers(&mut self, cx: &Ctx) {
        let (busy, blocked) = if cx.is_down() {
            (0, 0)
        } else {
            let working = self
                .phases
                .iter()
                .filter(|(i, p)| {
                    matches!(p, Phase::Setup | Phase::Processing) && !self.is_suspended(*i)
                })
                .count();
            let done = self
                .phases
                .iter()
                .filter(|(_, p)| *p == Phase::Done)
                .count();
            (working, done)
        };
        let now = cx.now();
        self.busy.set(now, busy as f64);
        self.blocked.set(now, blocked as f64);
    }
}

impl Behavior for Processor {
    fn kind(&self) -> &'static str {
        "processor"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.phases.clear();
        self.timers.clear();
        self.pending.clear();
        self.held.clear();
        self.resume.clear();
        self.suspended.clear();
        self.busy = TimeWeighted::default();
        self.blocked = TimeWeighted::default();
        cx.set_state(State::Idle);
    }

    /// Items in setup or processing restart their phase with the residual time; `Done`
    /// items are released at once. With operators, every phase that needs them waits
    /// for a new crew first (the operators start from wherever they are at `t = 0`).
    fn restore(&mut self, cx: &mut Ctx, state: &BlockState) -> Result<(), String> {
        state.check(
            "Processor",
            Accepts {
                items: true,
                phase: true,
                ..Accepts::default()
            },
        )?;
        let n = state.items.len();
        if n > self.capacity {
            return Err(format!("{n} itens, mas a capacidade é {}", self.capacity));
        }
        let first = if self.setup.is_some() {
            Phase::Setup
        } else {
            Phase::Processing
        };
        let ids = cx.contents().to_vec();
        for (k, (&id, it)) in ids.iter().zip(&state.items).enumerate() {
            let stage = match it.phase {
                None => first,
                Some(WorkPhase::Setup) if self.setup.is_none() => {
                    return Err(format!(
                        "item {}: fase Setup, mas o processador não tem setup",
                        k + 1
                    ));
                }
                Some(WorkPhase::Setup) => Phase::Setup,
                Some(WorkPhase::Processing) => Phase::Processing,
                Some(WorkPhase::Done) => Phase::Done,
            };
            if stage == Phase::Done {
                self.set_phase(id, Phase::Done);
                cx.release(id);
                continue;
            }
            if it.elapsed > 0.0 {
                self.resume.push((id, it.elapsed));
            }
            self.begin(cx, id, stage);
        }
        self.update_state(cx);
        Ok(())
    }

    fn save_state(&self, cx: &Ctx, state: &mut BlockState) -> Option<String> {
        let now = cx.now();
        let work = |p: Phase| {
            if p == Phase::Setup {
                WorkPhase::Setup
            } else {
                WorkPhase::Processing
            }
        };
        for (it, &id) in state.items.iter_mut().zip(cx.contents()) {
            let Some(phase) = self.phases.iter().find(|(i, _)| *i == id).map(|p| p.1) else {
                continue;
            };
            match phase {
                Phase::Setup | Phase::Processing => {
                    it.phase = Some(work(phase));
                    it.elapsed = self.timers.iter().find(|t| t.item == id).map_or(0.0, |t| {
                        let left = if t.event.is_some() {
                            (t.due - now).max(0.0)
                        } else {
                            t.remaining
                        };
                        (t.duration - left).max(0.0)
                    });
                }
                Phase::WaitingOperator => {
                    let stage = self
                        .pending
                        .iter()
                        .find(|p| p.item == id)
                        .map_or(Phase::Processing, |p| p.stage);
                    it.phase = Some(work(stage));
                    // A preempted stage keeps its paused timer; one not started yet may
                    // carry the work done before the saved state.
                    it.elapsed = match self.timers.iter().find(|t| t.item == id) {
                        Some(t) => (t.duration - t.remaining).max(0.0),
                        None => self
                            .resume
                            .iter()
                            .find(|(i, _)| *i == id)
                            .map_or(0.0, |r| r.1),
                    };
                }
                Phase::Done => {
                    it.phase = Some(WorkPhase::Done);
                    it.elapsed = 0.0;
                }
            }
        }
        None
    }

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        content < self.capacity
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        let first = if self.setup.is_some() {
            Phase::Setup
        } else {
            Phase::Processing
        };
        self.begin(cx, item, first);
        self.update_state(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        let Some(item) = item else { return };
        self.timers.retain(|t| !(t.item == item && t.tag == tag));
        match tag {
            SETUP_DONE => {
                // Keep the operators only if the process phase needs them too.
                if !self.needs_operators(Phase::Processing) {
                    self.release_operators(cx, item);
                }
                self.begin(cx, item, Phase::Processing);
            }
            PROCESS_DONE => {
                self.release_operators(cx, item);
                self.set_phase(item, Phase::Done);
                cx.release(item);
            }
            _ => return,
        }
        self.update_state(cx);
    }

    fn on_exit(&mut self, cx: &mut Ctx, item: ItemId) {
        self.phases.retain(|(id, _)| *id != item);
        self.resume.retain(|(id, _)| *id != item);
        self.update_state(cx);
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        self.track_servers(cx);
        let now = cx.now();
        for timer in &mut self.timers {
            if let Some(event) = timer.event.take() {
                cx.cancel(event);
                timer.remaining = (timer.due - now).max(0.0);
            }
        }
    }

    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        match tag {
            MSG_TRANSPORT_RESERVED => self.update_state(cx),
            MSG_ARRIVED => {
                let id = RequestId(payload);
                let Some(at) = self.pending.iter().position(|p| p.requests.contains(&id)) else {
                    // Nobody waits for this operator any more: give it back.
                    cx.send(from, MSG_RELEASE, payload);
                    return;
                };
                self.pending[at].arrived.push((id, from));
                if self.pending[at].arrived.len() == self.pending[at].requests.len() {
                    let p = self.pending.remove(at);
                    self.held.push((p.item, p.arrived));
                    if p.resume {
                        // Back from a preemption: the paused stage goes on.
                        self.set_phase(p.item, p.stage);
                        self.unpause(cx, p.item, id);
                    } else {
                        self.run_stage(cx, p.item, p.stage);
                    }
                }
                self.update_state(cx);
            }
            MSG_WITHDRAW => {
                self.withdraw(cx, RequestId(payload));
                self.update_state(cx);
            }
            MSG_SUSPEND => {
                self.suspend(cx, RequestId(payload));
                self.update_state(cx);
            }
            MSG_RESUME_WORK => {
                self.resume_work(cx, RequestId(payload));
                self.update_state(cx);
            }
            _ => {}
        }
    }

    fn on_warmup(&mut self, cx: &mut Ctx) {
        let now = cx.now();
        self.busy.reset(now);
        self.blocked.reset(now);
    }

    fn custom_metrics(&self, now: Time) -> Vec<(String, f64)> {
        vec![
            ("servers.busy".to_string(), self.busy.mean(now)),
            ("servers.blocked".to_string(), self.blocked.mean(now)),
            ("servers.capacity".to_string(), self.capacity as f64),
        ]
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        self.track_servers(cx);
        let now = cx.now();
        for timer in &mut self.timers {
            if timer.event.is_none() && !timer.held_off {
                timer.event = Some(cx.schedule(timer.remaining, timer.tag, Some(timer.item)));
                timer.due = now + timer.remaining;
            }
        }
    }
}
