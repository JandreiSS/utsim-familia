//! Domain of UTSim Seis Sigma, the Lean Six Sigma project manager of the UTSim family
//! (`docs/plano_lean_seis_sigma.md`). Pure: no file access, no clock, no randomness (the
//! caller supplies instants and random bytes), so everything here is tested in isolation
//! and gives the same result on every machine.
//!
//! - [`id`]: UUID v7 identifiers;
//! - [`op`]: operations `{id, autor, instante, relogio_lamport, entidade, campo, valor,
//!   base}`, entity kinds and the total order `(relogio_lamport, autor, id)`;
//! - [`state`]: deterministic reconstruction (last writer wins per field), optimistic
//!   locking with the list of conflicts to review;
//! - [`journal`]: operations arriving in any order, snapshots;
//! - [`format`]: the versioned project format (`formato: N`) and its migrations;
//! - [`domain`]: typed views (project, phase, action, risk, meeting, saving, person, role);
//! - [`kanban`]: the action board; [`overview`]: the project overview;
//! - [`presence`]: who has the project open and what they are editing;
//! - [`date`]: calendar dates;
//! - [`tools`]: the qualitative tools of the phases (VOC/CTQ, SIPOC, Ishikawa, five whys,
//!   C&E matrix, Pareto, FMEA with the action priority table, plans, A3).

pub mod date;
pub mod domain;
pub mod format;
pub mod id;
pub mod journal;
pub mod kanban;
pub mod op;
pub mod overview;
pub mod presence;
pub mod state;
pub mod tools;

pub use date::Date;
pub use id::Id;
pub use journal::{Insert, Journal, Snapshot};
pub use op::{Edit, EntityRef, Kind, Op, OrderKey};
pub use state::{Conflict, Entity, State};
