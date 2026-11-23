//! Domain of UTSim LSS, the Lean Six Sigma project manager of the UTSim family
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
pub mod merge;
pub mod op;
pub mod overview;
pub mod presence;
pub mod state;
pub mod texto;
pub mod tools;

// Stage L1, project management (`docs/plano_lean_seis_sigma.md`, 1.1 to 1.10).
pub mod board;
pub mod calendar;
pub mod charter;
pub mod closure;
pub mod digest;
pub mod lessons;
pub mod meeting;
pub mod portfolio;
pub mod risk;
pub mod roles;
pub mod savings;
pub mod schedule;
pub mod tollgate;

// Stage L6, implementation program (`docs/plano_lean_seis_sigma.md`, 8.1 to 8.4, 8.7 and
// 8.8; module names of decision MB9).
pub mod designacao;
pub mod mentoria;
pub mod painel;
pub mod perfis;
// Import of the program's people from HR's CSV (P9).
pub mod pessoas;
pub mod privacidade;
pub mod programa;
pub mod revisao;

// Stage L7, training and certification (`docs/plano_lean_seis_sigma.md`, 8.5).
pub mod certificacao;
pub mod formacao;

// Stage L4, integration with the UTSim model (`docs/plano_lean_seis_sigma.md`, 3).
pub mod dados;
pub mod simulacao;

// Stage L5, reports (`docs/plano_lean_seis_sigma.md`, 4.6 and 4.7).
pub mod relatorios;

// Stage L3 (screens) and P1, statistical analyses and MSA (`docs/plano_lean_seis_sigma.md`, 2.2 and 2.3).
pub mod analises;

// The DMAIC roadmap (`docs/plano_lean_seis_sigma.md`, 1.4 and 2).
pub mod roteiro;

// Design of experiments (P3, `docs/plano_lean_seis_sigma.md`, 2.4 and 3.4).
pub mod experimentos;

// Control with the twin and the panel (P5, `docs/plano_lean_seis_sigma.md`, 3.5).
pub mod controle;

// Project templates of the specialised lines (`docs/plano_linhas_especializadas.md`, 1.8).
pub mod modelos_linha;

pub use date::Date;
pub use id::Id;
pub use journal::{Insert, Journal, Snapshot};
pub use op::{Edit, EntityRef, Kind, Op, OrderKey};
pub use state::{Conflict, Entity, State};
