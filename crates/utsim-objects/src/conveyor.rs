use utsim_core::init::Accepts;
use utsim_core::{Behavior, BlockState, Ctx, EventId, Item, ItemId, State, Time};

/// The front item reached the end of the belt.
const ARRIVE: u32 = 0;
/// The entry zone (the first `item_length` of the belt) cleared.
const REOPEN: u32 = 1;

/// One item on the belt: distance from the start `pos` as of time `since`.
struct OnBelt {
    id: ItemId,
    pos: f64,
    since: Time,
}

/// Kinematic conveyor (phase 5): see `docs/arquitetura.md`, "Esteira".
///
/// Internal model: every item stores `(pos, since)`, its distance from the start at
/// the last update. While the belt moves, the position at time `t` is
/// `min(pos + speed * (t - since), target(k))`, where `k` is the item's index counted
/// from the front and `target(k) = length - k * item_length` is where it stops when
/// the items ahead are stopped. Before anything changes (an entry, an exit, an
/// arrival, a stop or a resume) all positions are brought up to date (`sync`), so
/// stopped time is never counted. Only two events exist at a time: the front item
/// reaching the end, and the entry zone clearing; both are recomputed from the model
/// after every change, which makes breakdowns trivial (no timers to pause).
///
/// - Accumulating: items behind a stopped front item stop at their targets.
/// - Non-accumulating: the whole belt stops while the front item waits at the end.
pub struct Conveyor {
    pub length: f64,
    pub speed: f64,
    pub item_length: f64,
    pub accumulating: bool,
    /// Items on the belt, front first (same order as the block content).
    items: Vec<OnBelt>,
    /// The front item reached the end and waits to leave.
    front_arrived: bool,
    /// Stopped by a breakdown or shift.
    halted: bool,
    /// The entry zone is clear (the last item that entered moved at least `item_length`).
    entry_open: bool,
    arrive_event: Option<EventId>,
    reopen_event: Option<EventId>,
}

impl Conveyor {
    pub fn new(length: f64, speed: f64, item_length: f64, accumulating: bool) -> Self {
        Conveyor {
            length,
            speed,
            item_length,
            accumulating,
            items: Vec::new(),
            front_arrived: false,
            halted: false,
            entry_open: true,
            arrive_event: None,
            reopen_event: None,
        }
    }

    /// Items that fit on the belt at once.
    pub fn capacity(&self) -> usize {
        (self.length / self.item_length).floor() as usize
    }

    /// Distances from the start of every item at `now`, front item first.
    /// Valid for any `now` at or after the last update; meant for the 3D view and tests.
    pub fn positions(&self, now: Time) -> Vec<(ItemId, f64)> {
        (0..self.items.len())
            .map(|k| (self.items[k].id, self.pos_at(k, now)))
            .collect()
    }

    fn eps(&self) -> f64 {
        1e-9 * self.length.max(1.0)
    }

    /// Where the item with index `k` (0 = front) stops if the ones ahead are stopped.
    fn target(&self, k: usize) -> f64 {
        self.length - k as f64 * self.item_length
    }

    /// The belt is moving: not halted, and (non-accumulating) the front is not waiting.
    /// An accumulating belt with a waiting front still moves the items behind it.
    fn is_moving(&self) -> bool {
        !self.halted && !(self.front_arrived && !self.accumulating)
    }

    fn pos_at(&self, k: usize, now: Time) -> f64 {
        let it = &self.items[k];
        if !self.is_moving() {
            return it.pos;
        }
        (it.pos + self.speed * (now - it.since).max(0.0)).min(self.target(k))
    }

    /// Brings every item up to date. Call before changing any flag or the item list.
    fn sync(&mut self, now: Time) {
        for k in 0..self.items.len() {
            let p = self.pos_at(k, now);
            self.items[k].pos = p;
            self.items[k].since = now;
        }
    }

    fn update_state(&self, cx: &mut Ctx) {
        let state = if self.items.is_empty() {
            State::Empty
        } else if self.front_arrived {
            State::Blocked
        } else {
            State::Releasing
        };
        cx.set_state(state);
    }

    /// Recomputes the two pending events (and the entry flag) from the model.
    /// Expects `sync` to have run at `cx.now()`.
    fn reschedule(&mut self, cx: &mut Ctx) {
        if let Some(e) = self.arrive_event.take() {
            cx.cancel(e);
        }
        if let Some(e) = self.reopen_event.take() {
            cx.cancel(e);
        }
        let moving = self.is_moving();
        if moving && !self.front_arrived {
            if let Some(front) = self.items.first() {
                let delay = (self.length - front.pos) / self.speed;
                self.arrive_event = Some(cx.schedule(delay, ARRIVE, Some(front.id)));
            }
        }
        let was_open = self.entry_open;
        match self.items.last() {
            None => self.entry_open = true,
            Some(last) if last.pos >= self.item_length - self.eps() => self.entry_open = true,
            Some(last) => {
                self.entry_open = false;
                if moving {
                    let delay = (self.item_length - last.pos) / self.speed;
                    self.reopen_event = Some(cx.schedule(delay, REOPEN, None));
                }
            }
        }
        if self.entry_open && !was_open {
            cx.wake_inputs();
        }
    }
}

impl Behavior for Conveyor {
    fn kind(&self) -> &'static str {
        "conveyor"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.items.clear();
        self.front_arrived = false;
        self.halted = false;
        self.entry_open = true;
        self.arrive_event = None;
        self.reopen_event = None;
        cx.set_state(State::Empty);
    }

    /// Items are listed front first, each with its distance from the start (`at`);
    /// without it they are queued at the end, as on a blocked belt.
    fn restore(&mut self, cx: &mut Ctx, state: &BlockState) -> Result<(), String> {
        state.check(
            "Conveyor",
            Accepts {
                items: true,
                at: true,
                ..Accepts::default()
            },
        )?;
        let n = state.items.len();
        let cap = self.capacity();
        if n > cap {
            return Err(format!(
                "{n} itens, mas cabem {cap} na esteira (length / item_length)"
            ));
        }
        let now = cx.now();
        let eps = self.eps();
        let ids = cx.contents().to_vec();
        let mut ahead: Option<f64> = None;
        for (k, (&id, it)) in ids.iter().zip(&state.items).enumerate() {
            let pos = it.at.unwrap_or_else(|| self.target(k));
            if pos < -eps || pos > self.length + eps {
                return Err(format!(
                    "item {}: at = {pos} fica fora da esteira [0, {}]",
                    k + 1,
                    self.length
                ));
            }
            if let Some(front) = ahead {
                if pos > front - self.item_length + eps {
                    return Err(format!(
                        "item {}: at = {pos} precisa ficar pelo menos item_length ({}) atrás do item da frente (at = {front}); liste os itens da frente para trás",
                        k + 1,
                        self.item_length
                    ));
                }
            }
            ahead = Some(pos);
            self.items.push(OnBelt {
                id,
                pos: pos.clamp(0.0, self.length),
                since: now,
            });
        }
        self.reschedule(cx);
        self.update_state(cx);
        Ok(())
    }

    fn save_state(&self, cx: &Ctx, state: &mut BlockState) -> Option<String> {
        let positions = self.positions(cx.now());
        for (it, id) in state.items.iter_mut().zip(cx.contents()) {
            it.at = positions.iter().find(|(i, _)| i == id).map(|p| p.1);
        }
        None
    }

    fn accepts_is_item_independent(&self) -> bool {
        true
    }

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        self.entry_open && content < self.capacity()
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        let now = cx.now();
        self.sync(now);
        self.items.push(OnBelt {
            id: item,
            pos: 0.0,
            since: now,
        });
        self.reschedule(cx);
        self.update_state(cx);
    }

    fn on_exit(&mut self, cx: &mut Ctx, item: ItemId) {
        let now = cx.now();
        self.sync(now);
        if let Some(at) = self.items.iter().position(|o| o.id == item) {
            self.items.remove(at);
            if at == 0 {
                self.front_arrived = false;
            }
        }
        self.reschedule(cx);
        self.update_state(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        let now = cx.now();
        self.sync(now);
        match tag {
            ARRIVE => {
                self.arrive_event = None;
                let length = self.length;
                let Some(front) = self.items.first_mut() else {
                    return;
                };
                if Some(front.id) != item {
                    return;
                }
                // The event was computed for exactly this moment: snap away rounding.
                front.pos = length;
                let id = front.id;
                self.front_arrived = true;
                cx.release(id);
            }
            REOPEN => {
                self.reopen_event = None;
                let s = self.item_length;
                if let Some(last) = self.items.last_mut() {
                    last.pos = last.pos.max(s);
                }
            }
            _ => return,
        }
        self.reschedule(cx);
        self.update_state(cx);
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        self.sync(cx.now());
        self.halted = true;
        self.reschedule(cx);
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        self.sync(cx.now());
        self.halted = false;
        self.reschedule(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L = 10, v = 2, s = 2 (capacity 5); items front first, positions at t = 0.
    fn belt_with(accumulating: bool, positions: &[f64]) -> Conveyor {
        let mut c = Conveyor::new(10.0, 2.0, 2.0, accumulating);
        for &pos in positions {
            c.items.push(OnBelt {
                id: ItemId::default(),
                pos,
                since: 0.0,
            });
        }
        c
    }

    fn at(c: &Conveyor, t: f64) -> Vec<f64> {
        c.positions(t).iter().map(|x| x.1).collect()
    }

    #[test]
    fn positions_move_at_speed_and_stack_behind_the_front() {
        let c = belt_with(true, &[4.0, 2.0, 0.0]);
        // After 1 time unit: 4 + 2 = 6, 4, 2. After 10: all stopped at their targets.
        assert_eq!(at(&c, 1.0), vec![6.0, 4.0, 2.0]);
        assert_eq!(at(&c, 10.0), vec![10.0, 8.0, 6.0]);
    }

    #[test]
    fn halted_belt_keeps_positions() {
        let mut c = belt_with(true, &[4.0]);
        c.halted = true;
        assert_eq!(at(&c, 100.0), vec![4.0]);
    }

    #[test]
    fn non_accumulating_belt_stops_while_front_waits() {
        let mut c = belt_with(false, &[10.0, 6.0]);
        c.front_arrived = true;
        assert_eq!(at(&c, 5.0), vec![10.0, 6.0]);
        // The accumulating belt lets the second item advance to 8.
        let mut a = belt_with(true, &[10.0, 6.0]);
        a.front_arrived = true;
        assert_eq!(at(&a, 5.0), vec![10.0, 8.0]);
    }
}
