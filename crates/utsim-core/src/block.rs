//! The contract every block implements, plus the vocabulary shared by all blocks.

use serde::{Deserialize, Serialize};

use crate::dist::Dist;
use crate::engine::Ctx;
use crate::init::BlockState;
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
    /// Stopped from outside the model (emulation, operator command).
    Stopped,
    /// Broken (or due for preventive maintenance) and waiting for the technician
    /// ([`crate::maintenance`]).
    WaitingRepair,
    /// In preventive maintenance ([`crate::maintenance`]).
    Maintenance,
    /// A person pausing to recover from fatigue ([`crate::person`]).
    Resting,
    /// A station whose operators are entering it (door, guard, cell) before the work.
    Entering,
    /// A station whose operators are leaving it after the work.
    Leaving,
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
            State::Stopped => "stopped",
            State::WaitingRepair => "waiting_repair",
            State::Maintenance => "maintenance",
            State::Resting => "resting",
            State::Entering => "entering",
            State::Leaving => "leaving",
        }
    }
}

/// Which output port a released item may leave through. Decided once, when the
/// item is released.
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShiftTable {
    pub period: f64,
    pub off: Vec<OffWindow>,
    /// What happens to work in progress when an off-shift window starts.
    #[serde(
        default,
        alias = "ao_fim_do_turno",
        skip_serializing_if = "ShiftEnd::is_pause"
    )]
    pub on_end: ShiftEnd,
}

/// Behaviour at the start of an off-shift window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShiftEnd {
    /// Stop at once: running timers freeze and resume when the shift comes back.
    #[default]
    #[serde(alias = "Pausar")]
    Pause,
    /// Close the inputs at once, finish the work in progress (overtime) and only then
    /// go off shift; an idle block goes off shift at once. The end of the window does
    /// not move: overtime never delays the next shift.
    #[serde(alias = "Terminar")]
    Finish,
}

impl ShiftEnd {
    pub fn is_pause(&self) -> bool {
        *self == ShiftEnd::Pause
    }
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

/// Travel time for items that leave a block through direct connections (no transporter):
/// `delay + distance / speed`. Without it a direct connection is instantaneous.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransferDef {
    /// Travel speed (length units per time unit), must be > 0.
    pub speed: f64,
    /// Fixed time added to every transfer (time units), >= 0.
    #[serde(default)]
    pub delay: f64,
    /// Which path gives the distance.
    #[serde(default)]
    pub path: TransferPath,
}

/// Path an item follows on a timed direct connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferPath {
    /// Straight line between the two objects' positions.
    #[default]
    Straight,
    /// Shortest route over the model's path network (straight when it has none).
    Network,
}

impl TransferDef {
    /// Checks the numbers; the error is user-facing text.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.speed.is_finite() && self.speed > 0.0) {
            return Err("transfer.speed deve ser finita e > 0".into());
        }
        if !(self.delay.is_finite() && self.delay >= 0.0) {
            return Err("transfer.delay deve ser finito e >= 0".into());
        }
        Ok(())
    }
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

    /// True when [`Behavior::accepts`] never looks at the item: its answer depends only
    /// on the block's own state, the content and the port. The kernel then takes one
    /// refusal as valid for every item waiting to come in until something moves, instead
    /// of asking again per item. Return true only when that is certain.
    fn accepts_is_item_independent(&self) -> bool {
        false
    }

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

    /// Whether the block still has work it must finish before going off shift when its
    /// shift table says [`ShiftEnd::Finish`]. While this is true after the window started,
    /// the inputs stay closed and the block keeps running. The default: an item is held
    /// that was not released yet (and is not waiting for a transporter).
    fn work_in_progress(&self, cx: &Ctx) -> bool {
        cx.unreleased_count() > 0
    }

    /// The warm-up period ended: blocks with their own statistics restart them.
    fn on_warmup(&mut self, cx: &mut Ctx) {
        let _ = cx;
    }

    /// Block-specific metrics for the report (e.g. tank levels), as `(name, value)`;
    /// they appear as `<block>.<name>` in `RunReport::metrics`.
    fn custom_metrics(&self, now: crate::Time) -> Vec<(String, f64)> {
        let _ = now;
        Vec::new()
    }

    /// A message from another block (`MSG_*` tags in [`crate::task`]).
    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        let _ = (cx, from, tag, payload);
    }

    /// Applies this block's part of an initial state (see [`crate::init`]), right after
    /// [`Behavior::reset`]. The kernel has already placed `state.items` in the block, in
    /// the same order as [`Ctx::contents`], and handles `down`, `up` and `off_shift`
    /// itself. The error is user-facing text. The default refuses anything of the
    /// block's own, so a block type never accepts a state it does not understand.
    fn restore(&mut self, cx: &mut Ctx, state: &BlockState) -> Result<(), String> {
        let _ = cx;
        if state.has_block_part() {
            Err(format!(
                "objetos do tipo '{}' ainda não aceitam estado inicial",
                self.kind()
            ))
        } else {
            Ok(())
        }
    }

    /// Writes this block's part of a saved state (see [`crate::Simulation::save_state`]).
    /// The kernel has filled `state.items` with the items held, in content order, with
    /// `elapsed` and `stay` equal to the time in the block, `age` and labels; the block
    /// adds what only it knows (phases, elapsed work, belt positions, slots, position).
    /// Returns a note about anything that cannot be represented and is lost. The
    /// default drops the items with a note.
    fn save_state(&self, cx: &Ctx, state: &mut BlockState) -> Option<String> {
        let _ = cx;
        if state.items.is_empty() {
            return None;
        }
        let n = state.items.len();
        state.items.clear();
        Some(format!(
            "{n} item(ns) num objeto do tipo '{}' não entram no estado gravado",
            self.kind()
        ))
    }
}
