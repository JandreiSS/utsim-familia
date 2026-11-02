use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Ctx, Dist, EventId, Item, ItemId, State, Time};

const ARRIVAL: u32 = 0;
const LOST_CHECK: u32 = 1;

/// What the source does with an item that cannot leave at the instant it is created.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnBlocked {
    /// Keep the item and stop generating until it leaves.
    #[default]
    Wait,
    /// Destroy the item (counted as `lost`) and keep the arrival clock running.
    Drop,
}

/// Creates items. With `OnBlocked::Wait` the next inter-arrival time starts when
/// the previous item leaves, so a blocked source stops generating (put a queue
/// after it to get true arrival-to-arrival timing). With `OnBlocked::Drop` an item
/// that cannot leave at once is destroyed and recorded as `lost`, so arrivals form
/// a renewal process. A breakdown pauses the arrival clock.
pub struct Source {
    on_blocked: OnBlocked,
    interarrival: Dist,
    first_at_zero: bool,
    max_arrivals: Option<u64>,
    labels: Vec<(String, Dist)>,
    created: u64,
    /// Items that left the source and items destroyed because they could not leave,
    /// both since the warm-up ended (the loss fraction is `lost / (lost + entered)`).
    entered: u64,
    lost: u64,
    /// Pending arrival and its due time.
    next: Option<(EventId, Time)>,
    /// Time left on the arrival clock while down.
    paused: Option<Time>,
}

impl Source {
    pub fn new(interarrival: Dist) -> Self {
        Source {
            on_blocked: OnBlocked::Wait,
            interarrival,
            first_at_zero: false,
            max_arrivals: None,
            labels: Vec::new(),
            created: 0,
            entered: 0,
            lost: 0,
            next: None,
            paused: None,
        }
    }

    pub fn on_blocked(mut self, mode: OnBlocked) -> Self {
        self.on_blocked = mode;
        self
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

    fn arrive_in(&mut self, cx: &mut Ctx, delay: Time) {
        let event = cx.schedule(delay, ARRIVAL, None);
        self.next = Some((event, cx.now() + delay));
        cx.set_state(State::Generating);
    }

    fn schedule_next(&mut self, cx: &mut Ctx) {
        if self.max_arrivals.is_some_and(|max| self.created >= max) {
            cx.set_state(State::Idle);
            return;
        }
        let dt = cx.sample_duration(&self.interarrival);
        self.arrive_in(cx, dt);
    }
}

impl Behavior for Source {
    fn kind(&self) -> &'static str {
        "source"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.created = 0;
        self.entered = 0;
        self.lost = 0;
        self.next = None;
        self.paused = None;
        if self.first_at_zero && self.max_arrivals != Some(0) {
            self.arrive_in(cx, 0.0);
        } else {
            self.schedule_next(cx);
        }
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        if tag == LOST_CHECK {
            // The kernel has resolved transfers by now: if the item is still here, it is lost.
            if let Some(id) = item.filter(|id| cx.contents().contains(id)) {
                cx.destroy(id);
                cx.record("lost", 1.0);
                self.lost += 1;
                if cx.is_down() {
                    // Stopped in this same instant: park the clock until resume.
                    self.paused = Some(cx.sample_duration(&self.interarrival));
                } else {
                    self.schedule_next(cx);
                }
            }
            return;
        }
        if tag != ARRIVAL {
            return;
        }
        self.next = None;
        let id = cx.spawn_item();
        for (name, dist) in &self.labels {
            let value = cx.sample(dist);
            cx.item_mut(id).set_label(name.clone(), value);
        }
        self.created += 1;
        cx.set_state(State::Blocked);
        cx.release(id);
        if self.on_blocked == OnBlocked::Drop {
            cx.schedule(0.0, LOST_CHECK, Some(id));
        }
    }

    fn on_exit(&mut self, cx: &mut Ctx, _item: ItemId) {
        self.entered += 1;
        self.schedule_next(cx);
    }

    fn on_warmup(&mut self, _cx: &mut Ctx) {
        self.entered = 0;
        self.lost = 0;
    }

    /// For `Drop` sources: `lost.count`, `entered.count` and `loss_fraction`, always
    /// present (zero when nothing was lost) so they work as optimisation objectives.
    fn custom_metrics(&self, _now: Time) -> Vec<(String, f64)> {
        if self.on_blocked != OnBlocked::Drop {
            return Vec::new();
        }
        let total = (self.lost + self.entered) as f64;
        vec![
            ("lost.count".to_string(), self.lost as f64),
            ("entered.count".to_string(), self.entered as f64),
            (
                "loss_fraction".to_string(),
                if total > 0.0 {
                    self.lost as f64 / total
                } else {
                    0.0
                },
            ),
        ]
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        if let Some((event, due)) = self.next.take() {
            cx.cancel(event);
            self.paused = Some((due - cx.now()).max(0.0));
        }
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        if let Some(remaining) = self.paused.take() {
            self.arrive_in(cx, remaining);
        }
    }
}
