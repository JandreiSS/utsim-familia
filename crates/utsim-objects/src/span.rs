use utsim_core::{Behavior, Ctx, Item, State};

/// Holds the observations of a model-level measurement (time between two points of the
/// flow, see `Simulation::add_meter`). It never holds items: the kernel records each
/// item's time in its tally `value`, which the report shows like any other tally
/// (mean, confidence interval, histogram).
#[derive(Default)]
pub struct Span;

impl Behavior for Span {
    fn kind(&self) -> &'static str {
        "span"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        cx.set_state(State::Idle);
    }

    fn accepts_is_item_independent(&self) -> bool {
        true
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }
}
