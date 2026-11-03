//! PERT/CPM of a part of a UTSim model.
//!
//! The person selects a set of objects (or "from A to B") and asks for the PERT/CPM of it:
//!
//! 1. [`network::derive`] turns the selection into an activity network (activity on node),
//!    listing the assumptions it had to make (AND/OR outputs, cut cycles, shared
//!    resources);
//! 2. three ways of computing, all on the same network:
//!    - [`observed::run`]: the critical path observed in the simulation itself, waits in
//!      queues, transporters and resources included (the default of the command line);
//!    - [`pert::analyze`]: classic PERT, analytic, with CPM dates and floats
//!      ([`cpm::schedule`]) and the probability of a deadline;
//!    - [`monte_carlo::run`]: draws of the durations over the network, criticality
//!      index, distribution of the total time and sensitivity;
//! 3. [`report`]: text, CSV and HTML, and the coordinates of the network diagram for the
//!    interface to draw.
//!
//! The crate is pure (no engine, no window) and deterministic: the same seed gives the
//! same result on any number of threads.

pub mod cpm;
pub mod math;
pub mod monte_carlo;
pub mod network;
pub mod observed;
pub mod pert;
pub mod report;

pub use cpm::{Schedule, schedule, schedule_means};
pub use monte_carlo::{McOptions, McResult};
pub use network::{
    Activity, ActivityKind, ActivityNetwork, DeriveOptions, Selection, WeightedBranch, derive,
    derive_scenarios,
};
pub use observed::{ObservedOptions, ObservedResult};
pub use pert::{Estimate, Moments, PertResult};

/// Why a network could not be derived or analysed.
#[derive(Debug, thiserror::Error)]
pub enum PertError {
    #[error("o objeto '{0}' não existe no modelo")]
    UnknownObject(String),
    #[error("a seleção está vazia (nenhum objeto que não seja recurso móvel)")]
    EmptySelection,
    #[error("não há caminho de '{from}' até '{to}' pelas ligações do modelo")]
    NoPath { from: String, to: String },
    #[error("a rede tem um ciclo: o CPM só aceita redes sem ciclos")]
    Cycle,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Model(#[from] utsim_model::ModelError),
}

/// Calculation mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Mode {
    /// Critical path observed in the simulation (default).
    #[default]
    Simulated,
    /// Classic analytic PERT.
    Classic,
    /// Monte Carlo over the network.
    MonteCarlo,
}

impl Mode {
    /// Parses the command-line names (`simulado`, `classico`, `montecarlo`).
    pub fn parse(s: &str) -> Option<Mode> {
        match s.to_lowercase().as_str() {
            "simulado" | "simulada" | "simulated" => Some(Mode::Simulated),
            "classico" | "clássico" | "classic" => Some(Mode::Classic),
            "montecarlo" | "monte-carlo" | "monte_carlo" => Some(Mode::MonteCarlo),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Simulated => "caminho crítico observado na simulação",
            Mode::Classic => "PERT clássico",
            Mode::MonteCarlo => "Monte Carlo sobre a rede",
        }
    }
}
