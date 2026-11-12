//! The safety inventory on a unit folder: five steps, company matrix, report notice.

use std::path::Path;

use utsim_posto::{Posto, Unidade, sugerir_postos};
use utsim_seguranca_core::inventario::{
    self, ACOES, PERIGOS, REAVALIACOES, gravar_matriz, perigos_da_exposicao,
};
use utsim_seguranca_core::relatorio::{self, Bloco};
use utsim_seguranca_core::{
    AVISO_LEGAL, Acao, Hierarquia, Inventario, Matriz, Perigo, Reavaliacao, Situacao,
};

#[test]
fn five_steps_on_a_unit_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut u = Unidade::criar(dir.path(), "Laboratório", "prof").unwrap();
    // 1. Identify the stations from the crossing model.
    let caminho =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cruzamento_empilhadeira.ron");
    let model = utsim_model::Model::load(&caminho).unwrap();
    for p in sugerir_postos(&model, &u.postos(), &caminho.display().to_string()) {
        u.gravar_posto(&p).unwrap();
    }
    let postos = u.postos();
    assert_eq!(postos.len(), 2);
    let prensa = postos
        .iter()
        .find(|p| p.objeto.as_deref() == Some("prensa"))
        .unwrap();
    assert!(prensa.epi.contains(&"protetor_auricular".to_string()));
    // 2 and 3. A hazard assessed with the matrix.
    let pid = u
        .gravar(
            PERIGOS,
            "",
            &Perigo {
                posto: prensa.id.clone(),
                descricao: "Prensamento das mãos".into(),
                probabilidade: 3,
                severidade: 4,
                ..Perigo::default()
            },
        )
        .unwrap();
    let inv = Inventario::ler(&u);
    assert_eq!(
        inv.matriz.niveis[inv.nivel_atual(&inv.perigos[0])].nome,
        "Crítico"
    );
    assert_eq!(inv.sem_acao().len(), 1);
    // 4. An engineering control.
    u.gravar(
        ACOES,
        "",
        &Acao {
            perigo: pid.clone(),
            hierarquia: Hierarquia::Engenharia,
            descricao: "Comando bimanual e cortina de luz".into(),
            responsavel: "manutenção".into(),
            prazo: "2026-12-01".into(),
            situacao: Situacao::Concluida,
            eficaz: Some(true),
            ..Acao::default()
        },
    )
    .unwrap();
    // 5. Reassessed after the action.
    u.gravar(
        REAVALIACOES,
        "",
        &Reavaliacao {
            perigo: pid.clone(),
            data: "2026-12-10".into(),
            probabilidade: 0,
            severidade: 4,
            nota: "proteção conferida".into(),
            ..Reavaliacao::default()
        },
    )
    .unwrap();
    let inv = Inventario::ler(&u);
    assert!(inv.sem_acao().is_empty());
    assert_eq!(
        inv.matriz.niveis[inv.nivel_atual(&inv.perigos[0])].nome,
        "Médio"
    );
    assert!(inv.atrasadas("2027-01-01").is_empty());
    // The report opens and closes with the legal notice.
    let b = relatorio::blocos(&inv, "Laboratório", &[], "2026-12-10");
    assert_eq!(b[2], Bloco::Aviso(AVISO_LEGAL.to_string()));
    assert_eq!(b.last(), Some(&Bloco::Aviso(AVISO_LEGAL.to_string())));
    let html = relatorio::html(&b);
    assert!(html.contains("Prensamento das mãos"));
    assert!(html.contains("Comando bimanual"));
    let pdf = relatorio::pdf(&b, "prof, 2026-12-10");
    assert!(pdf.starts_with(b"%PDF"));
    // The notice is in the page text (WinAnsi bytes of an ASCII stretch of it).
    let trecho = b"profissional legalmente habilitado";
    assert!(
        pdf.windows(trecho.len()).any(|w| w == trecho),
        "aviso fora do PDF"
    );
    // A company matrix replaces the default in the whole register.
    let csv = "niveis;Tolerável;Moderado;Intolerável\n\
               p/s;Leve;Séria;Fatal\n\
               Baixa;Tolerável;Tolerável;Moderado\n\
               Média;Tolerável;Moderado;Intolerável\n\
               Alta;Moderado;Intolerável;Intolerável\n";
    let m = Matriz::de_csv(csv, &Matriz::default()).unwrap();
    gravar_matriz(&mut u, &m).unwrap();
    let outra = Unidade::abrir(dir.path(), "aluno").unwrap();
    let inv = Inventario::ler(&outra);
    assert_eq!(inv.matriz, m);
    // Exposure of the model suggests hazards for the stations the person serves.
    let e = utsim_exposicao::analyze(&model, 0, utsim_exposicao::Options::default()).unwrap();
    let sug = perigos_da_exposicao(&inv.postos, &e, &model);
    assert!(
        sug.iter()
            .any(|p| p.descricao.contains("corredor_empilhadeira"))
    );
    assert!(sug.iter().all(|p| p.origem == "exposicao"));
    let _ = inventario::CONFIG;
    let _: Vec<Posto> = Vec::new();
}

#[test]
fn hazards_on_the_node_join_the_inventory() {
    use utsim_model::safety::{HazardDef, HazardGroup, SafetyDef};
    let dir = tempfile::tempdir().unwrap();
    let mut u = Unidade::criar(dir.path(), "Laboratório", "prof").unwrap();
    let caminho =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cruzamento_empilhadeira.ron");
    let mut model = utsim_model::Model::load(&caminho).unwrap();
    for p in sugerir_postos(&model, &u.postos(), &caminho.display().to_string()) {
        u.gravar_posto(&p).unwrap();
    }
    let prensa = model
        .objects
        .iter_mut()
        .find(|o| o.name == "prensa")
        .unwrap();
    prensa.safety = Some(Box::new(SafetyDef {
        hazards: vec![HazardDef {
            id: "h1".into(),
            group: HazardGroup::Physical,
            description: "Ruído da prensa".into(),
            controls: "Enclausuramento parcial".into(),
            probability: 3,
            severity: 3,
            ..HazardDef::default()
        }],
        ..SafetyDef::default()
    }));
    let postos = u.postos();
    let posto = postos
        .iter()
        .find(|p| p.objeto.as_deref() == Some("prensa"))
        .unwrap();

    // Without the model, only the unit's hazards.
    assert!(Inventario::ler(&u).perigos.is_empty());
    let inv = Inventario::ler(&u).com_nos(|_| Some(&model));
    let meus = inv.perigos_do_posto(&posto.id);
    assert_eq!(meus.len(), 1);
    let p = meus[0];
    assert!(p.eh_do_no());
    assert_eq!(p.id, format!("no:{}:h1", posto.id));
    assert_eq!(p.grupo, utsim_seguranca_core::inventario::Grupo::Fisico);
    assert_eq!(p.controles, "Enclausuramento parcial");
    // It counts in the alerts, and an action in the unit can point at it.
    assert_eq!(inv.sem_acao().len(), 1);
    u.gravar(
        ACOES,
        "",
        &Acao {
            perigo: p.id.clone(),
            hierarquia: Hierarquia::Engenharia,
            descricao: "Cabine acústica".into(),
            ..Acao::default()
        },
    )
    .unwrap();
    let inv = Inventario::ler(&u).com_nos(|_| Some(&model));
    assert!(inv.sem_acao().is_empty());
    assert_eq!(inv.acoes_do_perigo(&format!("no:{}:h1", posto.id)).len(), 1);
    // A station whose object is not in the model gets nothing from it.
    let outro = postos.iter().find(|p| p.id != posto.id).unwrap();
    assert!(inv.perigos_do_posto(&outro.id).is_empty());
}

#[test]
fn a_unit_hazard_passes_to_the_node_with_its_actions() {
    let dir = tempfile::tempdir().unwrap();
    let mut u = Unidade::criar(dir.path(), "Laboratório", "prof").unwrap();
    let p = Perigo {
        posto: "prensa".into(),
        grupo: inventario::Grupo::Ergonomico,
        descricao: "Postura curvada".into(),
        controles: "Pausas".into(),
        probabilidade: 2,
        severidade: 3,
        ..Perigo::default()
    };
    let id = u.gravar(PERIGOS, "", &p).unwrap();
    let outro = u.gravar(PERIGOS, "", &p).unwrap();
    for perigo in [&id, &outro] {
        u.gravar(
            ACOES,
            "",
            &Acao {
                perigo: perigo.clone(),
                descricao: "Bancada regulável".into(),
                ..Acao::default()
            },
        )
        .unwrap();
    }
    u.gravar(
        REAVALIACOES,
        "",
        &Reavaliacao {
            perigo: id.clone(),
            data: "2026-11-01".into(),
            probabilidade: 1,
            severidade: 3,
            ..Reavaliacao::default()
        },
    )
    .unwrap();

    // The node gets the same fields.
    let h = p.para_o_no();
    assert_eq!(h.group, utsim_model::safety::HazardGroup::Ergonomic);
    assert_eq!(h.description, "Postura curvada");
    assert_eq!(h.controls, "Pausas");
    assert_eq!((h.probability, h.severity), (2, 3));
    assert_eq!(Perigo::do_no("prensa", &h).grupo, p.grupo);

    // Its action and reassessment follow it, the other hazard keeps its own.
    let novo = "no:prensa:h1";
    assert_eq!(inventario::passar_ao_no(&mut u, &id, novo).unwrap(), 2);
    let inv = Inventario::ler(&u);
    assert_eq!(inv.perigos.len(), 1);
    assert_eq!(inv.perigos[0].id, outro);
    assert_eq!(inv.acoes_do_perigo(novo).len(), 1);
    assert_eq!(inv.acoes_do_perigo(&outro).len(), 1);
    assert_eq!(inv.ultima_reavaliacao(novo).unwrap().probabilidade, 1);
    assert!(inv.acoes_do_perigo(&id).is_empty());
}
