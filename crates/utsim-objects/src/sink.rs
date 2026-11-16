use utsim_core::{Behavior, Ctx, ITEM_TYPE_LABEL, Item, ItemId};

/// Destroys every item it receives and records its time in the system.
#[derive(Default)]
pub struct Sink;

impl Behavior for Sink {
    fn kind(&self) -> &'static str {
        "sink"
    }

    fn accepts_is_item_independent(&self) -> bool {
        true
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

/// A [`Sink`] that also records the time in the system per item type: the tally
/// `time_in_system.<type name>` (its count is the throughput of the type). Items without
/// a known type only count in `time_in_system`.
pub struct TypedSink {
    names: Vec<String>,
    /// `time_in_system.<name>`, built once.
    keys: Vec<String>,
}

impl TypedSink {
    /// `names`: the model's item types, in order (type `k` is `names[k - 1]`).
    pub fn new(names: Vec<String>) -> Self {
        let keys = names
            .iter()
            .map(|n| format!("time_in_system.{n}"))
            .collect();
        TypedSink { names, keys }
    }
}

impl Behavior for TypedSink {
    fn kind(&self) -> &'static str {
        "sink"
    }

    fn accepts_is_item_independent(&self) -> bool {
        true
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        true
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        let it = cx.item(item);
        let time_in_system = cx.now() - it.created_at;
        let k = it
            .label(ITEM_TYPE_LABEL)
            .and_then(|v| v.as_num())
            .filter(|v| *v >= 1.0 && *v <= self.names.len() as f64)
            .map(|v| v as usize - 1);
        cx.record("time_in_system", time_in_system);
        if let Some(k) = k {
            let key = self.keys[k].clone();
            cx.record(&key, time_in_system);
        }
        cx.destroy(item);
    }
}
