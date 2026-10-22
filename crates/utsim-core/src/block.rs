//! The contract every block implements, plus the vocabulary shared by all blocks.

use serde::{Deserialize, Serialize};

use crate::dist::Dist;
use crate::engine::Ctx;
use crate::item::{Item, ItemId};
use crate::task::TaskRequest;

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
    OffShift,
    TravelEmpty,
    TravelLoaded,
    Loading,
    Unloading,
    /// A mobile resource working at a station (operator).
    Utilize,
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
            State::OffShift => "off_shift",
            State::TravelEmpty => "travel_empty",
            State::TravelLoaded => "travel_loaded",
            State::Loading => "loading",
            State::Unloading => "unloading",
            State::Utilize => "utilize",
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
    /// A random port with the given relative weights, one per output port.
    Weighted(Vec<f64>),
    /// A user script returns the port (1-based in the script language; the model
    /// compiles it into a [`crate::script::PortScript`]).
    Script(String),
}

/// Repeating schedule of off-shift windows (breaks, nights, weekends).
/// Window times are offsets inside a cycle of length `period`, starting at t=0.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShiftTable {
    pub period: f64,
    pub off: Vec<OffWindow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OffWindow {
    pub start: f64,
    pub duration: f64,
}

impl ShiftTable {
    /// Windows must be positive, inside the period, sorted and non-overlapping.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.period.is_finite() && self.period > 0.0) {
            return Err("shifts.period deve ser finito e > 0".into());
        }
        let mut end_prev = 0.0;
        for (k, w) in self.off.iter().enumerate() {
            let ok = w.start.is_finite() && w.duration.is_finite() && w.duration > 0.0;
            if !ok || w.start < end_prev || w.start + w.duration > self.period {
                return Err(format!(
                    "shifts.off[{k}]: janelas precisam ter duração > 0, estar dentro do período, em ordem e sem sobreposição"
                ));
            }
            end_prev = w.start + w.duration;
        }
        Ok(())
    }

    /// Fraction of each cycle spent off shift.
    pub fn off_fraction(&self) -> f64 {
        self.off.iter().map(|w| w.duration).sum::<f64>() / self.period
    }
}

/// Random breakdowns: time between failures counted on the clock (not on use).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Downtime {
    pub mtbf: Dist,
    pub mttr: Dist,
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

    /// Whether input `port` (0-based, in connection order) is open for `item`,
    /// given the current content.
    fn accepts(&self, item: &Item, content: usize, port: usize) -> bool;

    /// The item has just been placed in this block through input `port`.
    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, port: usize) {
        let _ = (cx, item, port);
    }

    /// The item has just left this block (content already excludes it).
    fn on_exit(&mut self, cx: &mut Ctx, item: ItemId) {
        let _ = (cx, item);
    }

    /// An event this block scheduled with [`Ctx::schedule`] has fired.
    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        let _ = (cx, tag, item);
    }

    /// The block went down. The kernel already closed its ports and shows
    /// `down`; blocks with running timers should pause them (see [`Ctx::cancel`]).
    fn on_stop(&mut self, cx: &mut Ctx) {
        let _ = cx;
    }

    /// The block is back up. The kernel restores the last state set by the block.
    fn on_resume(&mut self, cx: &mut Ctx) {
        let _ = cx;
    }

    /// A task for a mobile resource or dispatcher (see [`crate::task`]). Blocks that
    /// cannot do work report a modelling error.
    fn on_task(&mut self, cx: &mut Ctx, request: TaskRequest) {
        let _ = request;
        cx.error("este objeto não executa tarefas de transporte ou operação");
    }

    /// A message from another block (`MSG_*` tags in [`crate::task`]).
    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        let _ = (cx, from, tag, payload);
    }
}
