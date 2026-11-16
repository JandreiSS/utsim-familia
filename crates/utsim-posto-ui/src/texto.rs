//! Os textos das telas comuns a Qualidade e Segurança nas cinco línguas da família: as chaves
//! de `idiomas/<código>/posto.ftl` (`docs/traducao.md`). O que fica gravado (nomes de coluna,
//! tipos de nó do modelo, valores dos parâmetros) não muda com a língua.

use std::fmt::Display;

use utsim_idiomas::{Catalogo, Valor};

/// Os textos das telas comuns nas cinco línguas, embutidos de `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("posto");

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
