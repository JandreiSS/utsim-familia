//! Os textos das abas Qualidade, Segurança e Manutenção dos nós nas cinco línguas da
//! família: as chaves de `idiomas/<código>/abas.ftl` (`docs/traducao.md`). Saem traduzidos
//! os rótulos, as ajudas, os nomes das abas e os erros; ficam como estão os caminhos, as
//! escolhas (os valores do arquivo do modelo, como `Customer` e `Scrap`) e o `sim`/`não` dos
//! interruptores, que é o valor trocado com [`crate::abas::editar`].

use std::fmt::Display;

use utsim_idiomas::{Catalogo, Valor};

/// Os textos das abas nas cinco línguas, embutidos de `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("abas");

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
    if let Ok(f) = s.parse::<f64>()
        && f.is_finite()
        && f.to_string() == s
    {
        return f.into();
    }
    s.into()
}
