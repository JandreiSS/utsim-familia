use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Ctx, Dist, EventId, Item, ItemId, State, Time};

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
            });
        } else {
            let event = cx.schedule(duration, DWELL_DONE, Some(item));
            self.stays.push(Stay {
                item,
                event: Some(event),
                due,
                remaining: 0.0,
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
