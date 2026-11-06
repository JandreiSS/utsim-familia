//! Pictures of the main screens drawn without a window (the campaign map, mission 1.1
//! before and after a run, with the 2D view, and the teacher's matrix). Each screen must
//! paint something; with `UTSIM_CAPTURAS=<pasta>` the PNGs are saved there for review.

use std::sync::Arc;

use egui::{Context, vec2};
use utsim_ensino_core::nota::RespostaPrevisao;
use utsim_ensino_store::perfil::Perfil;
use utsim_ensino_ui::turma::criar_turma;
use utsim_ensino_ui::{App, NaHora, Tela};
use utsim_ui_tema::captura::{Captura, Imagem};

fn salvar(img: &Imagem, nome: &str) {
    if let Some(d) = std::env::var_os("UTSIM_CAPTURAS") {
        let d = std::path::PathBuf::from(d);
        std::fs::create_dir_all(&d).unwrap();
        img.salvar_png(&d.join(format!("{nome}.png"))).unwrap();
    }
}

fn distintas(img: &Imagem) -> usize {
    let mut cores = std::collections::BTreeSet::new();
    for y in (0..img.height).step_by(7) {
        for x in (0..img.width).step_by(7) {
            cores.insert(img.pixel(x, y));
        }
    }
    cores.len()
}

#[test]
fn telas_principais() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::com(
        Perfil::em(&dir.path().join("aluno")),
        Arc::new(NaHora),
        || 0,
    );
    let mut cap = Captura::new(Context::default(), vec2(1180.0, 800.0));

    let img = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "0_identidade");
    assert!(distintas(&img) > 3);

    app.salvar_identidade("Ana Lima", "2027001", "ana");
    let img = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "1_campanha");
    assert!(distintas(&img) > 3);

    app.abrir_missao(0);
    cap.size = vec2(1180.0, 1500.0);
    let img = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "2_missao_antes");
    {
        let m = app.missao.as_mut().unwrap();
        m.tentativa
            .previsoes
            .insert("gargalo".into(), RespostaPrevisao::Escolha(1));
        m.tentativa.decisoes.insert("furacao".into(), 2.0);
        m.tentativa.justificativas = vec![Some(0)];
        m.simular("2027001".into(), &NaHora, None);
    }
    cap.size = vec2(1180.0, 2300.0);
    let img = cap.imagem(3, |ui| app.draw(ui));
    // Move the animation forward so the 2D view has queues and moving items.
    if let Some(a) = app.missao.as_mut().and_then(|m| m.animacao.as_mut()) {
        a.ir_para(120.0);
        a.pausado = true;
    }
    let img2 = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "3_missao_resultado");
    salvar(&img2, "4_missao_vista2d");
    assert!(distintas(&img2) > 10);

    // The teacher.
    let turma = dir.path().join("turma");
    let prof = Perfil::em(&dir.path().join("prof"));
    let alunos = utsim_ensino_core::turma::importar_alunos(
        "nome;registro\nAna Lima;2027001\nBeto Souza;2027002",
    )
    .unwrap();
    criar_turma(
        &turma,
        "Simulação",
        "2027-1",
        "notas",
        &prof.chave_professor().unwrap().publica(),
        &alunos,
    )
    .unwrap();
    utsim_ensino_store::pasta::PastaTurma { dir: turma.clone() }
        .gravar_tentativa(
            &alunos[0],
            &Perfil::em(&dir.path().join("aluno"))
                .chave()
                .unwrap()
                .publica(),
            &Perfil::em(&dir.path().join("aluno")).diario()[0],
        )
        .unwrap();
    let mut p = App::com(prof, Arc::new(NaHora), || 0);
    p.salvar_identidade("Professor", "prof", "");
    p.config.professor = true;
    p.tela = Tela::Professor;
    p.professor.abrir(&turma).unwrap();
    let pacote = p.pacote.clone().unwrap();
    p.professor.corrigir(pacote, &NaHora, None);
    cap.size = vec2(1180.0, 1000.0);
    let img = cap.imagem(4, |ui| p.draw(ui));
    salvar(&img, "5_professor");
    assert!(distintas(&img) > 3);
}
