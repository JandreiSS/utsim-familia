//! The case builder makes a new mission from a model of `examples/`: suggested levers and
//! measures, a draft package that loads, the reference by the exhaustive search and goals
//! the mission passes the checker with.

use std::path::PathBuf;

use utsim_ensino_core::construtor::{
    calibrar, pacote, rascunho, resumo, sugerir_alavancas, sugerir_metricas, texto_inicial,
};
use utsim_ensino_core::missao::Alavanca;
use utsim_ensino_core::pacote::carregar;
use utsim_model::Model;

#[test]
fn missao_nova_a_partir_de_um_exemplo() {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let txt = std::fs::read_to_string(raiz.join("examples/tutorial_02_linha.ron")).unwrap();
    let modelo = Model::parse(&txt).unwrap();

    let mut alavancas = sugerir_alavancas(&modelo);
    assert_eq!(alavancas.len(), 3, "uma alavanca por máquina");
    // The teacher keeps two levers and gives them a budget.
    alavancas.retain(|a| a.id() != "corte_capacidade");
    for a in alavancas.iter_mut() {
        if let Alavanca::Quantidade { ate, .. } = a {
            *ate -= 1;
        }
    }
    let metricas = sugerir_metricas(&modelo);
    assert!(metricas.iter().any(|m| m.nome == "saida_lead_time"));
    let mut missao = rascunho(
        "prof/linha",
        "A linha do tutorial",
        &modelo,
        alavancas,
        metricas,
    );
    missao.orcamento = Some(10000.0);
    let texto = texto_inicial(&missao.titulo, &modelo, &missao.alavancas);
    assert!(texto.contains("Capacidade de furacao"));

    let arquivos = pacote("Linha do tutorial", &missao, &txt, &texto);
    assert!(carregar(&arquivos).is_ok());
    let (calibrado, c) = calibrar(&arquivos).unwrap_or_else(|e| panic!("{}", e.join("\n")));
    eprintln!("{}", resumo(&c));
    assert!(c.passou(), "{}", resumo(&c));
    let p = carregar(&calibrado).unwrap();
    let m = &p.missoes[0].missao;
    // The reference buys the drilling machine, and the lead time has a goal now.
    assert_eq!(m.referencia.get("furacao_capacidade"), Some(&2.0));
    assert!(m.metricas[0].meta.is_some());
}
