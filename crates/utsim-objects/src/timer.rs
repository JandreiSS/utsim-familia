use utsim_core::{Ctx, EventId, ItemId, Time};

/// A single scheduled event that a breakdown can pause and resume.
/// Call [`PausableTimer::clear`] when the event fires.
#[derive(Default)]
pub(crate) struct PausableTimer {
    active: bool,
    event: Option<EventId>,
    tag: u32,
    item: Option<ItemId>,
    due: Time,
    /// Time left when paused by a breakdown.
    remaining: Time,
}

impl PausableTimer {
    pub fn start(&mut self, cx: &mut Ctx, delay: Time, tag: u32, item: Option<ItemId>) {
        self.active = true;
        self.tag = tag;
        self.item = item;
        self.due = cx.now() + delay;
        self.event = Some(cx.schedule(delay, tag, item));
    }

    /// Forgets the timer without cancelling (use after the event fired).
    pub fn clear(&mut self) {
        self.active = false;
        self.event = None;
    }

    /// Cancels the pending event, if any.
    pub fn cancel(&mut self, cx: &mut Ctx) {
        if let Some(event) = self.event {
            cx.cancel(event);
        }
        self.clear();
    }

    pub fn pause(&mut self, cx: &mut Ctx) {
        if let Some(event) = self.event.take() {
            cx.cancel(event);
            self.remaining = (self.due - cx.now()).max(0.0);
        }
    }

    pub fn resume(&mut self, cx: &mut Ctx) {
        if self.active && self.event.is_none() {
            self.due = cx.now() + self.remaining;
            self.event = Some(cx.schedule(self.remaining, self.tag, self.item));
        }
    }
}
