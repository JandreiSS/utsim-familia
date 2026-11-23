//! Os textos dos componentes nas cinco línguas da família: as chaves de
//! `idiomas/<código>/visual.ftl` (`docs/traducao.md`).

use utsim_idiomas::{Catalogo, Valor};

/// Os textos dos componentes nas cinco línguas, embutidos de `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("visual");

/// O texto de uma chave na língua de agora.
pub fn tx(chave: &'static str) -> &'static str {
    TEXTOS.t(chave)
}

/// O texto de uma chave com valores com nome.
pub fn fx(chave: &str, args: &[(&str, Valor)]) -> String {
    TEXTOS.f(chave, args)
}
