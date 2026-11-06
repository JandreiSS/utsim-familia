//! The campaign built into the app (acts 1 to 3), from `ensino/campanha`.

use crate::pacote::{Arquivos, PacoteCarregado, carregar};

mod gerado {
    include!(concat!(env!("OUT_DIR"), "/campanha.rs"));
}

/// The files of the built-in campaign.
pub fn arquivos() -> Arquivos {
    gerado::ARQUIVOS
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_vec()))
        .collect()
}

/// The built-in campaign, read and validated.
pub fn carregar_campanha() -> Result<PacoteCarregado, Vec<String>> {
    carregar(&arquivos())
}
