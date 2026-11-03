//! UTSim discrete-event simulation kernel.
//!
//! The kernel owns the clock, the future event list, the flow items and the
//! port graph. Blocks (sources, queues, processors...) implement [`Behavior`]
//! and never touch each other directly: they only talk to the kernel through
//! [`Ctx`]. Item movement between blocks is resolved by the kernel using the
//! "both sides open" rule described in `docs/arquitetura.md`.

pub mod block;
pub mod dist;
pub mod engine;
pub mod init;
pub mod item;
pub mod item_log;
pub mod maintenance;
pub mod person;
pub mod report;
mod residual;
pub mod rng;
pub mod script;
mod special;
pub mod stats;
pub mod task;
pub mod trace;

/// Simulation time. Units are whatever the model author chooses (usually seconds).
pub type Time = f64;

pub use block::{
    Behavior, BlockId, Downtime, OffWindow, OutputRule, ShiftEnd, ShiftTable, State, TransferDef,
    TransferPath,
};
pub use dist::Dist;
pub use engine::{Ctx, Decision, EventId, RunConfig, SimError, Simulation, Step};
pub use init::{BlockState, ItemState, StartState, WorkPhase};
pub use item::{Item, ItemId, Value};
pub use item_log::ItemRecord;
pub use maintenance::{
    AgeBasis, Maintenance, MaintenanceCost, Preventive, RepairEffect, Trigger, Wear,
};
pub use person::{Fatigue, Learning, Lift, LoadedWalk, Person, Variation};
pub use report::{BlockReport, RunReport, SeriesPoint, TallySummary, Utilization, summarize};
pub use stats::Histogram;
pub use task::{RequestId, Router, TaskKind, TaskRequest};
pub use trace::{Observer, TraceEvent, TraceRecord};
