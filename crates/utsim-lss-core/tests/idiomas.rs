//! As receitas nas cinco línguas: nenhuma chave falta, o nome e o rótulo mostrado mudam com
//! a língua, e o rótulo guardado no resultado continua em português (é procurado pelo nome).

use utsim_idiomas::Idioma;
use utsim_lss_core::analises::{self, Recipe, Table};
use utsim_lss_core::texto::{TEXTOS, rotulo};

#[test]
fn as_receitas_nas_cinco_linguas() {
    assert!(TEXTOS.faltas().is_empty(), "{:?}", TEXTOS.faltas());
    let receita = Recipe::Descriptive { col: 0 };
    let tabela = Table::parse("x
1
2
3
4
");
    let mut nomes = Vec::new();
    for i in Idioma::TODOS {
        utsim_idiomas::definir_nesta_thread(Some(i));
        nomes.push(receita.label());
        let o = analises::run(&receita, &tabela).unwrap();
        assert!(o.lines.iter().any(|(l, _)| l == "média"), "{:?}", o.lines);
        assert!(o.value("média").is_some());
        assert!(!o.verdict.is_empty());
    }
    nomes.dedup();
    assert_eq!(nomes.len(), Idioma::TODOS.len(), "{nomes:?}");

    utsim_idiomas::definir_nesta_thread(Some(Idioma::En));
    assert_eq!(rotulo("média"), "mean");
    assert_eq!(rotulo("valor-p"), "p-value");
    assert_eq!(rotulo("média de turno A"), "mean of turno A");
    assert_eq!(rotulo("valor-p A"), "p-value A");
    assert_eq!(rotulo("coluna do usuário"), "coluna do usuário");
    utsim_idiomas::definir_nesta_thread(Some(Idioma::PtBr));
    assert_eq!(rotulo("média de turno A"), "média de turno A");
    utsim_idiomas::definir_nesta_thread(None);
}
