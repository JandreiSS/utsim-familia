use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Ctx, Dist, Item, ItemId, State};

use crate::timer::PausableTimer;

const SETUP_DONE: u32 = 0;
const PROCESS_DONE: u32 = 1;

/// What the combiner does with the components once the set is processed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombineMode {
    /// Components are packed inside the container; only the container leaves.
    #[default]
    Pack,
    /// Components are destroyed; the container leaves (e.g. a part gets a screw).
    Join,
    /// Container and components all leave, container first, still separate.
    Batch,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Collecting,
    Setup,
    Processing,
    /// Everything was released; waiting for it all to leave.
    Releasing,
}

/// Input 0 receives the container, inputs 1..=k receive components. When the
/// set is complete it works for `setup` + `process`, then releases per [`CombineMode`].
/// One set at a time: nothing new is accepted until the previous set has left.
pub struct Combiner {
    needed: Vec<usize>,
    mode: CombineMode,
    setup: Option<Dist>,
    process: Dist,
    container: Option<ItemId>,
    components: Vec<ItemId>,
    received: Vec<usize>,
    phase: Phase,
    /// Released items that have not left yet.
    outstanding: usize,
    timer: PausableTimer,
}

impl Combiner {
    /// `needed[j]` is how many items to take from input port `j + 1`.
    pub fn new(needed: Vec<usize>, mode: CombineMode, process: Dist) -> Self {
        let received = vec![0; needed.len()];
        Combiner {
            needed,
            mode,
            setup: None,
            process,
            container: None,
            components: Vec::new(),
            received,
            phase: Phase::Collecting,
            outstanding: 0,
            timer: PausableTimer::default(),
        }
    }

    pub fn setup(mut self, setup: Option<Dist>) -> Self {
        self.setup = setup;
        self
    }

    fn complete(&self) -> bool {
        self.container.is_some() && self.received.iter().zip(&self.needed).all(|(r, n)| r >= n)
    }

    fn reset_set(&mut self) {
        self.container = None;
        self.components.clear();
        self.received.iter_mut().for_each(|r| *r = 0);
        self.phase = Phase::Collecting;
        self.outstanding = 0;
    }

    fn update_state(&self, cx: &mut Ctx) {
        let state = match self.phase {
            Phase::Setup => State::Setup,
            Phase::Processing => State::Processing,
            Phase::Releasing => State::Blocked,
            Phase::Collecting if self.container.is_none() && self.components.is_empty() => {
                State::Idle
            }
            Phase::Collecting => State::Collecting,
        };
        cx.set_state(state);
    }

    fn start_process(&mut self, cx: &mut Ctx) {
        self.phase = Phase::Processing;
        let t = cx.sample_duration(&self.process);
        self.timer.start(cx, t, PROCESS_DONE, None);
    }

    fn finish(&mut self, cx: &mut Ctx) {
        let container = self.container.expect("set is complete");
        let components = std::mem::take(&mut self.components);
        match self.mode {
            CombineMode::Pack => {
                for c in components {
                    cx.pack(container, c);
                }
                cx.release(container);
                self.outstanding = 1;
            }
            CombineMode::Join => {
                for c in components {
                    cx.destroy(c);
                }
                cx.release(container);
                self.outstanding = 1;
            }
            CombineMode::Batch => {
                self.outstanding = 1 + components.len();
                cx.release(container);
                for c in components {
                    cx.release(c);
                }
            }
        }
        self.phase = Phase::Releasing;
    }
}

impl Behavior for Combiner {
    fn kind(&self) -> &'static str {
        "combiner"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.reset_set();
        self.timer = PausableTimer::default();
        cx.set_state(State::Idle);
    }

    fn accepts(&self, _item: &Item, _content: usize, port: usize) -> bool {
        if port == 0 {
            self.container.is_none()
        } else {
            self.received
                .get(port - 1)
                .zip(self.needed.get(port - 1))
                .is_some_and(|(r, n)| r < n)
        }
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, port: usize) {
        if port == 0 {
            self.container = Some(item);
        } else {
            self.components.push(item);
            self.received[port - 1] += 1;
        }
        if self.phase == Phase::Collecting && self.complete() {
            match &self.setup {
                Some(setup) => {
                    self.phase = Phase::Setup;
                    let t = cx.sample_duration(setup);
                    self.timer.start(cx, t, SETUP_DONE, None);
                }
                None => self.start_process(cx),
            }
        }
        self.update_state(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _item: Option<ItemId>) {
        self.timer.clear();
        match tag {
            SETUP_DONE => self.start_process(cx),
            PROCESS_DONE => self.finish(cx),
            _ => return,
        }
        self.update_state(cx);
    }

    fn on_exit(&mut self, cx: &mut Ctx, _item: ItemId) {
        self.outstanding = self.outstanding.saturating_sub(1);
        if self.outstanding == 0 {
            self.reset_set();
        }
        self.update_state(cx);
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        self.timer.pause(cx);
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        self.timer.resume(cx);
    }
}
