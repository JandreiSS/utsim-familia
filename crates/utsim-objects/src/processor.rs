use serde::{Deserialize, Serialize};
use utsim_core::task::MSG_TRANSPORT_RESERVED;
use utsim_core::{Behavior, BlockId, Ctx, Dist, EventId, Item, ItemId, State, Time};

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

/// Holds each item for an optional setup time followed by a process time,
/// then releases it. With capacity > 1 items are worked on in parallel.
/// A breakdown pauses every running timer; the remaining time resumes on repair.
pub struct Processor {
    capacity: usize,
    setup: Option<Dist>,
    process: Dist,
    phases: Vec<(ItemId, Phase)>,
    timers: Vec<Timer>,
    /// Operator team and usage (the operator front implements the protocol).
    operators: Option<(BlockId, OperatorUse)>,
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
        let event = Some(cx.schedule(duration, tag, Some(item)));
        self.timers.push(Timer {
            item,
            tag,
            event,
            due: cx.now() + duration,
            remaining: 0.0,
        });
    }

    fn start_process(&mut self, cx: &mut Ctx, item: ItemId) {
        self.set_phase(item, Phase::Processing);
        let t = cx.sample_duration(&self.process);
        self.start_timer(cx, item, PROCESS_DONE, t);
    }

    /// With several items inside, the most "productive" phase wins.
    fn update_state(&self, cx: &mut Ctx) {
        let has = |p: Phase| self.phases.iter().any(|(_, q)| *q == p);
        let state = if has(Phase::Processing) {
            State::Processing
        } else if has(Phase::Setup) {
            State::Setup
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
        cx.set_state(State::Idle);
    }

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        content < self.capacity
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        match &self.setup {
            Some(setup) => {
                let t = cx.sample_duration(setup);
                self.set_phase(item, Phase::Setup);
                self.start_timer(cx, item, SETUP_DONE, t);
            }
            None => self.start_process(cx, item),
        }
        self.update_state(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        let Some(item) = item else { return };
        self.timers.retain(|t| !(t.item == item && t.tag == tag));
        match tag {
            SETUP_DONE => self.start_process(cx, item),
            PROCESS_DONE => {
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

    fn on_message(&mut self, cx: &mut Ctx, _from: BlockId, tag: u32, _payload: u64) {
        if tag == MSG_TRANSPORT_RESERVED {
            self.update_state(cx);
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
