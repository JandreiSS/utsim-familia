use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use utsim_core::init::Accepts;
use utsim_core::lugares::{Lugar, Lugares};
use utsim_core::{Behavior, BlockState, Ctx, Dist, EventId, Item, ItemId, State, Time, Value};

use crate::timer::PausableTimer;

const MAX_WAIT: u32 = 0;
/// An item's patience ran out (the item is the event's).
const PATIENCE: u32 = 1;
/// The time of a wave came.
const WAVE: u32 = 2;

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

/// Consolidation: the lines of an order are held until all are here (see [`Queue`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gather {
    /// Label with the order the line belongs to (`pedido`).
    pub group: String,
    /// Label with how many lines the order has (`linhas`).
    pub count: String,
}

/// Wave release: items wait and leave together at the `times` of each `cycle` (see
/// [`Queue`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Waves {
    /// Length of the cycle (1440 for a day in minutes, 10080 for a week).
    pub cycle: f64,
    /// Instants of the waves inside the cycle.
    pub times: Vec<f64>,
}

impl Waves {
    /// Delay from `now` to the next wave (`strict`: a wave exactly at `now` does not count).
    fn next(&self, now: f64, strict: bool) -> Option<f64> {
        if self.cycle.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
            || self.times.is_empty()
        {
            return None;
        }
        let base = (now / self.cycle).floor() * self.cycle;
        let mut best: Option<f64> = None;
        for k in 0..2 {
            for t in &self.times {
                let at = base + k as f64 * self.cycle + t;
                let ok = if strict {
                    at > now + 1e-9
                } else {
                    at >= now - 1e-9
                };
                if ok && best.is_none_or(|b| at < b) {
                    best = Some(at);
                }
            }
        }
        best.map(|at| (at - now).max(0.0))
    }
}

/// Buffer that releases every item as soon as it enters, or, with a [`Batch`],
/// holds items and releases them in groups.
///
/// **Consolidation** ([`Queue::gather`]). A line with the `group` label waits until as
/// many lines of the same group are here as its `count` label says; then the first one
/// leaves as the whole order, with the other lines packed inside it. An item without the
/// label leaves as usual.
///
/// **Waves** ([`Queue::waves`]). Items (whole orders, with consolidation) wait and all
/// leave together at each wave time of the cycle, in the queue's order.
///
/// **Places** ([`Queue::lugares`]): a real waiting area (a table, a pallet, a shelf, a
/// marked floor) whose places are known ([`utsim_core::lugares`]). Each item takes the
/// first free place with support and is traced there (`Slot`); the carrier that takes it
/// goes to the front of its place ([`Ctx::set_pickup_point`]). Only items with nothing on
/// top can leave: among them, the oldest (the newest with `Lifo`; by the label with
/// `order`). An item covered by a new one is taken back from the output until it is free
/// again; an item already promised to a carrier is never covered.
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
    gather: Option<Gather>,
    /// Lines held per order: (group value bits, lines, lines needed).
    groups: Vec<(u64, Vec<ItemId>, usize)>,
    waves: Option<Waves>,
    /// Items waiting for the next wave.
    wave_held: Vec<ItemId>,
    /// Waves released since the warm-up.
    waves_released: u64,
    /// The places of a real waiting area, when it has them.
    lugares: Option<Lugares>,
    /// Order of arrival of the items in the places (the rank of the free ones).
    chegada: HashMap<ItemId, u64>,
    seq: u64,
    /// Items the queue let go that wait for the ones on top to leave.
    liberados: HashSet<ItemId>,
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
            gather: None,
            groups: Vec::new(),
            waves: None,
            wave_held: Vec::new(),
            waves_released: 0,
            lugares: None,
            chegada: HashMap::new(),
            seq: 0,
            liberados: HashSet::new(),
        }
    }

    /// The places of a real waiting area (see [`Queue`]); `None` or empty: none.
    pub fn lugares(mut self, lugares: Option<Vec<Lugar>>) -> Self {
        self.lugares = lugares.filter(|l| !l.is_empty()).map(Lugares::new);
        self
    }

    fn rank(&self, id: ItemId) -> f64 {
        let s = self.chegada.get(&id).copied().unwrap_or(u64::MAX) as f64;
        match self.discipline {
            Discipline::Lifo => -s,
            Discipline::Fifo => s,
        }
    }

    /// The item takes its place; the one it covers is taken back from the output.
    fn place(&mut self, cx: &mut Ctx, id: ItemId) {
        let Some(l) = self.lugares.as_mut() else {
            return;
        };
        self.seq += 1;
        self.chegada.insert(id, self.seq);
        let promised = |i: ItemId| !cx.item(i).awaiting_transport;
        if let Some(k) = l.entrar_com(id, promised) {
            cx.trace_slot(id, k);
        }
        cx.set_pickup_point(id, l.acesso(id));
        if let Some(under) = l.sob(id) {
            cx.withhold(under);
        }
    }

    /// The item left its place: one from beside the area may take it, and the one under it
    /// may go now.
    fn unplace(&mut self, cx: &mut Ctx, id: ItemId) {
        self.liberados.remove(&id);
        self.chegada.remove(&id);
        let Some(l) = self.lugares.as_mut() else {
            return;
        };
        let under = l.sob(id);
        let promised = |i: ItemId| !cx.item(i).awaiting_transport;
        let moved = l.sair_com(id, promised);
        if let Some((m, k)) = moved {
            cx.trace_slot(m, k);
            cx.set_pickup_point(m, l.acesso(m));
            if let Some(u) = l.sob(m) {
                cx.withhold(u);
            }
        }
        if let Some(u) = under
            && self.liberados.contains(&u)
            && self.lugares.as_ref().is_some_and(|l| l.livre(u))
        {
            self.let_go(cx, u);
        }
    }

    /// Ready to leave, in the queue's order.
    fn let_go(&self, cx: &mut Ctx, id: ItemId) {
        match &self.order {
            Some(LabelOrder::Asc(l)) => cx.release_ordered(id, l, false),
            Some(LabelOrder::Desc(l)) => cx.release_ordered(id, l, true),
            None => cx.release_ranked(id, self.rank(id)),
        }
    }

    /// Holds the lines of each order until all are here (see [`Queue`]).
    pub fn gather(mut self, gather: Option<Gather>) -> Self {
        self.gather = gather;
        self
    }

    /// Releases the items in waves (see [`Queue`]).
    pub fn waves(mut self, waves: Option<Waves>) -> Self {
        self.waves = waves;
        self
    }

    /// The item goes on after consolidation: held for the next wave, put in the batch or
    /// released.
    fn admit(&mut self, cx: &mut Ctx, item: ItemId) {
        if self.waves.is_some() {
            self.wave_held.push(item);
            cx.set_state(State::Collecting);
            return;
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

    /// Consolidation of one line: `Some(order)` when it completes an order (the other
    /// lines packed inside it), `None` while the order waits for more lines.
    fn consolidate(&mut self, cx: &mut Ctx, g: &Gather, item: ItemId) -> Option<ItemId> {
        let key = match cx.item(item).label(&g.group).and_then(|v| v.as_num()) {
            Some(v) if v.is_finite() => v.to_bits(),
            _ => return Some(item),
        };
        let needed = cx
            .item(item)
            .label(&g.count)
            .and_then(|v| v.as_num())
            .filter(|v| v.is_finite())
            .map_or(1, |v| v.round().max(1.0) as usize);
        let at = match self.groups.iter().position(|(k, _, _)| *k == key) {
            Some(at) => at,
            None => {
                self.groups.push((key, Vec::new(), needed));
                self.groups.len() - 1
            }
        };
        self.groups[at].1.push(item);
        if self.groups[at].1.len() < self.groups[at].2 {
            return None;
        }
        let (_, lines, _) = self.groups.remove(at);
        let order = lines[0];
        for line in &lines[1..] {
            if let Some(i) = self.giving_up.iter().position(|(x, _)| x == line) {
                let (_, event) = self.giving_up.remove(i);
                cx.cancel(event);
            }
            cx.pack(order, *line);
        }
        Some(order)
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
        // Under another item it cannot get out.
        if self.lugares.as_ref().is_some_and(|l| !l.livre(item)) {
            return;
        }
        let waited: Time = cx.now() - cx.item(item).entered_at;
        cx.item_mut(item)
            .set_label(ABANDONED_LABEL, Value::Num(1.0));
        cx.record("abandon_wait", waited);
        self.abandoned += 1;
        self.held.retain(|i| *i != item);
        self.wave_held.retain(|i| *i != item);
        for (_, lines, _) in &mut self.groups {
            lines.retain(|i| *i != item);
        }
        self.groups.retain(|(_, lines, _)| !lines.is_empty());
        // Destroying does not call `on_exit`: the place is freed here.
        self.unplace(cx, item);
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

    fn release_one(&mut self, cx: &mut Ctx, id: ItemId) {
        if let Some(l) = &self.lugares {
            // Only an item with nothing on top goes now; the others when uncovered.
            self.liberados.insert(id);
            if l.livre(id) {
                self.let_go(cx, id);
            }
            return;
        }
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
        let state =
            if !self.held.is_empty() || !self.wave_held.is_empty() || !self.groups.is_empty() {
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
        self.groups.clear();
        self.wave_held.clear();
        self.waves_released = 0;
        if let Some(l) = self.lugares.as_mut() {
            l.clear();
        }
        self.chegada.clear();
        self.seq = 0;
        self.liberados.clear();
        if let Some(delay) = self.waves.as_ref().and_then(|w| w.next(cx.now(), false)) {
            cx.schedule(delay, WAVE, None);
        }
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

    fn accepts_is_item_independent(&self) -> bool {
        true
    }

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        if self.batch.is_some_and(|b| b.flush) && self.outstanding > 0 {
            return false;
        }
        self.capacity.is_none_or(|cap| content < cap)
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        self.place(cx, item);
        if let Some(p) = &self.patience {
            let p = p.clone();
            let wait = cx.sample_duration(&p);
            let event = cx.schedule(wait, PATIENCE, Some(item));
            self.giving_up.push((item, event));
        }
        let item = match self.gather.clone() {
            Some(g) => match self.consolidate(cx, &g, item) {
                Some(order) => order,
                None => {
                    cx.set_state(State::Collecting);
                    return;
                }
            },
            None => item,
        };
        self.admit(cx, item);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        if tag == PATIENCE {
            if let Some(item) = item {
                self.give_up(cx, item);
                self.update_state(cx);
            }
            return;
        }
        if tag == WAVE {
            let wave = std::mem::take(&mut self.wave_held);
            if !wave.is_empty() {
                self.waves_released += 1;
            }
            for id in wave {
                self.release_one(cx, id);
            }
            if let Some(delay) = self.waves.as_ref().and_then(|w| w.next(cx.now(), true)) {
                cx.schedule(delay, WAVE, None);
            }
            self.update_state(cx);
            return;
        }
        self.timer.clear();
        if tag == MAX_WAIT && !self.held.is_empty() {
            self.release_held(cx);
            self.update_state(cx);
        }
    }

    fn on_exit(&mut self, cx: &mut Ctx, item: ItemId) {
        self.unplace(cx, item);
        if let Some(at) = self.giving_up.iter().position(|(i, _)| *i == item) {
            let (_, event) = self.giving_up.remove(at);
            cx.cancel(event);
        }
        self.outstanding = self.outstanding.saturating_sub(1);
        self.update_state(cx);
    }

    fn on_warmup(&mut self, _cx: &mut Ctx) {
        self.abandoned = 0;
        self.waves_released = 0;
    }

    /// With patience: `abandoned.count` (items that gave up since the warm-up). With
    /// waves: `waves.count` (waves that released something since the warm-up).
    fn custom_metrics(&self, _now: Time) -> Vec<(String, f64)> {
        let mut out = Vec::new();
        if self.patience.is_some() {
            out.push(("abandoned.count".to_string(), self.abandoned as f64));
        }
        if self.waves.is_some() {
            out.push(("waves.count".to_string(), self.waves_released as f64));
        }
        out
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        self.timer.pause(cx);
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        self.timer.resume(cx);
    }
}
