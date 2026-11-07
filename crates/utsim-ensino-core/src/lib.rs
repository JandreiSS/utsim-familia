//! Core of UTSim Ensino, the teaching app of the UTSim family
//! (`docs/plano_ensino_e_jogo.md`). Pure and tested without a window: the simulation is
//! the judge, every grade comes from metrics of the engine with replications and
//! confidence intervals.
//!
//! - [`missao`]: the mission format (`missao.ron`) and its levers;
//! - [`pacote`]: the package (`pacote.ron`), loading and validation of every mission;
//! - [`validar`]: a mission checked against its model;
//! - [`variante`]: the per-student variant drawn from `hash(missao, aluno)`;
//! - [`motor`]: the runs with common random numbers and the replay for the 2D view;
//! - [`nota`]: gate, performance, prediction, justification, tolerance and stars;
//! - [`pecas`]: line balancing and grid layout (the pieces of acts 4 and 5);
//! - [`dicas`]: automatic hints about the attempt;
//! - [`progresso`]: grades per mission and unlocking;
//! - [`turma`]: the class, the student list, the matrix and the common mistakes;
//! - [`conferir`]: the exhaustive reference search, the mission checker and the golden;
//! - [`campanha`]: the built-in campaign.

pub mod campanha;
pub mod conferir;
pub mod construtor;
pub mod dados;
pub mod dicas;
pub mod missao;
pub mod modelagem;
pub mod motor;
pub mod nota;
pub mod pacote;
pub mod pecas;
pub mod progresso;
pub mod turma;
pub mod validar;
pub mod variante;

/// Version of the core that goes in every delivery (the grade is recomputed with it).
pub const VERSAO_NUCLEO: &str = env!("CARGO_PKG_VERSION");
