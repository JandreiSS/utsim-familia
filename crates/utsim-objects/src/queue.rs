use serde::{Deserialize, Serialize};
use utsim_core::init::Accepts;
use utsim_core::{Behavior, BlockState, Ctx, Item, ItemId, State};

use crate::timer::PausableTimer;

const MAX_WAIT: u32 = 0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Discipline {
    #[default]
    Fifo,
    Lifo,
}

/// Batching: items are held and released together.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    /// Items that complete a batch.
    pub size: usize,
    /// Releases a partial batch this long after its first item (0 = wait forever).
    #[serde(default)]
    pub max_wait: f64,
    /// Close the input after a batch is released until all its items have left.
    /// Otherwise the next batch is collected meanwhile.
    #[serde(default)]
    pub flush: bool,
}

/// Buffer that releases every item as soon as it enters, or, with a [`Batch`],
/// holds items and releases them in groups.
pub struct Queue {
    capacity: Option<usize>,
    discipline: Discipline,
    batch: Option<Batch>,
    /// Items held for the batch being collected.
    held: Vec<ItemId>,
    /// Released batch items that have not left yet.
    outstanding: usize,
    timer: PausableTimer,
}

impl Queue {
    /// `None` means unlimited capacity.
    pub fn new(capacity: Option<usize>) -> Self {
        Queue {
            capacity,
            discipline: Discipline::Fifo,
            batch: None,
            held: Vec::new(),
            outstanding: 0,
            timer: PausableTimer::default(),
        }
    }

    pub fn discipline(mut self, discipline: Discipline) -> Self {
        self.discipline = discipline;
        self
    }

    pub fn batch(mut self, batch: Option<Batch>) -> Self {
        self.batch = batch;
        self
    }

    fn release_held(&mut self, cx: &mut Ctx) {
        self.timer.cancel(cx);
        let held = std::mem::take(&mut self.held);
        self.outstanding += held.len();
        for id in held {
            match self.discipline {
                Discipline::Fifo => cx.release(id),
                Discipline::Lifo => cx.release_front(id),
            }
        }
    }

    fn update_state(&self, cx: &mut Ctx) {
        let state = if !self.held.is_empty() {
            State::Collecting
        } else if cx.content_len() > 0 {
            State::Releasing
        } else {
            State::Empty
        };
        cx.set_state(state);
    }
}

impl Behavior for Queue {
    fn kind(&self) -> &'static str {
        "queue"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.held.clear();
        self.outstanding = 0;
        self.timer = PausableTimer::default();
        cx.set_state(State::Empty);
    }

    /// The items enter in order, as if they had just arrived (batches are collected
    /// again); the wait for a partial batch counts from the first held item's `elapsed`.
    fn restore(&mut self, cx: &mut Ctx, state: &BlockState) -> Result<(), String> {
        state.check(
            "Queue",
            Accepts {
                items: true,
                ..Accepts::default()
            },
        )?;
        let n = state.items.len();
        if let Some(cap) = self.capacity.filter(|cap| n > *cap) {
            return Err(format!("{n} itens, mas a capacidade é {cap}"));
        }
        let ids = cx.contents().to_vec();
        for &id in &ids {
            self.on_entry(cx, id, 0);
        }
        if let (Some(batch), Some(first)) = (self.batch, self.held.first()) {
            if batch.max_wait > 0.0 {
                let k = ids.iter().position(|id| id == first).unwrap_or(0);
                let left = (batch.max_wait - state.items[k].elapsed).max(0.0);
                self.timer.cancel(cx);
                self.timer.start(cx, left, MAX_WAIT, None);
            }
        }
        Ok(())
    }

    /// The kernel already wrote the items with their waits; nothing else to add.
    fn save_state(&self, _cx: &Ctx, _state: &mut BlockState) -> Option<String> {
        None
    }

    /// Waiting items are not work: at the end of a shift with `Finish` a queue goes off
    /// shift at once, as with `Pause`.
    fn work_in_progress(&self, _cx: &Ctx) -> bool {
        false
    }

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        if self.batch.is_some_and(|b| b.flush) && self.outstanding > 0 {
            return false;
        }
        self.capacity.is_none_or(|cap| content < cap)
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        let Some(batch) = self.batch else {
            match self.discipline {
                Discipline::Fifo => cx.release(item),
                Discipline::Lifo => cx.release_front(item),
            }
            cx.set_state(State::Releasing);
            return;
        };
        self.held.push(item);
        if self.held.len() >= batch.size {
            self.release_held(cx);
        } else if self.held.len() == 1 && batch.max_wait > 0.0 {
            self.timer.start(cx, batch.max_wait, MAX_WAIT, None);
        }
        self.update_state(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _item: Option<ItemId>) {
        self.timer.clear();
        if tag == MAX_WAIT && !self.held.is_empty() {
            self.release_held(cx);
            self.update_state(cx);
        }
    }

    fn on_exit(&mut self, cx: &mut Ctx, _item: ItemId) {
        self.outstanding = self.outstanding.saturating_sub(1);
        self.update_state(cx);
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        self.timer.pause(cx);
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        self.timer.resume(cx);
    }
}
