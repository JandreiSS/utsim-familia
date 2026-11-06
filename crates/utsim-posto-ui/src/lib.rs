//! Telas egui comuns aos apps UTSim Qualidade e UTSim Segurança do Trabalho (etapas QS8 e
//! QS9 de `docs/plano_qualidade_e_seguranca.md`). Os dois apps abrem a MESMA pasta de
//! unidade ([`utsim_posto::Unidade`]).
//!
//! - [`sessao`]: a unidade aberta, o autor lembrado em arquivo e a atualização periódica;
//! - [`inicial`]: criar ou abrir a pasta de unidade;
//! - [`postos`]: lista, edição e importação dos postos de trabalho;
//! - [`trabalho`]: uma tarefa pesada num fio de trabalho, sem travar a tela;
//! - [`util`]: data de hoje, cores e leitura de modelos.

pub mod inicial;
pub mod postos;
pub mod sessao;
pub mod trabalho;
pub mod util;

pub use inicial::TelaInicial;
pub use postos::TelaPostos;
pub use sessao::{Config, Sessao};
pub use trabalho::Trabalho;
