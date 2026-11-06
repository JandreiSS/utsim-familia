//! The screens drawn without a window (egui's headless context): identity, map, mission
//! 1.1 solved through the screen state, the 2D view coherent with the report, sandbox,
//! class and the teacher's batch grading.

use std::sync::Arc;

use egui::{Context, RawInput};
use utsim_core::State;
use utsim_ensino_core::nota::RespostaPrevisao;
use utsim_ensino_store::perfil::Perfil;
use utsim_ensino_ui::turma::criar_turma;
use utsim_ensino_ui::{App, NaHora, Tela};

fn frames(app: &mut App, ctx: &Context, n: usize) {
    for _ in 0..n {
        let mut out = ctx.run_ui(RawInput::default(), |ui| app.draw(ui));
        out.textures_delta.clear();
    }
}

fn app_em(dir: &std::path::Path) -> App {
    App::com(Perfil::em(dir), Arc::new(NaHora), || 1_800_000_000_000)
}

#[test]
fn aluno_resolve_a_missao_1_1() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let mut app = app_em(dir.path());
    assert!(app.erro_pacote.is_empty(), "{:?}", app.erro_pacote);
    assert_eq!(app.tela, Tela::Identidade);
    frames(&mut app, &ctx, 2);
    assert!(!app.salvar_identidade("A", "x y", ""));
    assert!(app.salvar_identidade("Ana Lima", "2027001", ""));
    assert_eq!(app.tela, Tela::Mapa);
    frames(&mut app, &ctx, 2);

    app.abrir_missao(0);
    assert_eq!(app.tela, Tela::Missao);
    frames(&mut app, &ctx, 2);
    {
        let m = app.missao.as_mut().unwrap();
        assert!(!m.previsoes_completas());
        m.tentativa
            .previsoes
            .insert("gargalo".into(), RespostaPrevisao::Escolha(1));
        m.tentativa.decisoes.insert("furacao".into(), 2.0);
        m.tentativa.justificativas = vec![Some(0)];
        m.simular("2027001".into(), &NaHora, None);
    }
    frames(&mut app, &ctx, 3);
    let m = app.missao.as_ref().unwrap();
    let r = m.resultado.as_ref().expect("resultado");
    assert!(r.resolvida, "{r:#?}");
    assert_eq!(r.estrelas, 3);
    assert!(m.previsao_travada);
    assert!(app.progresso.resolvida("ato1/furadeira"));
    // Progress and the signed journal are on disk.
    assert!(
        Perfil::em(dir.path())
            .progresso("campanha")
            .resolvida("ato1/furadeira")
    );
    assert_eq!(Perfil::em(dir.path()).diario().len(), 1);

    // The 2D view: states and queue contents coherent with the replay's own report.
    let mut a =
        utsim_ensino_ui::vista2d::Animacao::new(m.animacao.as_ref().unwrap().replay.clone(), "min");
    a.ir_para(a.replay.stop_time);
    let rep = &a.replay.report;
    for (i, b) in a.replay.blocks.iter().enumerate() {
        if let Some(br) = rep.block(&b.name) {
            assert_eq!(a.conteudo[i], br.content_now, "{}", b.name);
        }
    }
    let furacao = a
        .replay
        .blocks
        .iter()
        .position(|b| b.name == "furacao")
        .unwrap();
    assert!(matches!(
        a.estados[furacao],
        State::Idle | State::Processing
    ));

    // The next mission is unlocked; the sandbox runs.
    app.abrir_areia(0);
    {
        let s = app.areia.as_mut().unwrap();
        s.b.insert("furacao".into(), 2.0);
        s.comparar("2027001".into(), &NaHora, None);
    }
    frames(&mut app, &ctx, 3);
    assert_eq!(
        app.areia
            .as_ref()
            .unwrap()
            .resultado
            .as_ref()
            .unwrap()
            .len(),
        2
    );

    for t in [Tela::Glossario, Tela::Turma, Tela::Ajustes, Tela::Mapa] {
        app.tela = t;
        frames(&mut app, &ctx, 2);
    }
}

#[test]
fn professor_cria_turma_e_corrige() {
    let dir = tempfile::tempdir().unwrap();
    let turma_dir = dir.path().join("turma");
    let ctx = Context::default();

    // A student plays and saves to the class folder.
    let mut aluno = app_em(&dir.path().join("aluno"));
    let prof_perfil = Perfil::em(&dir.path().join("prof"));
    let alunos =
        utsim_ensino_core::turma::importar_alunos("nome;registro\nAna;2027001\nBeto;2027002")
            .unwrap();
    criar_turma(
        &turma_dir,
        "Simulação",
        "2027-1",
        "notas da disciplina",
        &prof_perfil.chave_professor().unwrap().publica(),
        &alunos,
    )
    .unwrap();
    assert!(turma_dir.join("convite.utsc").exists());
    assert!(turma_dir.join("pacotes").join("campanha.utse").exists());
    aluno.salvar_identidade("Ana", "2027001", "ana");
    aluno.turma.convite = turma_dir.join("convite.utsc").display().to_string();
    aluno.turma.pasta = turma_dir.display().to_string();
    aluno.tela = Tela::Turma;
    frames(&mut aluno, &ctx, 1);
    // Join through the same path the button uses.
    let c = utsim_ensino_store::arquivos::Convite::ler(&turma_dir.join("convite.utsc")).unwrap();
    aluno.config.turma = Some(utsim_ensino_store::perfil::Vinculo {
        convite: c,
        pasta: Some(turma_dir.clone()),
    });
    aluno.salvar_config();
    aluno.abrir_missao(0);
    {
        let m = aluno.missao.as_mut().unwrap();
        m.tentativa
            .previsoes
            .insert("gargalo".into(), RespostaPrevisao::Escolha(0));
        m.tentativa.decisoes.insert("corte".into(), 2.0);
        m.simular("2027001".into(), &NaHora, None);
    }
    frames(&mut aluno, &ctx, 2);
    assert!(
        turma_dir
            .join("entregas")
            .join("2027001")
            .join("aluno.json")
            .exists()
    );

    // The teacher grades.
    let mut prof = App::com(prof_perfil, Arc::new(NaHora), || 0);
    prof.salvar_identidade("Professor", "prof", "");
    prof.config.professor = true;
    prof.tela = Tela::Professor;
    prof.professor.abrir(&turma_dir).unwrap();
    let pacote = prof.pacote.clone().unwrap();
    prof.professor.corrigir(pacote, &NaHora, None);
    frames(&mut prof, &ctx, 3);
    let c = prof.professor.correcao.as_ref().expect("correção");
    assert_eq!(c.matriz.linhas.len(), 2);
    let ana = c
        .matriz
        .linhas
        .iter()
        .find(|l| l.aluno.registro == "2027001")
        .unwrap();
    let cel = ana.celulas[0].as_ref().unwrap();
    assert!(!cel.resolvida);
    assert_eq!(cel.tentativas, 1);
    let beto = c
        .matriz
        .linhas
        .iter()
        .find(|l| l.aluno.registro == "2027002")
        .unwrap();
    assert!(beto.celulas.iter().all(Option::is_none));
}
