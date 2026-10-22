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
pub mod item;
pub mod report;
pub mod rng;
pub mod stats;
pub mod trace;

/// Simulation time. Units are whatever the model author chooses (usually seconds).
pub type Time = f64;

pub use block::{Behavior, BlockId, Downtime, OffWindow, OutputRule, ShiftTable, State};
pub use dist::Dist;
pub use engine::{Ctx, EventId, RunConfig, SimError, Simulation};
pub use item::{Item, ItemId, Value};
pub use report::{BlockReport, RunReport, SeriesPoint, TallySummary, summarize};
pub use stats::Histogram;
pub use trace::{Observer, TraceEvent, TraceRecord};
