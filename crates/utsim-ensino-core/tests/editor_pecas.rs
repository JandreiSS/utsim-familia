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

/// The mixed-model line of the campaign (`ato4/mix`), opened again in the editor with the
/// files of its drawn statement.
fn mix() -> Edicao {
    let mut ed = Edicao::abrir(
        Missao::parse(&ler("ato4/mix.ron")).unwrap(),
        &ler("modelos/ventilador_mix.ron"),
        &ler("ato4/mix.md"),
    )
    .unwrap();
    let c = ed.base.cena.clone().unwrap();
    for p in utsim_ensino_core::editor_cena::caminhos_cena(&c, &[]) {
        let b = std::fs::read(raiz().join("ato4").join(&p)).unwrap();
        ed.cena_arquivos.insert(p, b);
    }
    ed
}

#[test]
fn mix_aberto_no_editor_guarda_a_alavanca_e_o_mix() {
    let ed = mix();
    assert_eq!(ed.tipo_decisao(), TipoDecisao::Balanceamento);
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    // The sequence lever stays beside the pieces, and so does the reference.
    let m = ed.missao();
    let campanha = Missao::parse(&ler("ato4/mix.ron")).unwrap();
    assert_eq!(m.alavancas, campanha.alavancas);
    assert_eq!(m.referencia, campanha.referencia);
    assert_eq!(m.balanceamento, campanha.balanceamento);
    assert_eq!(ed.combinacoes(), 3);
    let chaves = ed.chaves_de_decisao();
    for k in ["balanceamento.pico", "decisao.sequencia"] {
        assert!(chaves.iter().any(|c| c == k), "{k} em {chaves:?}");
    }
    // The model sets the label through the sequence of the source: no warning about it.
    assert!(
        !ed.avisos_das_pecas().iter().any(|a| a.contains("rótulo")),
        "{:?}",
        ed.avisos_das_pecas()
    );
}

#[test]
fn modelos_do_mix_entram_e_saem() {
    let mut ed = ventilador();
    ed.novo_modelo_mix();
    let b = ed.base.balanceamento.as_ref().unwrap();
    // The first model turns the single-model line into two models with the same times.
    assert_eq!(b.mix.len(), 2);
    let a = &b.tarefas[0];
    assert_eq!(a.tempos.len(), 2);
    assert!(a.tempos.values().all(|v| *v == a.tempo));
    assert!(b.validar().is_empty(), "{:?}", b.validar());
    // A different time for one model moves the average by its share.
    let id = b.mix[1].id.clone();
    let b = ed.base.balanceamento.as_mut().unwrap();
    b.tarefas[0].tempos.insert(id.clone(), 40.0);
    b.mix[1].parcela = 3.0;
    b.acertar_medias();
    assert_eq!(b.tarefas[0].tempo, (20.0 + 3.0 * 40.0) / 4.0);
    assert!(b.validar().is_empty(), "{:?}", b.validar());
    // The model of the ventilator line sets no model label: the editor warns.
    assert!(
        ed.avisos_das_pecas()
            .iter()
            .any(|a| a.contains("rótulo \"modelo\"")),
        "{:?}",
        ed.avisos_das_pecas()
    );
    // A third model, then back to a single model: the last averages stay as the times.
    ed.novo_modelo_mix();
    assert_eq!(ed.base.balanceamento.as_ref().unwrap().mix.len(), 3);
    let ids: Vec<String> = ed
        .base
        .balanceamento
        .as_ref()
        .unwrap()
        .mix
        .iter()
        .map(|m| m.id.clone())
        .collect();
    ed.remover_modelo_mix(&ids[2]);
    ed.remover_modelo_mix(&ids[0]);
    let b = ed.base.balanceamento.as_ref().unwrap();
    assert!(b.mix.is_empty() && b.tarefas.iter().all(|t| t.tempos.is_empty()));
    assert_eq!(b.tarefas[0].tempo, 35.0);
    assert!(b.validar().is_empty(), "{:?}", b.validar());
    // A new task gets a time for each model.
    ed.novo_modelo_mix();
    ed.nova_tarefa();
    let b = ed.base.balanceamento.as_ref().unwrap();
    assert_eq!(b.tarefas.last().unwrap().tempos.len(), 2);
}
