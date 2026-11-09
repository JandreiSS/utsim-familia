//! Quality statistics shared by the simulator, the digital twin panel and the future
//! Lean Six Sigma app (`docs/plano_lean_seis_sigma.md`, section 3.6; items E4 and E6 of
//! `docs/plano_gemeo_digital_e_painel.md`).
//!
//! Pure functions over numbers (no model), built on `utsim-stats`; the only file and
//! network access is [`gemeo::read`], which reads the digital twin:
//! - [`constants`]: `d2`, `d3` (by integration), `c4` and the chart factors;
//! - [`charts`]: I-MR, X̄-R, X̄-S, p, np, c, u; phase I (with excluded subgroups) and
//!   phase II (frozen limits); limits from the model's replications;
//! - [`rules`]: Western Electric and Nelson rules, one by one;
//! - [`memory`]: EWMA and tabular CUSUM, residuals real − simulated;
//! - [`arl`]: average run length by simulation and closed forms;
//! - [`capability`]: Cp, Cpk, Pp, Ppk, Cpm with intervals, PPM, sigma level, Box-Cox and
//!   the percentile method;
//! - [`normality`]: Anderson-Darling for the normal, Box-Cox;
//! - [`msa`]: Gage R&R by ANOVA and by averages and ranges, bias, linearity;
//! - [`attribute`]: agreement, Cohen's and Fleiss' kappa;
//! - [`data`]: CSV layouts of the `utsim qualidade` command;
//! - [`gemeo`]: recordings and panel of the digital twin as series for phase II;
//! - [`report`]: text and self-contained HTML reports.

// `!(x > 0.0)` is deliberate throughout: NaN must fail the checks too.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

pub mod arl;
pub mod attribute;
pub mod capability;
pub mod charts;
pub mod constants;
pub mod data;
pub mod gemeo;
pub mod memory;
pub mod msa;
pub mod normality;
pub mod prob;
pub mod report;
pub mod rules;

pub use capability::{Capability, Spec};
pub use charts::{Chart, ChartData, ChartKind, Limits};
pub use constants::Constants;
pub use rules::{Rule, RuleSet};
