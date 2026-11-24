//! The names the core gives to the screens and reports (the steps of the inventory), in the
//! language now: the keys of `idiomas/<código>/seguranca.ftl`, the same catalog as the app
//! (`docs/traducao.md`).

use utsim_idiomas::Catalogo;

/// The texts of the UTSim Segurança in the five languages, embedded from `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("seguranca");

/// The text of a key in the language now.
pub fn tx(chave: &'static str) -> &'static str {
    TEXTOS.t(chave)
}
