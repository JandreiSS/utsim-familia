//! Reports of the program: the document model, the PDF generator and the CSV of the
//! tables live in the common crate `utsim-ferramentas` (stage QS7 of
//! `docs/plano_qualidade_e_seguranca.md`), re-exported here at the same paths.

pub use utsim_ferramentas::relatorios::pdf;
pub use utsim_ferramentas::relatorios::*;
