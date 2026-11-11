//! Stage E of the mission editor: missions of pieces. The balancing and the layout come
//! suggested from the model, the calibration finds the optimum by the exact search, and
//! the mission goes to the package and back.

use std::path::PathBuf;

use utsim_ensino_core::conferir::conferir;
use utsim_ensino_core::editor::{Edicao, PacoteProfessor, calibrar};
use utsim_ensino_core::editor_pecas::{
    METRICA_ARRANJO, METRICA_BALANCEAMENTO, TipoDecisao, arranjo_sugerido, balanceamento_sugerido,
};
use utsim_ensino_core::missao::Missao;
use utsim_model::Model;

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ensino/campanha")
}

fn ler(caminho: &str) -> String {
    std::fs::read_to_string(raiz().join(caminho)).unwrap()
}

#[test]
fn balanceamento_sugerido_pelo_modelo() {
    let m = Model::parse(&ler("modelos/ventilador.ron")).unwrap();
    let b = balanceamento_sugerido(&m);
    assert_eq!(b.estacoes, 10);
    assert_eq!(b.takt, 60.0);
    // The stations have no time yet: two example tasks.
    assert_eq!(b.tarefas.len(), 2);
    assert_eq!(b.tarefas[1].depois_de, vec!["A".to_string()]);
    assert!(b.validar().is_empty(), "{:?}", b.validar());
}

#[test]
fn arranjo_sugerido_pelo_modelo() {
    let m = Model::parse(&ler("modelos/oficina.ron")).unwrap();
    let a = arranjo_sugerido(&m);
    let ids: Vec<&str> = a.maquinas.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "doca",
            "serra",
            "torno",
            "retifica",
            "inspecao",
            "furadeira",
            "fresa"
        ]
    );
    assert!(a.maquinas[0].fixa && a.maquinas[1..].iter().all(|x| !x.fixa));
    assert_eq!((a.colunas, a.linhas), (4, 3));
    assert_eq!(a.fluxos.len(), 5);
    let eixos = a.fluxos.iter().find(|f| f.nome == "eixos").unwrap();
    assert_eq!(
        eixos.roteiro,
        ["doca", "serra", "torno", "retifica", "inspecao", "doca"]
    );
    assert!((eixos.volume - 8.3).abs() < 1e-9);
    assert!(a.validar().is_empty(), "{:?}", a.validar());
    assert!(a.otimo().is_some());
}

/// The ventilator line of the campaign, drafted in the editor with the campaign tasks.
fn ventilador() -> Edicao {
    let mut ed = Edicao::nova(
        "prof/ventilador",
        "Ventilador",
        &ler("modelos/ventilador.ron"),
    )
    .unwrap();
    ed.trocar_tipo_decisao(TipoDecisao::Balanceamento);
    let campanha = Missao::parse(&ler("ato4/ventilador.ron")).unwrap();
    ed.base.balanceamento = campanha.balanceamento;
    ed
}

#[test]
fn balanceamento_calibra_pelo_otimo() {
    let mut ed = ventilador();
    assert!(ed.e_pecas());
    assert_eq!(ed.tipo_decisao(), TipoDecisao::Balanceamento);
    assert!(ed.alavancas.iter().all(|a| !a.usar));
    assert!(ed.metricas[0].usar && ed.metricas[0].metrica.nome == METRICA_BALANCEAMENTO);
    assert!(ed.metricas[1..].iter().all(|m| !m.usar));
    assert!(
        ed.chaves_de_decisao()
            .contains(&"balanceamento.minimo".to_string())
    );
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    assert!(
        ed.avisos_das_pecas().is_empty(),
        "{:?}",
        ed.avisos_das_pecas()
    );

    let c = calibrar(&ed).unwrap();
    ed.aplicar(&c, true);
    let b = ed.base.balanceamento.clone().unwrap();
    let o = b.otimo().unwrap();
    assert_eq!(b.resumo(&b.atribuicao(&ed.base.referencia)), b.resumo(&o));
    let meta = ed.metricas[0].metrica.meta.unwrap();
    assert!(meta > 0.7 && meta < b.resumo(&o).eficiencia, "{meta}");
    // The reference of the pieces survives in the mission written.
    assert_eq!(ed.missao().referencia, ed.base.referencia);
    let p = ed.carregar().unwrap();
    let conf = conferir(&p.missoes[0]).unwrap();
    assert!(conf.referencia_confere, "{:?}", conf.erros);
}

#[test]
fn arranjo_vai_ao_pacote_e_volta() {
    let mut ed = Edicao::nova("prof/oficina", "Oficina", &ler("modelos/oficina.ron")).unwrap();
    ed.trocar_tipo_decisao(TipoDecisao::Arranjo);
    assert_eq!(ed.metricas[0].metrica.nome, METRICA_ARRANJO);
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    let c = calibrar(&ed).unwrap();
    ed.aplicar(&c, true);
    assert!(ed.base.referencia.keys().any(|k| k.starts_with("pos.")));

    let mut p = PacoteProfessor::novo("prof", "Pacote", "Prof");
    let caminho = p.gravar(&mut ed, 0).unwrap();
    p.conferir().unwrap();
    let mut de_novo = p.abrir(&caminho).unwrap();
    assert_eq!(de_novo.tipo_decisao(), TipoDecisao::Arranjo);
    assert_eq!(de_novo.base.arranjo, ed.base.arranjo);
    assert_eq!(de_novo.base.referencia, ed.base.referencia);

    // Back to levers: the pieces and their measure leave.
    de_novo.trocar_tipo_decisao(TipoDecisao::Alavancas);
    assert!(!de_novo.e_pecas() && de_novo.base.arranjo.is_none());
    assert!(de_novo.base.referencia.is_empty());
    assert!(
        de_novo
            .metricas
            .iter()
            .all(|m| m.metrica.nome != METRICA_ARRANJO)
    );
}

#[test]
fn erros_das_pecas() {
    // A model without stations est1, est2...
    let mut ed = Edicao::nova("prof/x", "X", &ler("modelos/oficina.ron")).unwrap();
    ed.trocar_tipo_decisao(TipoDecisao::Balanceamento);
    ed.base.balanceamento.as_mut().unwrap().tarefas[0].tempo = 500.0;
    let e = ed.erros().join("\n");
    for x in ["precisa de estações", "passa do takt"] {
        assert!(e.contains(x), "{x} in {e}");
    }

    let mut ed = Edicao::nova("prof/y", "Y", &ler("modelos/oficina.ron")).unwrap();
    ed.trocar_tipo_decisao(TipoDecisao::Arranjo);
    ed.novo_fluxo();
    let a = ed.base.arranjo.as_mut().unwrap();
    let n = a.fluxos.len();
    a.fluxos[n - 1].roteiro.truncate(1);
    a.fluxos[n - 1].volume = 0.0;
    ed.remover_maquina("serra");
    let livres = ed.maquinas_livres();
    assert_eq!(livres, ["serra"]);
    ed.nova_maquina("serra");
    let a = ed.base.arranjo.as_ref().unwrap();
    assert!(a.maquinas.iter().any(|m| m.id == "serra"));
    // Serra came back but left the routes it was in.
    assert!(
        a.fluxos
            .iter()
            .all(|f| !f.roteiro.contains(&"serra".to_string()))
    );
    let e = ed.erros().join("\n");
    for x in ["duas máquinas", "sem volume"] {
        assert!(e.contains(x), "{x} in {e}");
    }
}
