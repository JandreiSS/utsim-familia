//! As ferramentas nas cinco línguas: nenhuma chave falta e os nomes de tela mudam com a
//! língua, sem mudar as chaves gravadas.

use utsim_ferramentas::cinco_s::Sense;
use utsim_ferramentas::ishikawa::{SIX_M, category_name};
use utsim_ferramentas::texto::TEXTOS;
use utsim_idiomas::Idioma;

#[test]
fn as_ferramentas_nas_cinco_linguas() {
    assert!(TEXTOS.faltas().is_empty(), "{:?}", TEXTOS.faltas());
    let mut nomes = Vec::new();
    for i in Idioma::TODOS {
        utsim_idiomas::definir_nesta_thread(Some(i));
        nomes.push(category_name("medicao"));
        for s in Sense::ALL {
            assert!(!s.name().is_empty());
        }
    }
    nomes.sort();
    nomes.dedup();
    assert_eq!(nomes.len(), Idioma::TODOS.len(), "{nomes:?}");

    // Em português, os nomes são os de SIX_M; uma categoria livre passa como está.
    utsim_idiomas::definir_nesta_thread(Some(Idioma::PtBr));
    for (chave, nome) in SIX_M {
        assert_eq!(category_name(chave), nome);
    }
    assert_eq!(category_name("Fornecedor"), "Fornecedor");
}
