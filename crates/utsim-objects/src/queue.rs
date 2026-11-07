use serde::{Deserialize, Serialize};
use utsim_core::init::Accepts;
use utsim_core::{Behavior, BlockState, Ctx, Dist, EventId, Item, ItemId, State, Time, Value};

use crate::timer::PausableTimer;

const MAX_WAIT: u32 = 0;
/// An item's patience ran out (the item is the event's).
const PATIENCE: u32 = 1;

/// Label set to 1 on an item that gave up waiting (read by the reports before it leaves).
pub const ABANDONED_LABEL: &str = "abandonou";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Discipline {
    #[default]
    Fifo,
    Lifo,
}

/// Order of a queue by a numeric label (an inbox by priority or by deadline). It wins
/// over the [`Discipline`]; ties and items without the label stay in arrival order, the
/// latter after the labelled ones.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelOrder {
    /// Smallest value first (the earliest deadline).
    Asc(String),
    /// Largest value first (the highest priority).
    Desc(String),
}

impl LabelOrder {
    pub fn label(&self) -> &str {
        match self {
            LabelOrder::Asc(l) | LabelOrder::Desc(l) => l,
        }
    }
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
///
/// **Patience** ([`Queue::patience`]). Each item draws its patience when it enters; if it
/// is still here when that time is over (it did not get into the next object), it gives up:
/// it gets the label `abandonou = 1` and leaves the model. The patience runs also while the
/// queue is off shift (the customer waits all the same). An item already promised to a
/// transporter never gives up. The report gets `abandoned.count` and the tally
/// `abandon_wait` (time waited before giving up).
pub struct Queue {
    capacity: Option<usize>,
    discipline: Discipline,
    batch: Option<Batch>,
    order: Option<LabelOrder>,
    /// Items held for the batch being collected.
    held: Vec<ItemId>,
    /// Released batch items that have not left yet.
    outstanding: usize,
    timer: PausableTimer,
    patience: Option<Dist>,
    /// The patience event of each waiting item.
    giving_up: Vec<(ItemId, EventId)>,
    /// Items that gave up since the warm-up.
    abandoned: u64,
}

impl Queue {
    /// `None` means unlimited capacity.
    pub fn new(capacity: Option<usize>) -> Self {
        Queue {
            capacity,
            discipline: Discipline::Fifo,
            batch: None,
            order: None,
            held: Vec::new(),
            outstanding: 0,
            timer: PausableTimer::default(),
            patience: None,
            giving_up: Vec::new(),
            abandoned: 0,
        }
    }

    /// How long an item waits before giving up (see [`Queue`]); `None`: forever.
    pub fn patience(mut self, patience: Option<Dist>) -> Self {
        self.patience = patience;
        self
    }

    /// The item gives up waiting, unless it already left or a transporter was promised it.
    fn give_up(&mut self, cx: &mut Ctx, item: ItemId) {
        self.giving_up.retain(|(i, _)| *i != item);
        if !cx.contents().contains(&item) || cx.item(item).awaiting_transport {
            return;
        }
        let waited: Time = cx.now() - cx.item(item).entered_at;
        cx.item_mut(item)
            .set_label(ABANDONED_LABEL, Value::Num(1.0));
        cx.record("abandon_wait", waited);
        self.abandoned += 1;
        self.held.retain(|i| *i != item);
        cx.destroy(item);
    }

    pub fn discipline(mut self, discipline: Discipline) -> Self {
        self.discipline = discipline;
        self
    }

    pub fn batch(mut self, batch: Option<Batch>) -> Self {
        self.batch = batch;
        self
    }

    /// Items leave by label value with `order` (see [`LabelOrder`]).
    pub fn order(mut self, order: Option<LabelOrder>) -> Self {
        self.order = order;
        self
    }

    fn release_one(&self, cx: &mut Ctx, id: ItemId) {
        match (&self.order, self.discipline) {
            (Some(LabelOrder::Asc(l)), _) => cx.release_ordered(id, l, false),
            (Some(LabelOrder::Desc(l)), _) => cx.release_ordered(id, l, true),
            (None, Discipline::Fifo) => cx.release(id),
            (None, Discipline::Lifo) => cx.release_front(id),
        }
    }

    fn release_held(&mut self, cx: &mut Ctx) {
        self.timer.cancel(cx);
        let held = std::mem::take(&mut self.held);
        self.outstanding += held.len();
        for id in held {
            self.release_one(cx, id);
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
        self.giving_up.clear();
        self.abandoned = 0;
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
        if let Some(p) = &self.patience {
            let p = p.clone();
            let wait = cx.sample_duration(&p);
            let event = cx.schedule(wait, PATIENCE, Some(item));
            self.giving_up.push((item, event));
        }
        let Some(batch) = self.batch else {
            self.release_one(cx, item);
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

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        if tag == PATIENCE {
            if let Some(item) = item {
                self.give_up(cx, item);
                self.update_state(cx);
            }
            return;
        }
        self.timer.clear();
        if tag == MAX_WAIT && !self.held.is_empty() {
            self.release_held(cx);
            self.update_state(cx);
        }
    }

    fn on_exit(&mut self, cx: &mut Ctx, item: ItemId) {
        if let Some(at) = self.giving_up.iter().position(|(i, _)| *i == item) {
            let (_, event) = self.giving_up.remove(at);
            cx.cancel(event);
        }
        self.outstanding = self.outstanding.saturating_sub(1);
        self.update_state(cx);
    }

    fn on_warmup(&mut self, _cx: &mut Ctx) {
        self.abandoned = 0;
    }

    /// With patience: `abandoned.count` (items that gave up since the warm-up).
    fn custom_metrics(&self, _now: Time) -> Vec<(String, f64)> {
        if self.patience.is_none() {
            return Vec::new();
        }
        vec![("abandoned.count".to_string(), self.abandoned as f64)]
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        self.timer.pause(cx);
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        self.timer.resume(cx);
    }
}
