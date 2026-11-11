//! Stage D of the mission editor: modelling missions. The model opened becomes the answer
//! key with suggested criteria, the student starts from another model, the check runs both
//! and the mission goes to the package and back.

use std::path::PathBuf;

use utsim_ensino_core::editor::{Edicao, PacoteProfessor, calibrar};
use utsim_ensino_core::editor_modelagem::{TipoCriterio, sugerir_criterios};
use utsim_ensino_core::modelagem::Criterio;
use utsim_model::Model;

fn ler(nome: &str) -> String {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(raiz.join("ensino/campanha/modelos").join(nome)).unwrap()
}

fn edicao() -> Edicao {
    let mut ed = Edicao::nova("prof/padaria", "A padaria", &ler("padaria_gabarito.ron")).unwrap();
    ed.virar_modelagem();
    ed
}

#[test]
fn criterios_sugeridos_pelo_gabarito() {
    let g = Model::parse(&ler("padaria_gabarito.ron")).unwrap();
    let c = sugerir_criterios(&g);
    let tipos: Vec<TipoCriterio> = c.iter().map(TipoCriterio::de).collect();
    for t in [
        TipoCriterio::Objetos,
        TipoCriterio::Quebras,
        TipoCriterio::Capacidade,
        TipoCriterio::Medida,
    ] {
        assert!(tipos.contains(&t), "{t:?} in {c:?}");
    }
    assert!(c.iter().any(|x| matches!(x,
        Criterio::Capacidade { minimo, .. } if *minimo == 10)));
    assert!(c.iter().any(|x| matches!(x,
        Criterio::Objetos { tipo, minimo, .. } if tipo == "Processor" && *minimo == 4)));
}

#[test]
fn virar_modelagem_e_conferir() {
    let mut ed = edicao();
    assert!(ed.e_modelagem());
    assert_eq!(ed.gabarito_ron.as_deref(), Some(ed.modelo_ron.as_str()));
    let m = ed.missao();
    assert!(m.alavancas.is_empty() && m.metricas.is_empty());
    // No levers or measures to mark: only the statement and the criteria count.
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    assert!(calibrar(&ed).is_err());
    // The student gets the answer key until another model is loaded.
    assert!(ed.avisos_da_modelagem()[0].contains("próprio gabarito"));
    let c = ed.conferir_modelagem().unwrap();
    assert!(c.gabarito.iter().all(|k| k.ok), "{:?}", c.gabarito);
    assert!(c.erros().iter().any(|x| x.contains("não ensina nada")));

    ed.trocar_modelo_inicial(&ler("padaria_inicial.ron"))
        .unwrap();
    assert!(ed.alavancas.iter().all(|a| !a.usar));
    let c = ed.conferir_modelagem().unwrap();
    assert!(c.erros().is_empty(), "{:?}", c.erros());
    assert!(c.inicial.iter().any(|k| !k.ok));
}

#[test]
fn erros_dos_criterios() {
    let mut ed = edicao();
    let md = ed.base.modelagem.as_mut().unwrap();
    md.criterios = vec![
        Criterio::Objetos {
            rotulo: "Muitas filas".into(),
            tipo: "Queue".into(),
            minimo: 40,
            peso: 1.0,
        },
        Criterio::Objetos {
            rotulo: "".into(),
            tipo: "Forno".into(),
            minimo: 0,
            peso: -1.0,
        },
        Criterio::Capacidade {
            rotulo: "Paralelo".into(),
            minimo: 1,
            peso: 1.0,
        },
        TipoCriterio::Medida.padrao(None),
    ];
    if let Criterio::Medida { valor, .. } = &mut md.criterios[3] {
        valor.clear();
    }
    let e = ed.erros().join("\n");
    for x in [
        "não cumpre \"Muitas filas\"",
        "critério 2 sem rótulo",
        "o tipo 'Forno' não existe",
        "mínimo de zero",
        "peso negativo",
        "capacidade mínima",
        "precisa de uma chave",
    ] {
        assert!(e.contains(x), "{x} in {e}");
    }
    ed.base.modelagem.as_mut().unwrap().criterios.clear();
    assert!(
        ed.erros()
            .iter()
            .any(|x| x.contains("ao menos um critério"))
    );
}

#[test]
fn modelagem_vai_ao_pacote_e_volta() {
    let mut ed = edicao();
    ed.trocar_modelo_inicial(&ler("padaria_inicial.ron"))
        .unwrap();
    let mut p = PacoteProfessor::novo("prof", "Pacote", "Prof");
    let caminho = p.gravar(&mut ed, 0).unwrap();
    let pasta = caminho.trim_end_matches("missao.ron");
    assert_eq!(
        p.arquivos
            .get(&format!("{pasta}gabarito.ron"))
            .map(Vec::as_slice),
        ed.gabarito_ron.as_deref().map(str::as_bytes)
    );
    let carregado = p.conferir().unwrap();
    assert!(carregado.missoes[0].gabarito.is_some());
    let de_novo = p.abrir(&caminho).unwrap();
    assert!(de_novo.e_modelagem());
    assert_eq!(de_novo.gabarito_ron, ed.gabarito_ron);
    assert_eq!(de_novo.modelo_ron, ed.modelo_ron);
    assert_eq!(de_novo.base.modelagem, ed.base.modelagem);

    // Back to a mission of levers: the answer key is the model again.
    let mut ed = de_novo;
    ed.deixar_modelagem();
    assert!(!ed.e_modelagem() && ed.gabarito_ron.is_none());
    assert_eq!(ed.modelo_ron, ler("padaria_gabarito.ron"));
}
