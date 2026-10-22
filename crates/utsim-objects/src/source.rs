use utsim_core::{Behavior, Ctx, Dist, Item, ItemId, State};

const ARRIVAL: u32 = 0;

/// Creates items. The next inter-arrival time starts when the previous item
/// leaves, so a blocked source stops generating (put a queue after it to get
/// true arrival-to-arrival timing).
pub struct Source {
    interarrival: Dist,
    first_at_zero: bool,
    max_arrivals: Option<u64>,
    labels: Vec<(String, Dist)>,
    created: u64,
}

impl Source {
    pub fn new(interarrival: Dist) -> Self {
        Source { interarrival, first_at_zero: false, max_arrivals: None, labels: Vec::new(), created: 0 }
    }

    /// Creates the first item at time 0 instead of after one inter-arrival time.
    pub fn first_at_zero(mut self, yes: bool) -> Self {
        self.first_at_zero = yes;
        self
    }

    pub fn max_arrivals(mut self, max: Option<u64>) -> Self {
        self.max_arrivals = max;
        self
    }

    /// Assigns a label to every created item, sampled from `dist`.
    pub fn label(mut self, name: impl Into<String>, dist: Dist) -> Self {
        self.labels.push((name.into(), dist));
        self
    }

    fn schedule_next(&mut self, cx: &mut Ctx) {
        if self.max_arrivals.is_some_and(|max| self.created >= max) {
            cx.set_state(State::Idle);
            return;
        }
        let dt = cx.sample_duration(&self.interarrival);
        cx.schedule(dt, ARRIVAL, None);
        cx.set_state(State::Generating);
    }
}

impl Behavior for Source {
    fn kind(&self) -> &'static str {
        "source"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.created = 0;
        if self.first_at_zero && self.max_arrivals != Some(0) {
            cx.schedule(0.0, ARRIVAL, None);
            cx.set_state(State::Generating);
        } else {
            self.schedule_next(cx);
        }
    }

    fn accepts(&self, _item: &Item, _content: usize) -> bool {
        false
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _item: Option<ItemId>) {
        if tag != ARRIVAL {
            return;
        }
        let id = cx.spawn_item();
        for (name, dist) in &self.labels {
            let value = cx.sample(dist);
            cx.item_mut(id).set_label(name.clone(), value);
        }
        self.created += 1;
        cx.set_state(State::Blocked);
        cx.release(id);
    }

    fn on_exit(&mut self, cx: &mut Ctx, _item: ItemId) {
        self.schedule_next(cx);
    }
}
