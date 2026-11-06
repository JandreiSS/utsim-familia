//! Quality engineering on top of the simulation (stage QS3 of
//! `docs/plano_qualidade_e_seguranca.md`): indicators of the quality blocks of a model
//! ([`simulado`]) and their self-contained HTML report ([`relatorio`]). Base of the
//! UTSim Qualidade app.

pub mod relatorio;
pub mod simulado;

pub use simulado::{Indicadores, Medicao, RelatorioQualidade, Replicacao, rodar};
