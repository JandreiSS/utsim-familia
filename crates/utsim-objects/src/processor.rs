use utsim_core::{Behavior, Ctx, Dist, Item, ItemId, State};

const SETUP_DONE: u32 = 0;
const PROCESS_DONE: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Setup,
    Processing,
    Done,
}

/// Holds each item for an optional setup time followed by a process time,
/// then releases it. With capacity > 1 items are worked on in parallel.
pub struct Processor {
    capacity: usize,
    setup: Option<Dist>,
    process: Dist,
    phases: Vec<(ItemId, Phase)>,
}

impl Processor {
    pub fn new(process: Dist) -> Self {
        Processor { capacity: 1, setup: None, process, phases: Vec::new() }
    }

    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    pub fn setup(mut self, setup: Option<Dist>) -> Self {
        self.setup = setup;
        self
    }

    fn set_phase(&mut self, item: ItemId, phase: Phase) {
        match self.phases.iter_mut().find(|(id, _)| *id == item) {
            Some(entry) => entry.1 = phase,
            None => self.phases.push((item, phase)),
        }
    }

    fn start_process(&mut self, cx: &mut Ctx, item: ItemId) {
        self.set_phase(item, Phase::Processing);
        let t = cx.sample_duration(&self.process);
        cx.schedule(t, PROCESS_DONE, Some(item));
    }

    /// With several items inside, the most "productive" phase wins.
    fn update_state(&self, cx: &mut Ctx) {
        let has = |p: Phase| self.phases.iter().any(|(_, q)| *q == p);
        let state = if has(Phase::Processing) {
            State::Processing
        } else if has(Phase::Setup) {
            State::Setup
        } else if has(Phase::Done) {
            State::Blocked
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
        cx.set_state(State::Idle);
    }

    fn accepts(&self, _item: &Item, content: usize) -> bool {
        content < self.capacity
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId) {
        match &self.setup {
            Some(setup) => {
                let t = cx.sample_duration(setup);
                self.set_phase(item, Phase::Setup);
                cx.schedule(t, SETUP_DONE, Some(item));
            }
            None => self.start_process(cx, item),
        }
        self.update_state(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        let Some(item) = item else { return };
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
}
