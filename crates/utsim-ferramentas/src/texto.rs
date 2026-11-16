//! Os textos de tela das ferramentas comuns nas cinco línguas da família: as chaves de
//! `idiomas/<código>/ferramentas.ftl` (`docs/traducao.md`). Saem traduzidos os nomes dos
//! sensos do 5S e das categorias do 6M e os achados do poka-yoke e do QFD. O que fica gravado
//! no estudo (as chaves das categorias, a lista inicial do 5S) não muda.

use std::fmt::Display;

use utsim_idiomas::{Catalogo, Valor};

/// Os textos das ferramentas nas cinco línguas, embutidos de `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("ferramentas");

/// O texto de uma chave na língua de agora.
pub fn tx(chave: &'static str) -> &'static str {
    TEXTOS.t(chave)
}

/// O texto de uma chave com valores com nome.
pub fn fx(chave: &str, args: &[(&str, Valor)]) -> String {
    TEXTOS.f(chave, args)
}

/// Um valor: número quando se lê de volta igual, senão texto.
pub fn vx(x: &impl Display) -> Valor<'static> {
    let s = x.to_string();
    if let Ok(n) = s.parse::<i64>()
        && n.to_string() == s
    {
        return n.into();
    }
    s.into()
}

/// Um texto como está (o nome que a equipe escreveu).
pub fn sx(s: String) -> Valor<'static> {
    s.into()
}
