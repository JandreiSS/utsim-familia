//! Telas egui comuns aos apps UTSim Qualidade e UTSim Segurança do Trabalho (etapas QS8 e
//! QS9 de `docs/plano_qualidade_e_seguranca.md`). Os apps abrem a MESMA pasta de
//! unidade ([`utsim_posto::Unidade`]).
//!
//! - [`ferramentas`]: os estudos com as ferramentas da qualidade e da segurança.
//! - [`sessao`]: a unidade aberta, o autor lembrado em arquivo e a atualização periódica;
//! - [`inicial`]: criar ou abrir a pasta de unidade;
//! - [`modelo`]: abre o `.ron` do UTSim, edita os campos de cada nó e grava de volta;
//! - [`postos`]: lista, edição e importação dos postos de trabalho;
//! - [`trabalho`]: uma tarefa pesada num fio de trabalho, sem travar a tela;
//! - [`util`]: data de hoje, cores e leitura de modelos.

pub mod abrir;
pub mod ferramentas;
pub mod inicial;
pub mod modelo;
pub mod postos;
pub mod sessao;
pub mod trabalho;
pub mod util;

pub use abrir::{Abrir, Destino};
pub use ferramentas::{FnLeituras, FnLimites, FnPuxar, FnSimular, TelaFerramentas};
pub use inicial::{Marca, Saida, TelaInicial};
pub use modelo::{DESTAQUE, FnResumo, Resumo, TelaModelo};
pub use postos::TelaPostos;
pub use sessao::{Config, Sessao, linhas_da_barra};
pub use trabalho::Trabalho;
