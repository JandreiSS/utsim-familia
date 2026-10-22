use utsim_core::{Behavior, Ctx, Item, ItemId};

/// Destroys every item it receives and records its time in the system.
#[derive(Default)]
pub struct Sink;

impl Behavior for Sink {
    fn kind(&self) -> &'static str {
        "sink"
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        true
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        let time_in_system = cx.now() - cx.item(item).created_at;
        cx.record("time_in_system", time_in_system);
        cx.destroy(item);
    }
}
