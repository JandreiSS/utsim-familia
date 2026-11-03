//! Specialised lines of UTSim (`docs/plano_linhas_especializadas.md`).
//!
//! A line is a domain pack over the same kernel and model format: a library of presets
//! (ordinary objects with domain names and defaults), a vocabulary, ready models, and
//! indicators with their report. [`pacote`] reads and checks a pack and places its
//! presets in a model being edited; [`registro`] holds the lines that come with UTSim.

pub mod pacote;
pub mod registro;

pub use pacote::{Field, Library, ModelRef, Pack, Placement, Preset, Role, Vocabulary};
pub use registro::{LEAN_OFFICE, builtin, find};
