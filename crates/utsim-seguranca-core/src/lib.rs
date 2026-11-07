//! Occupational safety on top of the unit folder (stage QS9 of
//! `docs/plano_qualidade_e_seguranca.md`): risk matrix ([`matriz`]), inventory of hazards
//! and risks with the action plan ([`inventario`]) and the report ([`relatorio`]). Base of
//! the UTSim Segurança do Trabalho app.
//!
//! The UTSim supports study, teaching and the organisation of information. It does not
//! replace the legally qualified professional, issues no technical report and certifies
//! no compliance with any Norma Regulamentadora. Every report carries
//! [`AVISO_LEGAL`].

pub mod checklist;
pub mod ergonomia;
pub mod incidentes;
pub mod inventario;
pub mod matriz;
pub mod relatorio;

pub use inventario::{Acao, Grupo, Hierarquia, Inventario, Perigo, Reavaliacao, Situacao};
pub use matriz::Matriz;
pub use utsim_exposicao::relatorio::AVISO_LEGAL;
