//! Starting a run from a given state instead of an empty system ("simular a partir do
//! estado atual").
//!
//! A [`StartState`] says, for some blocks, what they hold at `t = 0` and how far their
//! activities have got, plus the calendar instant that corresponds to `t = 0` (so shift
//! tables and arrival schedules start at the right point of their cycle). The kernel
//! places the items in each block, lets the block rebuild its own timers through
//! [`crate::Behavior::restore`], and handles breakdowns and shifts itself. Times are
//! *elapsed* durations (how long it has already lasted), never absolute instants: what
//! is left is drawn from the activity's distribution conditioned on the time already
//! elapsed ([`crate::Dist::sample_residual`]), from the block's own random stream.
//!
//! A block type that does not override `restore` refuses any state of its own, so a new
//! block never accepts a state it does not understand. The same structure is what
//! [`crate::Simulation::save_state`] writes, so runs can be chained.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Time;
use crate::block::BlockId;
use crate::item::Value;

/// Initial state of a whole run, with blocks already resolved to ids.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StartState {
    /// Calendar instant at `t = 0`: shift tables and arrival schedules start at
    /// `clock` modulo their cycle.
    pub clock: Time,
    pub blocks: Vec<(BlockId, BlockState)>,
}

/// State of one block. Every field is optional; what a block type accepts is listed in
/// `docs/manual/src/estado-inicial.md`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockState {
    /// That many items with no history (elapsed 0, no labels). Use either this or `items`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    /// Items in the block, in content order (the front of a queue or belt first).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ItemState>,
    /// Broken down for this long (the repair draws its residual from `mttr`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down: Option<Time>,
    /// Running since the last repair for this long (the next failure draws its residual
    /// from `mtbf`). Without it the time to the first failure is an ordinary draw.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up: Option<Time>,
    /// A check only: whether the block is off shift at `t = 0`. The shift table and the
    /// clock decide; a mismatch is an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub off_shift: Option<bool>,
    /// Position of a transporter (idle there at `t = 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<[f64; 3]>,
    /// Source: time since the arrival clock started (the next arrival draws its residual).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<Time>,
    /// Fluid network: level of each tank, by node name (applied by the model, which sets
    /// the tanks' initial levels).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub levels: BTreeMap<String, f64>,
}

/// Phase of an item inside a processor (or of a stored item in a rack).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkPhase {
    #[serde(alias = "Preparacao")]
    Setup,
    #[serde(alias = "Processando")]
    Processing,
    /// Finished, waiting to leave (blocked or waiting for a transporter).
    #[serde(alias = "Pronto")]
    Done,
}

/// One item of a [`BlockState`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemState {
    /// Time already spent in the current activity: the phase of a processor, the dwell
    /// of a rack, the wait in a queue, the ride on a belt.
    #[serde(skip_serializing_if = "is_zero")]
    pub elapsed: Time,
    /// Time in this block so far, for its stay time (default: `elapsed`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stay: Option<Time>,
    /// Time since the item was created, for the time in system (default: `stay`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<Time>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, Value>,
    /// Processor phase (default: setup when the processor has one, else processing);
    /// in a rack, `Done` means the dwell is over.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<WorkPhase>,
    /// Conveyor: distance from the start of the belt (default: queued at the end).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    /// Rack: storage position `level * bays + bay` (default: the placement rule).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<usize>,
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

impl ItemState {
    /// Time in the block, for the stay time.
    pub fn stay_time(&self) -> Time {
        self.stay.unwrap_or(self.elapsed)
    }

    /// Time since creation, for the time in system.
    pub fn age_time(&self) -> Time {
        self.age.unwrap_or_else(|| self.stay_time())
    }

    /// Drops `stay` and `age` when they repeat what the defaults give (tidier files).
    pub fn tidy(&mut self) {
        if self.stay == Some(self.elapsed) {
            self.stay = None;
        }
        if self.age == Some(self.stay_time()) {
            self.age = None;
        }
    }
}

/// What a block type accepts in its own part of a [`BlockState`] (the kernel itself
/// handles `down`, `up` and `off_shift` for every block).
#[derive(Clone, Copy, Debug, Default)]
pub struct Accepts {
    pub items: bool,
    pub phase: bool,
    pub at: bool,
    pub slot: bool,
    pub position: bool,
    pub elapsed: bool,
}

impl BlockState {
    /// Nothing in it: the block starts as usual.
    pub fn is_empty(&self) -> bool {
        *self == BlockState::default()
    }

    /// Whether it has anything only the block itself can apply.
    pub fn has_block_part(&self) -> bool {
        !self.items.is_empty()
            || self.count.is_some()
            || self.position.is_some()
            || self.elapsed.is_some()
            || !self.levels.is_empty()
    }

    /// Checks that only accepted fields are used. `kind` is the user-facing type name.
    pub fn check(&self, kind: &str, accepts: Accepts) -> Result<(), String> {
        let refuse = |what: &str| {
            Err(format!(
                "objetos do tipo '{kind}' não aceitam '{what}' no estado inicial"
            ))
        };
        if !accepts.items && (!self.items.is_empty() || self.count.is_some_and(|n| n > 0)) {
            return refuse("items/count");
        }
        if !accepts.position && self.position.is_some() {
            return refuse("position");
        }
        if !accepts.elapsed && self.elapsed.is_some() {
            return refuse("elapsed");
        }
        if !self.levels.is_empty() {
            return refuse("levels");
        }
        for (k, it) in self.items.iter().enumerate() {
            let n = k + 1;
            if !accepts.phase && it.phase.is_some() {
                return Err(format!(
                    "item {n}: objetos do tipo '{kind}' não aceitam 'phase'"
                ));
            }
            if !accepts.at && it.at.is_some() {
                return Err(format!(
                    "item {n}: objetos do tipo '{kind}' não aceitam 'at'"
                ));
            }
            if !accepts.slot && it.slot.is_some() {
                return Err(format!(
                    "item {n}: objetos do tipo '{kind}' não aceitam 'slot'"
                ));
            }
        }
        Ok(())
    }
}

/// Every time in a state must be a finite number >= 0; the message names the field.
pub(crate) fn check_times(state: &BlockState) -> Result<(), String> {
    let bad = |x: f64| !(x.is_finite() && x >= 0.0);
    for (name, v) in [
        ("down", state.down),
        ("up", state.up),
        ("elapsed", state.elapsed),
    ] {
        if let Some(x) = v {
            if bad(x) {
                return Err(format!(
                    "'{name}' deve ser um número finito >= 0 (está {x})"
                ));
            }
        }
    }
    if state.count.is_some() && !state.items.is_empty() {
        return Err("use 'count' ou 'items', não os dois".into());
    }
    if state.down.is_some() && state.up.is_some() {
        return Err("use 'down' (quebrado) ou 'up' (funcionando), não os dois".into());
    }
    for (k, it) in state.items.iter().enumerate() {
        let n = k + 1;
        for (name, v) in [
            ("elapsed", Some(it.elapsed)),
            ("stay", it.stay),
            ("age", it.age),
        ] {
            if let Some(x) = v {
                if bad(x) {
                    return Err(format!(
                        "item {n}: '{name}' deve ser um número finito >= 0 (está {x})"
                    ));
                }
            }
        }
        if let Some(at) = it.at {
            if !at.is_finite() {
                return Err(format!("item {n}: 'at' deve ser finito"));
            }
        }
    }
    Ok(())
}
