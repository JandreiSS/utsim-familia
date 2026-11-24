//! The names the core gives to the screens and reports (cost categories, data origins), in
//! the language now: the keys of `idiomas/<código>/qualidade-nucleo.ftl`, apart from the
//! app (`docs/traducao.md`).

use utsim_idiomas::Catalogo;

/// The texts of the UTSim Qualidade in the five languages, embedded from `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("qualidade-nucleo");

/// The text of a key in the language now.
pub fn tx(chave: &'static str) -> &'static str {
    TEXTOS.t(chave)
}
