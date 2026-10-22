use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Ctx, Dist, Item, ItemId, State};

use crate::timer::PausableTimer;

const PROCESS_DONE: u32 = 1;

/// How the separator splits what it received.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeparateMode {
    /// Takes out whatever is packed in the item. The container is released
    /// first, then its contents in packing order.
    #[default]
    Unpack,
    /// Releases the item plus `n - 1` copies (same labels).
    Split(usize),
}

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
        }
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
        match self.mode {
            SeparateMode::Unpack => out.extend(cx.unpack(item)),
            SeparateMode::Split(n) => out.extend((1..n).map(|_| cx.clone_item(item))),
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
