use serde::{Deserialize, Serialize};
use utsim_core::init::Accepts;
use utsim_core::{Behavior, BlockState, Ctx, Dist, EventId, Item, ItemId, State, Time, WorkPhase};

const DWELL_DONE: u32 = 0;

/// Which free position a rack gives to an incoming item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RackPlacement {
    /// Lowest level first, then lowest bay.
    #[default]
    FirstFree,
    /// Uniformly among free positions (block's random stream).
    Random,
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
            slots: vec![None; bays * levels],
            stays: Vec::new(),
        }
    }

    /// The storage position currently holding `item`, if it is stored here.
    pub fn slot_of(&self, item: ItemId) -> Option<usize> {
        self.slots.iter().position(|s| *s == Some(item))
    }

    /// Chooses a free position for the next item, or `None` if the rack is full.
    fn pick_slot(&self, cx: &mut Ctx) -> Option<usize> {
        match self.placement {
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
                    let s = self.pick_slot(cx).expect("capacity checked above");
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

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        content < self.bays * self.levels
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        let Some(slot) = self.pick_slot(cx) else {
            cx.error("porta-paletes cheio: item recebido sem posição livre");
            return;
        };
        self.slots[slot] = Some(item);
        cx.trace_slot(item, slot);
        let duration = self.dwell.as_ref().map(|dwell| cx.sample_duration(dwell));
        match duration {
            Some(d) => self.start_stay(cx, item, d),
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
