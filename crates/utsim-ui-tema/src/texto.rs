//! Os textos do tema nas cinco línguas da família: as chaves de `idiomas/<código>/tema.ftl`
//! (`docs/traducao.md`). A abertura, o campo de caminho, o seletor de arquivos e o campo de
//! data leem daqui na língua de agora; os apps que passam os próprios textos continuam podendo.

use utsim_idiomas::Catalogo;

/// Os textos do tema nas cinco línguas, embutidos de `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("tema");

/// O texto de uma chave na língua de agora.
pub fn tx(chave: &'static str) -> &'static str {
    TEXTOS.t(chave)
}

#[cfg(test)]
mod tests {
    use super::TEXTOS;
    use crate::arquivos::{self, Textos};
    use crate::campos::TextosData;
    use utsim_idiomas::Idioma;

    #[test]
    fn o_tema_nas_cinco_linguas() {
        utsim_idiomas::definir_nesta_thread(Some(Idioma::PtBr));
        let pt = Textos::atual();
        assert_eq!(pt.procurar, Textos::PT.procurar);
        assert_eq!(pt.dica_pasta, Textos::PT.dica_pasta);
        assert_eq!(arquivos::RON.nome_na_tela(), "Modelo do UTSim (.ron)");
        assert_eq!(
            TextosData::default()
                .antes
                .replace("{limite}", "01/01/2026"),
            "Esta data vem antes de 01/01/2026."
        );
        #[cfg(feature = "abertura")]
        assert_eq!(
            crate::abertura::Rotulos::atual().o_que_fazer,
            crate::abertura::Rotulos::PT.o_que_fazer
        );
        let mut vistos = Vec::new();
        for i in Idioma::TODOS {
            utsim_idiomas::definir_nesta_thread(Some(i));
            vistos.push(Textos::atual().todos_os_arquivos);
            assert!(TextosData::default().antes.contains("{limite}"));
            assert!(!arquivos::PDF.nome_na_tela().starts_with("arquivos-"));
        }
        vistos.dedup();
        assert_eq!(vistos.len(), Idioma::TODOS.len(), "{vistos:?}");
        utsim_idiomas::definir_nesta_thread(None);
        assert!(TEXTOS.faltas().is_empty(), "{:?}", TEXTOS.faltas());
    }
}
