//! Storage of UTSim Ensino (`docs/plano_ensino_e_jogo.md`, sections 2.3, 4.3 and 4.6). No
//! server and no Drive API: packages, deliveries and class folders are plain files.
//!
//! - [`arquivos`]: packages (folder or `.utse`), deliveries (`.utsr`), invitations (`.utsc`);
//! - [`chave`]: ed25519 keys of the student and the teacher;
//! - [`diario`]: the signed attempt journal chained by hash;
//! - [`pasta`]: the class folder (local or synced by Google Drive for desktop);
//! - [`correcao`]: batch grading on the teacher's machine;
//! - [`planilha`]: the grades as `.xlsx`;
//! - [`perfil`]: the local profile (identity, progress, journal, joined class).

pub mod arquivos;
pub mod chave;
pub mod correcao;
pub mod diario;
pub mod pasta;
pub mod perfil;
pub mod planilha;

#[derive(Debug, thiserror::Error)]
pub enum Erro {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalido(String),
}

pub type Result<T> = std::result::Result<T, Erro>;

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn unhex(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(Erro::Invalido("hexadecimal de tamanho ímpar".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(s.get(i..i + 2).unwrap_or("zz"), 16)
                .map_err(|_| Erro::Invalido("hexadecimal inválido".into()))
        })
        .collect()
}
