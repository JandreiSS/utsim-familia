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
