//! Input modelling: which distribution best describes a sample of real data?
//!
//! Pipeline (see `docs/arquitetura.md`, "Ajuste de distribuições"):
//! 1. [`diagnostics`] checks the i.i.d. assumptions and describes the data;
//! 2. every [`Family`] in [`families::all`] is fitted by maximum likelihood;
//! 3. [`gof`] runs goodness-of-fit tests on every fitted candidate;
//! 4. [`rank`] applies the published selection rule and explains the outcome;
//! 5. [`report`] renders the result for people (text/HTML) and for models (a `Dist`).

pub mod diagnostics;
pub mod families;
pub mod family;
pub mod gof;
pub mod rank;
pub mod report;

pub use family::{Family, Fitted, FittedModel};
pub use rank::{CandidateResult, Ranking};
