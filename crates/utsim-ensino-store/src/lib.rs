//! Storage of UTSim Ensino (`docs/plano_ensino_e_jogo.md`, sections 2.3, 4.3 and 4.6). No
//! server and no Drive API: packages, deliveries and class folders are plain files.
//!
//! - [`analise`]: the data of a mission opened in UTSim Estatística or UTSim Qualidade;
//! - [`arquivos`]: packages (folder or `.utse`), deliveries (`.utsr`), invitations (`.utsc`);
//! - [`caso`]: the case builder on disk (draft package, check, `.utse`);
//! - [`chave`]: ed25519 keys of the student and the teacher;
//! - [`diario`]: the signed attempt journal chained by hash (both on the shared crate
//!   `utsim-diario`, decision E12);
//! - [`pasta`]: the class folder (local or synced by Google Drive for desktop);
//! - [`correcao`]: batch grading on the teacher's machine;
//! - [`desafio`]: the class scoreboard and the team solutions;
//! - [`planilha`]: the grades as `.xlsx`;
//! - [`prova`]: exams published in the class folder and rebuilt from the seed;
//! - [`seis_sigma`]: the training project of the Lean Six Sigma case for UTSim Seis Sigma;
//! - [`tres_d`]: "Ver em 3D", the attempt opened in the UTSim client of the launcher;
//! - [`perfil`]: the local profile (identity, progress, journal, joined class).

pub mod analise;
pub mod arquivos;
pub mod caso;
pub mod chave;
pub mod correcao;
pub mod desafio;
pub mod diario;
pub mod pasta;
pub mod perfil;
pub mod planilha;
pub mod prova;
pub mod seis_sigma;
pub mod tres_d;

#[derive(Debug, thiserror::Error)]
pub enum Erro {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalido(String),
}

pub type Result<T> = std::result::Result<T, Erro>;

impl From<utsim_diario::Erro> for Erro {
    fn from(e: utsim_diario::Erro) -> Erro {
        match e {
            utsim_diario::Erro::Io(e) => Erro::Io(e),
            utsim_diario::Erro::Invalido(s) => Erro::Invalido(s),
        }
    }
}
