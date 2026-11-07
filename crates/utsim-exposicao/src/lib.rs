//! Exposure of people on the floor (stage QS5 of `docs/plano_qualidade_e_seguranca.md`).
//! The computation lives in `utsim_model::exposure`, so the model can put the exposure of
//! a model with risk zones into its own report (`RunReport::metrics`). This crate
//! re-exports it and adds the HTML report with the legal notice ([`relatorio`]).

pub mod relatorio;

pub use utsim_model::exposure::*;
