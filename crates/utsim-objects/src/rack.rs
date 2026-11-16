use serde::{Deserialize, Serialize};
use utsim_core::init::Accepts;
use utsim_core::{Behavior, BlockState, Ctx, Dist, EventId, Item, ItemId, State, Time, WorkPhase};

/// Label with the ABC class of an item (1 = A, 2 = B, 3 = C) read by [`RackPlacement::Abc`].
pub const CLASS_LABEL: &str = "classe";

const DWELL_DONE: u32 = 0;

/// Which free position a rack gives to an incoming item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum RackPlacement {
    /// Lowest level first, then lowest bay.
    #[default]
    FirstFree,
    /// Uniformly among free positions (block's random stream).
    Random,
    /// By ABC class (label `classe`: 1 = A, 2 = B, 3 = C; other values and items without
    /// it count as C). The positions are ordered by access time (with [`Access`]; else
    /// lowest level and bay first): class A takes the first fraction `a` of them, B the
    /// next `b`, C the rest. An item takes the best free position of its zone; a full zone
    /// sends it to the next worse one, then to the better ones.
    Abc { a: f64, b: f64 },
}

/// Time to reach a position (put away or pick), in addition to the dwell: `per_bay` for
/// each bay from the front (bay 0 costs nothing) and `per_level` for each level above
/// the floor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Access {
    pub per_bay: f64,
    pub per_level: f64,
}

/// An item stored in the rack whose dwell time is running.
struct Stay {
    item: ItemId,
    /// `None` while paused by a breakdown or off shift.
    event: Option<EventId>,
    due: Time,
    /// Time left when paused.
    remaining: Time,
    /// Full dwell drawn for the item (for saved states).
    duration: Time,
}

/// Addressable storage (phase 5): see `docs/arquitetura.md`, "Porta-paletes".
///
/// Capacity is `bays * levels`. Each item takes a free position (`slot`, numbered
/// `level * bays + bay`), stays at least `dwell` (if set), and is then released in the
/// order it became ready. The position is freed when the item leaves. A breakdown or
/// off shift pauses the dwell timers and resumes them with the time left.
pub struct Rack {
    pub bays: usize,
    pub levels: usize,
    pub dwell: Option<Dist>,
    pub placement: RackPlacement,
    pub access: Option<Access>,
    /// Item in each position, indexed by `level * bays + bay`.
    slots: Vec<Option<ItemId>>,
    stays: Vec<Stay>,
}

impl Rack {
    pub fn new(bays: usize, levels: usize, dwell: Option<Dist>, placement: RackPlacement) -> Self {
        Rack {
            bays,
            levels,
            dwell,
            placement,
            access: None,
            slots: vec![None; bays * levels],
            stays: Vec::new(),
        }
    }

    /// Time to reach the positions (see [`Access`]); `None`: no extra time.
    pub fn access(mut self, access: Option<Access>) -> Self {
        self.access = access;
        self
    }

    /// Access time of position `slot`.
    fn access_time(&self, slot: usize) -> Time {
        let Some(a) = self.access else { return 0.0 };
        let (bay, level) = (slot % self.bays.max(1), slot / self.bays.max(1));
        bay as f64 * a.per_bay + level as f64 * a.per_level
    }

    /// The positions from the cheapest to reach to the dearest (ties: lowest index).
    fn by_cost(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.slots.len()).collect();
        v.sort_by(|x, y| {
            self.access_time(*x)
                .total_cmp(&self.access_time(*y))
                .then(x.cmp(y))
        });
        v
    }

    /// ABC class of an item: 0 (A), 1 (B) or 2 (C).
    fn class_of(cx: &Ctx, item: ItemId) -> usize {
        match cx.item(item).label(CLASS_LABEL).and_then(|v| v.as_num()) {
            Some(v) if (v - 1.0).abs() < 0.5 => 0,
            Some(v) if (v - 2.0).abs() < 0.5 => 1,
            _ => 2,
        }
    }

    /// Best free position for an item of class `class` with zones `a` and `b`.
    fn abc_slot(&self, class: usize, a: f64, b: f64) -> Option<usize> {
        let order = self.by_cost();
        let n = order.len();
        let na = ((a.clamp(0.0, 1.0) * n as f64).ceil() as usize).min(n);
        let nb = ((b.clamp(0.0, 1.0) * n as f64).ceil() as usize).min(n - na);
        let zones = [&order[..na], &order[na..na + nb], &order[na + nb..]];
        // Own zone, then the worse ones, then the better ones.
        let mut try_order: Vec<usize> = (class..3).collect();
        try_order.extend((0..class).rev());
        try_order
            .into_iter()
            .find_map(|z| zones[z].iter().copied().find(|s| self.slots[*s].is_none()))
    }

    /// The storage position currently holding `item`, if it is stored here.
    pub fn slot_of(&self, item: ItemId) -> Option<usize> {
        self.slots.iter().position(|s| *s == Some(item))
    }

    /// Chooses a free position for `item`, or `None` if the rack is full.
    fn pick_slot(&self, cx: &mut Ctx, item: ItemId) -> Option<usize> {
        match self.placement {
            RackPlacement::Abc { a, b } => self.abc_slot(Self::class_of(cx, item), a, b),
            RackPlacement::FirstFree => self.slots.iter().position(|s| s.is_none()),
            RackPlacement::Random => {
                let free: Vec<usize> = self
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.is_none())
                    .map(|(i, _)| i)
                    .collect();
                if free.is_empty() {
                    return None;
                }
                // Uniform draw in [0, 1) from the block's own stream.
                let u = cx.sample(&Dist::Uniform { min: 0.0, max: 1.0 });
                let k = ((u * free.len() as f64) as usize).min(free.len() - 1);
                Some(free[k])
            }
        }
    }

    fn start_stay(&mut self, cx: &mut Ctx, item: ItemId, duration: Time) {
        let due = cx.now() + duration;
        if cx.is_down() {
            // Entered while stopped: start paused, resume on repair.
            self.stays.push(Stay {
                item,
                event: None,
                due,
                remaining: duration,
                duration,
            });
        } else {
            let event = cx.schedule(duration, DWELL_DONE, Some(item));
            self.stays.push(Stay {
                item,
                event: Some(event),
                due,
                remaining: 0.0,
                duration,
            });
        }
    }

    fn update_state(&self, cx: &mut Ctx) {
        let state = if cx.content_len() == 0 {
            State::Empty
        } else {
            State::Releasing
        };
        cx.set_state(state);
    }
}

impl Behavior for Rack {
    fn kind(&self) -> &'static str {
        "rack"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.slots = vec![None; self.bays * self.levels];
        self.stays.clear();
        cx.set_state(State::Empty);
    }

    /// Items with a `slot` take it; the others follow the placement rule. The dwell
    /// draws the residual of `dwell` given `elapsed`; `phase: Done` (or no dwell)
    /// releases the item at once.
    fn restore(&mut self, cx: &mut Ctx, state: &BlockState) -> Result<(), String> {
        state.check(
            "Rack",
            Accepts {
                items: true,
                slot: true,
                phase: true,
                ..Accepts::default()
            },
        )?;
        let cap = self.bays * self.levels;
        let n = state.items.len();
        if n > cap {
            return Err(format!("{n} itens, mas o porta-paletes tem {cap} posições"));
        }
        let ids = cx.contents().to_vec();
        for (k, (&id, it)) in ids.iter().zip(&state.items).enumerate() {
            if matches!(it.phase, Some(WorkPhase::Setup | WorkPhase::Processing)) {
                return Err(format!(
                    "item {}: no porta-paletes só vale phase: Done (permanência mínima cumprida)",
                    k + 1
                ));
            }
            if let Some(s) = it.slot {
                if s >= cap {
                    return Err(format!(
                        "item {}: slot {s} não existe (posições 0..{})",
                        k + 1,
                        cap - 1
                    ));
                }
                if self.slots[s].is_some() {
                    return Err(format!("item {}: slot {s} usado por dois itens", k + 1));
                }
                self.slots[s] = Some(id);
            }
        }
        for (&id, it) in ids.iter().zip(&state.items) {
            let slot = match it.slot {
                Some(s) => s,
                None => {
                    let s = self.pick_slot(cx, id).expect("capacity checked above");
                    self.slots[s] = Some(id);
                    s
                }
            };
            cx.trace_slot(id, slot);
            match (&self.dwell, it.phase) {
                (Some(dwell), None) => {
                    let dwell = dwell.clone();
                    let d = cx.sample_residual(&dwell, it.elapsed);
                    self.start_stay(cx, id, d);
                }
                _ => cx.release(id),
            }
        }
        self.update_state(cx);
        Ok(())
    }

    fn save_state(&self, cx: &Ctx, state: &mut BlockState) -> Option<String> {
        let now = cx.now();
        for (it, &id) in state.items.iter_mut().zip(cx.contents()) {
            it.slot = self.slot_of(id);
            match self.stays.iter().find(|s| s.item == id) {
                Some(stay) => {
                    let left = if stay.event.is_some() {
                        (stay.due - now).max(0.0)
                    } else {
                        stay.remaining
                    };
                    it.elapsed = (stay.duration - left).max(0.0);
                }
                None if self.dwell.is_some() => it.phase = Some(WorkPhase::Done),
                None => {}
            }
        }
        None
    }

    /// Stored items are not work: a rack goes off shift at once, as with `Pause`.
    fn work_in_progress(&self, _cx: &Ctx) -> bool {
        false
    }

    fn accepts_is_item_independent(&self) -> bool {
        true
    }

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        content < self.bays * self.levels
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        let Some(slot) = self.pick_slot(cx, item) else {
            cx.error("porta-paletes cheio: item recebido sem posição livre");
            return;
        };
        self.slots[slot] = Some(item);
        cx.trace_slot(item, slot);
        let duration = self.dwell.as_ref().map(|dwell| cx.sample_duration(dwell));
        let access = self.access_time(slot);
        if self.access.is_some() {
            cx.record("access_time", access);
            let class = ["A", "B", "C"][Self::class_of(cx, item)];
            cx.record(&format!("access_time.{class}"), access);
        }
        match duration {
            Some(d) => self.start_stay(cx, item, d + access),
            None if access > 0.0 => self.start_stay(cx, item, access),
            None => cx.release(item),
        }
        self.update_state(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        if tag != DWELL_DONE {
            return;
        }
        let Some(item) = item else { return };
        if let Some(at) = self.stays.iter().position(|s| s.item == item) {
            self.stays.remove(at);
            cx.release(item);
        }
    }

    fn on_exit(&mut self, cx: &mut Ctx, item: ItemId) {
        for slot in self.slots.iter_mut() {
            if *slot == Some(item) {
                *slot = None;
            }
        }
        self.stays.retain(|s| s.item != item);
        self.update_state(cx);
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        let now = cx.now();
        for stay in &mut self.stays {
            if let Some(event) = stay.event.take() {
                cx.cancel(event);
                stay.remaining = (stay.due - now).max(0.0);
            }
        }
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        let now = cx.now();
        for stay in &mut self.stays {
            if stay.event.is_none() {
                stay.due = now + stay.remaining;
                stay.event = Some(cx.schedule(stay.remaining, DWELL_DONE, Some(stay.item)));
            }
        }
    }
}
