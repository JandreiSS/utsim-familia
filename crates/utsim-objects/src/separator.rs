use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Ctx, Dist, Item, ItemId, State, Value};

use crate::timer::PausableTimer;

const PROCESS_DONE: u32 = 1;

/// How the separator splits what it received.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeparateMode {
    /// Takes out whatever is packed in the item. The container is released
    /// first, then its contents in packing order.
    #[default]
    Unpack,
    /// Releases the item plus `n - 1` copies (same labels).
    Split(usize),
    /// An order into its lines: the item plus copies, one per unit of the numeric label
    /// `count` (rounded, at least 1, at most [`MAX_SPLIT`]). Every line gets the label
    /// `group` = the number of the order at this separator (1, 2, ...) and `linha` = its
    /// index (1..n), so a queue with `gather` puts the order together again.
    SplitBy { count: String, group: String },
}

/// Most lines one item is split into by [`SeparateMode::SplitBy`].
pub const MAX_SPLIT: usize = 10_000;

/// Label with the index of a line after [`SeparateMode::SplitBy`].
pub const LINE_LABEL: &str = "linha";

/// Handles one item at a time: waits `process`, then separates it.
///
/// Everything is released in order, so with the default `FirstAvailable` rule
/// all items go to the first open output. Use `ByLabel` (or `RoundRobin`) on
/// the separator to send containers and contents to different places.
pub struct Separator {
    mode: SeparateMode,
    process: Dist,
    item: Option<ItemId>,
    processing: bool,
    /// Released items that have not left yet.
    outstanding: usize,
    timer: PausableTimer,
    /// Orders split so far (the `group` number of the last one).
    orders: u64,
}

impl Separator {
    pub fn new(mode: SeparateMode, process: Dist) -> Self {
        Separator {
            mode,
            process,
            item: None,
            processing: false,
            outstanding: 0,
            timer: PausableTimer::default(),
            orders: 0,
        }
    }

    /// The lines of `item` for [`SeparateMode::SplitBy`].
    fn split_by(&mut self, cx: &mut Ctx, item: ItemId, count: &str, group: &str) -> Vec<ItemId> {
        let n = cx
            .item(item)
            .label(count)
            .and_then(|v| v.as_num())
            .filter(|v| v.is_finite())
            .map_or(1, |v| (v.round().max(1.0) as usize).min(MAX_SPLIT));
        self.orders += 1;
        let order = self.orders as f64;
        let mut lines = vec![item];
        lines.extend((1..n).map(|_| cx.clone_item(item)));
        for (k, id) in lines.iter().enumerate() {
            let it = cx.item_mut(*id);
            it.set_label(group, Value::Num(order));
            it.set_label(LINE_LABEL, Value::Num((k + 1) as f64));
        }
        lines
    }

    fn update_state(&self, cx: &mut Ctx) {
        cx.set_state(match (self.item, self.processing) {
            (None, _) => State::Idle,
            (Some(_), true) => State::Processing,
            (Some(_), false) => State::Blocked,
        });
    }
}

impl Behavior for Separator {
    fn kind(&self) -> &'static str {
        "separator"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.item = None;
        self.processing = false;
        self.outstanding = 0;
        self.timer = PausableTimer::default();
        self.orders = 0;
        cx.set_state(State::Idle);
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        self.item.is_none()
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        self.item = Some(item);
        self.processing = true;
        let t = cx.sample_duration(&self.process);
        self.timer.start(cx, t, PROCESS_DONE, None);
        self.update_state(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _item: Option<ItemId>) {
        self.timer.clear();
        if tag != PROCESS_DONE {
            return;
        }
        let Some(item) = self.item else { return };
        self.processing = false;
        let mut out = vec![item];
        match self.mode.clone() {
            SeparateMode::Unpack => out.extend(cx.unpack(item)),
            SeparateMode::Split(n) => out.extend((1..n).map(|_| cx.clone_item(item))),
            SeparateMode::SplitBy { count, group } => {
                out = self.split_by(cx, item, &count, &group);
            }
        }
        self.outstanding = out.len();
        for id in out {
            cx.release(id);
        }
        self.update_state(cx);
    }

    fn on_exit(&mut self, cx: &mut Ctx, _item: ItemId) {
        self.outstanding = self.outstanding.saturating_sub(1);
        if self.outstanding == 0 {
            self.item = None;
        }
        self.update_state(cx);
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        self.timer.pause(cx);
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        self.timer.resume(cx);
    }
}
