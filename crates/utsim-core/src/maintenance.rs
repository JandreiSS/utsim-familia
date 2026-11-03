//! Machine maintenance on top of breakdowns: a repair crew that has to come to the
//! machine, wear (failure rate growing with age, Weibull), preventive maintenance by
//! use, cycles, clock or calendar, costs and OEE.
//!
//! Everything here is optional. A block with only [`crate::Downtime`] keeps the plain
//! breakdown kernel (failures on the clock, repair without resources) and gives exactly
//! the same results as before. A block with a [`Maintenance`] (set with
//! [`crate::Simulation::set_maintenance`]) runs the failures through this module:
//!
//! - **Age.** Each block has a virtual age measured in its [`AgeBasis`]: clock time while
//!   it is not broken nor in maintenance, working time (the states in [`is_working`]) or
//!   items that left it (cycles).
//! - **Failure.** With [`Wear`] `Weibull(shape β, scale η)`, the age at the next failure is
//!   drawn conditioned on the current age `a`: `A = η·((a/η)^β − ln(1 − U))^(1/β)`, with
//!   `U` from the block's breakdown stream. With `β = 1` this is `a − η·ln(1 − U)`, the
//!   exponential with mean `η`, drawn exactly as `Dist::Exp` draws it. Without wear the
//!   next failure comes `mtbf` (a draw) after the end of the last repair or preventive.
//! - **Corrective.** With a crew, the block asks it for an `Operate` task (priority
//!   [`CORRECTIVE_PRIORITY`]) and shows `waiting_repair` until the technician arrives; the
//!   repair (`mttr`) starts on arrival. Then the age becomes `q·a` ([`RepairEffect`]).
//! - **Preventive.** When a [`Preventive`] is due the block closes its inputs, finishes
//!   the work it holds (like a shift ending with `Finish`), stops in `maintenance` (or
//!   `waiting_repair` while its technician comes, priority [`PREVENTIVE_PRIORITY`]), and
//!   comes back as good as new: age 0.
//!
//! The metrics go to the block's `custom` metrics (see [`METRICS`]).

use serde::{Deserialize, Serialize};

use crate::block::{BlockId, ShiftTable, State};
use crate::dist::Dist;

/// Priority of a corrective repair request (above every operator request, which uses 0).
pub const CORRECTIVE_PRIORITY: i32 = 20;
/// Priority of a preventive request: after corrective repairs, before operator requests.
pub const PREVENTIVE_PRIORITY: i32 = 10;

/// What the age of a machine counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgeBasis {
    /// Clock time while the machine is not broken nor in maintenance (also off shift).
    #[default]
    #[serde(alias = "Relogio")]
    Clock,
    /// Working time: the states of [`is_working`].
    #[serde(alias = "Uso")]
    Busy,
    /// Items that left the machine.
    #[serde(alias = "Ciclos")]
    Cycles,
}

impl AgeBasis {
    pub fn is_clock(&self) -> bool {
        *self == AgeBasis::Clock
    }

    pub fn label(self) -> &'static str {
        match self {
            AgeBasis::Clock => "tempo no relógio",
            AgeBasis::Busy => "tempo de uso",
            AgeBasis::Cycles => "ciclos",
        }
    }
}

/// Failure law that depends on the age of the machine.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Wear {
    /// Weibull with shape `β` and scale `η` (in the units of the age basis): failure
    /// rate `h(a) = β/η·(a/η)^(β−1)`, growing with age when `β > 1`.
    Weibull { shape: f64, scale: f64 },
}

impl Wear {
    pub fn validate(&self) -> Result<(), String> {
        let Wear::Weibull { shape, scale } = *self;
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if ok(shape) && ok(scale) {
            Ok(())
        } else {
            Err("wear: Weibull exige shape > 0 e scale > 0".into())
        }
    }

    /// Age at the next failure for a machine of age `age`, given `u` uniform in `[0, 1)`.
    pub fn next_failure_age(&self, age: f64, u: f64) -> f64 {
        let Wear::Weibull { shape, scale } = *self;
        scale * ((age / scale).powf(shape) - (1.0 - u).ln()).powf(1.0 / shape)
    }

    /// Cumulative hazard `H(a) = (a/η)^β`: expected failures from age 0 to `a` under
    /// minimal repair.
    pub fn cumulative_hazard(&self, age: f64) -> f64 {
        let Wear::Weibull { shape, scale } = *self;
        (age / scale).powf(shape)
    }
}

/// How much of the age a corrective repair takes away.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum RepairEffect {
    /// Minimal repair: the machine comes back as old as it was ("tão ruim quanto velha").
    #[default]
    #[serde(alias = "TaoRuimQuantoVelha")]
    AsBadAsOld,
    /// Perfect repair: age 0 ("tão boa quanto nova").
    #[serde(alias = "TaoBoaQuantoNova")]
    AsGoodAsNew,
    /// The age is multiplied by `q` in `[0, 1]` (virtual age, Kijima type II).
    #[serde(alias = "Parcial")]
    Partial(f64),
}

impl RepairEffect {
    pub fn is_default(&self) -> bool {
        *self == RepairEffect::AsBadAsOld
    }

    /// Fraction of the age kept.
    pub fn kept(self) -> f64 {
        match self {
            RepairEffect::AsBadAsOld => 1.0,
            RepairEffect::AsGoodAsNew => 0.0,
            RepairEffect::Partial(q) => q,
        }
    }
}

/// Optional costs, in the model's money unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaintenanceCost {
    /// Per time unit the machine is stopped by maintenance (waiting for the technician,
    /// repairing or in preventive).
    pub per_time_down: f64,
    /// Per corrective repair.
    pub per_repair: f64,
    /// Per preventive maintenance.
    pub per_preventive: f64,
}

impl MaintenanceCost {
    pub fn validate(&self) -> Result<(), String> {
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        if ok(self.per_time_down) && ok(self.per_repair) && ok(self.per_preventive) {
            Ok(())
        } else {
            Err("cost: os custos devem ser finitos e >= 0".into())
        }
    }
}

/// When a preventive maintenance is due (kernel form, calendars already resolved).
#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    /// After this much working time since the last one.
    Hours(f64),
    /// After this many items left the machine since the last one.
    Cycles(u64),
    /// This much clock time after the end of the last one.
    Clock(f64),
    /// At the start of every off window of this table (clock and calendar start as in
    /// shifts).
    Calendar(ShiftTable),
}

/// One preventive maintenance plan of a machine.
#[derive(Clone, Debug, PartialEq)]
pub struct Preventive {
    pub trigger: Trigger,
    pub duration: Dist,
    /// Team (transporter or dispatcher) that does it; `None` = the machine's crew, if any.
    pub crew: Option<BlockId>,
}

/// Maintenance options of one block (see the module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Maintenance {
    /// Team that repairs breakdowns (a transporter or a dispatcher).
    pub crew: Option<BlockId>,
    pub wear: Option<Wear>,
    pub basis: AgeBasis,
    pub repair: RepairEffect,
    pub preventive: Vec<Preventive>,
    pub cost: MaintenanceCost,
    /// Ideal time per item for the OEE performance (already divided by the number of
    /// servers); `None` = performance not measured (taken as 1).
    pub ideal_cycle: Option<f64>,
    /// Items sent from this block to these blocks count as rejects (OEE quality).
    pub reject_to: Vec<BlockId>,
    /// Items leaving with this label different from 0 / false / "" count as rejects.
    pub reject_label: Option<String>,
}

/// States in which a machine is working (age with [`AgeBasis::Busy`], preventive by
/// `Hours`).
pub fn is_working(state: State) -> bool {
    matches!(
        state,
        State::Processing
            | State::Setup
            | State::Utilize
            | State::TravelEmpty
            | State::TravelLoaded
            | State::Loading
            | State::Unloading
    )
}

/// Names of the metrics a block with maintenance adds to its `custom` metrics.
pub const METRICS: &[&str] = &[
    "oee",
    "oee.availability",
    "oee.performance",
    "oee.quality",
    "maintenance.failures",
    "maintenance.repairs",
    "maintenance.preventives",
    "maintenance.mtbf",
    "maintenance.mttr",
    "maintenance.repair_time",
    "maintenance.wait_technician",
    "maintenance.wait_technician_total",
    "maintenance.down_time",
    "maintenance.preventive_time",
    "maintenance.cost",
    "maintenance.age",
    "maintenance.rejects",
];
