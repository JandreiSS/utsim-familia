use serde::{Deserialize, Serialize};
use utsim_core::task::{MSG_ARRIVED, MSG_RELEASE, MSG_TRANSPORT_RESERVED};
use utsim_core::{
    Behavior, BlockId, Ctx, Dist, EventId, Item, ItemId, RequestId, State, TaskKind, Time,
};

/// Operators a processor needs (phase 4, contract): `count` members of `team`
/// (a transporter or a dispatcher), during setup and/or processing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperatorUse {
    pub team: String,
    #[serde(default = "one")]
    pub count: usize,
    #[serde(default = "yes")]
    pub setup: bool,
    #[serde(default = "yes")]
    pub process: bool,
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
}

/// An item waiting for all its operators to arrive before `stage` starts.
struct Pending {
    item: ItemId,
    stage: Phase,
    requests: Vec<RequestId>,
    arrived: Vec<(RequestId, BlockId)>,
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
        if cx.is_down() {
            // Operators arrived while stopped: start paused, resume on repair.
            self.timers.push(Timer {
                item,
                tag,
                event: None,
                due: cx.now() + duration,
                remaining: duration,
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
        });
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
            let requests = (0..op.count)
                .map(|_| cx.request_task(team, TaskKind::Operate { station: me, tag }, 0))
                .collect();
            self.pending.push(Pending {
                item,
                stage,
                requests,
                arrived: Vec::new(),
            });
            self.set_phase(item, Phase::WaitingOperator);
            return;
        }
        self.run_stage(cx, item, stage);
    }

    fn run_stage(&mut self, cx: &mut Ctx, item: ItemId, stage: Phase) {
        self.set_phase(item, stage);
        if stage == Phase::Setup {
            let setup = self.setup.clone().expect("setup stage needs a setup time");
            let t = cx.sample_duration(&setup);
            self.start_timer(cx, item, SETUP_DONE, t);
        } else {
            let t = cx.sample_duration(&self.process);
            self.start_timer(cx, item, PROCESS_DONE, t);
        }
    }

    /// Sends MSG_RELEASE to the operators the item holds, if any.
    fn release_operators(&mut self, cx: &mut Ctx, item: ItemId) {
        if let Some(at) = self.held.iter().position(|(i, _)| *i == item) {
            for (request, operator) in self.held.remove(at).1 {
                cx.send(operator, MSG_RELEASE, request.0);
            }
        }
    }

    /// With several items inside, the most "productive" phase wins:
    /// processing > setup > waiting for operators > blocked/waiting transport > idle.
    /// So `WaitingOperator` shows only while nothing is being worked on.
    fn update_state(&self, cx: &mut Ctx) {
        let has = |p: Phase| self.phases.iter().any(|(_, q)| *q == p);
        let state = if has(Phase::Processing) {
            State::Processing
        } else if has(Phase::Setup) {
            State::Setup
        } else if has(Phase::WaitingOperator) {
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
        cx.set_state(State::Idle);
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
        self.update_state(cx);
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
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
                    self.run_stage(cx, p.item, p.stage);
                }
                self.update_state(cx);
            }
            _ => {}
        }
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        let now = cx.now();
        for timer in &mut self.timers {
            if timer.event.is_none() {
                timer.event = Some(cx.schedule(timer.remaining, timer.tag, Some(timer.item)));
                timer.due = now + timer.remaining;
            }
        }
    }
}
