//! The contract every block implements, plus the vocabulary shared by all blocks.

use serde::{Deserialize, Serialize};

use crate::engine::Ctx;
use crate::item::{Item, ItemId};

/// Index of a block inside a [`Simulation`](crate::Simulation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlockId(pub usize);

/// Block states tracked for statistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Idle,
    Empty,
    Generating,
    Releasing,
    Collecting,
    Setup,
    Processing,
    Blocked,
    WaitingOperator,
    WaitingTransport,
    Down,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Empty => "empty",
            State::Generating => "generating",
            State::Releasing => "releasing",
            State::Collecting => "collecting",
            State::Setup => "setup",
            State::Processing => "processing",
            State::Blocked => "blocked",
            State::WaitingOperator => "waiting_operator",
            State::WaitingTransport => "waiting_transport",
            State::Down => "down",
        }
    }
}

/// Which output port a released item may leave through. Decided once, when the
/// item is released (the simuladores comerciais "send to port" moment).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum OutputRule {
    /// Any port; the first open downstream block in port order takes it.
    #[default]
    FirstAvailable,
    /// Ports in turn, regardless of whether the chosen one is open.
    RoundRobin,
    /// A uniformly random port.
    Random,
    /// The port number (1-based) stored in the given numeric label.
    ByLabel(String),
}

/// Logic of a block. The kernel calls these hooks; a block reacts by
/// scheduling events, changing state and releasing items through [`Ctx`].
pub trait Behavior: Send {
    /// Short type name used in reports ("queue", "processor"...).
    fn kind(&self) -> &'static str;

    /// Called once before the run starts. Initial state and events go here.
    fn reset(&mut self, cx: &mut Ctx) {
        let _ = cx;
    }

    /// Whether the input side is open for `item`, given the current content.
    fn accepts(&self, item: &Item, content: usize) -> bool;

    /// The item has just been placed in this block.
    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId) {
        let _ = (cx, item);
    }

    /// The item has just left this block (content already excludes it).
    fn on_exit(&mut self, cx: &mut Ctx, item: ItemId) {
        let _ = (cx, item);
    }

    /// An event this block scheduled with [`Ctx::schedule`] has fired.
    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        let _ = (cx, tag, item);
    }
}
