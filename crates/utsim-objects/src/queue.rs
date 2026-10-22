use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Ctx, Item, ItemId, State};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Discipline {
    #[default]
    Fifo,
    Lifo,
}

/// Buffer that releases every item as soon as it enters.
pub struct Queue {
    capacity: Option<usize>,
    discipline: Discipline,
}

impl Queue {
    /// `None` means unlimited capacity.
    pub fn new(capacity: Option<usize>) -> Self {
        Queue { capacity, discipline: Discipline::Fifo }
    }

    pub fn discipline(mut self, discipline: Discipline) -> Self {
        self.discipline = discipline;
        self
    }
}

impl Behavior for Queue {
    fn kind(&self) -> &'static str {
        "queue"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        cx.set_state(State::Empty);
    }

    fn accepts(&self, _item: &Item, content: usize) -> bool {
        self.capacity.is_none_or(|cap| content < cap)
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId) {
        match self.discipline {
            Discipline::Fifo => cx.release(item),
            Discipline::Lifo => cx.release_front(item),
        }
        cx.set_state(State::Releasing);
    }

    fn on_exit(&mut self, cx: &mut Ctx, _item: ItemId) {
        if cx.content_len() == 0 {
            cx.set_state(State::Empty);
        }
    }
}
