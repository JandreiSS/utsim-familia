//! Ed25519 keys of the student (signs each attempt) and of the teacher (named in the
//! class invitation), from the shared crate `utsim-diario` (decision E12). Created on first
//! use; the private key never leaves the profile.

pub use utsim_diario::{Chave, verificar};

/// Header of the key files of UTSim Ensino.
pub const CABECALHO: &str = "utsim-ensino-chave-ed25519 v1";
