//! Quality engineering on top of the simulation (stage QS3 of
//! `docs/plano_qualidade_e_seguranca.md`): indicators of the quality blocks of a model
//! ([`simulado`]) and their self-contained HTML report ([`relatorio`]). Base of the
//! UTSim Qualidade app.

pub mod amostragem;
pub mod custos;
pub mod oito_d;
pub mod planos;
pub mod relatorio;
pub mod seis_sigma;
pub mod simulado;

pub use simulado::{Indicadores, Medicao, RelatorioQualidade, Replicacao, rodar};
