//! Specialised lines of UTSim (`docs/plano_linhas_especializadas.md`).
//!
//! A line is a domain pack over the same kernel and model format: a library of presets
//! (ordinary objects with domain names and defaults), a vocabulary, ready models, and
//! indicators with their report. [`pacote`] reads and checks a pack and places its
//! presets in a model being edited; [`registro`] holds the lines that come with UTSim.

pub mod alimentacao;
pub mod lean_office;
pub mod logistica;
pub mod manufatura;
pub mod pacote;
pub mod processos;
pub mod registro;
pub mod saude;
pub mod servicos;

pub use pacote::{Field, Library, ModelRef, Pack, Placement, Preset, Role, Vocabulary};
pub use registro::{
    ALIMENTACAO, LEAN_OFFICE, LOGISTICA, MANUFATURA, PROCESSOS, QUALIDADE, SAUDE, SEGURANCA,
    SERVICOS, active, builtin, find,
};
