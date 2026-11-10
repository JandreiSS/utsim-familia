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

    // Reopening the mission brings back the locked prediction and the decisions.
    app.abrir_missao(0);
    {
        let m = app.missao.as_ref().unwrap();
        assert!(m.previsao_travada);
        assert!(m.previsoes_completas());
        assert_eq!(m.tentativa.decisoes["furacao"], 2.0);
    }

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

#[test]
fn construtor_de_casos_na_tela_do_professor() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let mut prof = App::com(Perfil::em(&dir.path().join("prof")), Arc::new(NaHora), || 0);
    prof.salvar_identidade("Professor", "prof", "");
    prof.config.professor = true;
    prof.tela = Tela::Professor;
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    prof.professor.caso_modelo = raiz.join("examples/mm1.ron").display().to_string();
    prof.professor.caso_titulo = "Fila clássica".into();
    let pasta = dir.path().join("caso");
    prof.professor.caso_pasta = pasta.display().to_string();
    prof.professor.criar_caso(&NaHora, None);
    frames(&mut prof, &ctx, 2);
    let r = prof.professor.caso_relatorio.clone().expect("relatório");
    assert!(r.contains("conferidor"), "{r}");
    assert!(pasta.join("pacote.ron").exists());
    prof.professor.conferir_caso(&NaHora, None);
    frames(&mut prof, &ctx, 2);
    assert!(
        prof.professor
            .caso_relatorio
            .as_ref()
            .unwrap()
            .contains("Fila clássica")
    );
}

/// A copy of the campaign under another id, as a teacher's package.
fn pacote_do_professor() -> utsim_ensino_core::pacote::Arquivos {
    let mut a = utsim_ensino_core::campanha::arquivos();
    let p = String::from_utf8(a["pacote.ron"].clone()).unwrap();
    let p = p.replacen(r#"id: "campanha""#, r#"id: "pacote_teste""#, 1);
    a.insert("pacote.ron".into(), p.into_bytes());
    a
}

#[test]
fn abre_outro_pacote_e_volta_a_campanha() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let mut app = app_em(dir.path());
    assert!(app.salvar_identidade("Ana Lima", "2027001", ""));
    assert!(!app.pacote_externo());

    // A .utse that does not exist: the campaign stays.
    let falta = dir.path().join("falta.utse");
    assert!(app.abrir_pacote(&falta).is_err());
    assert_eq!(app.pacote.as_ref().unwrap().pacote.id, "campanha");

    // A folder.
    let pasta = dir.path().join("pacote");
    for (k, v) in pacote_do_professor() {
        let f = pasta.join(&k);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, v).unwrap();
    }
    app.abrir_missao(0);
    app.abrir_pacote(&pasta).unwrap();
    assert!(app.pacote_externo());
    assert_eq!(app.pacote.as_ref().unwrap().pacote.id, "pacote_teste");
    assert_eq!(app.tela, Tela::Mapa);
    assert!(app.missao.is_none());
    frames(&mut app, &ctx, 2);

    // The package comes back when the app opens again.
    let mut de_novo = app_em(dir.path());
    assert!(de_novo.pacote_externo());
    assert_eq!(de_novo.pacote.as_ref().unwrap().pacote.id, "pacote_teste");
    de_novo.voltar_campanha();
    assert_eq!(de_novo.pacote.as_ref().unwrap().pacote.id, "campanha");
    let campanha = app_em(dir.path());
    assert!(!campanha.pacote_externo());

    // A .utse.
    let utse = dir.path().join("pacote.utse");
    utsim_ensino_store::arquivos::gravar_utse(&pacote_do_professor(), &utse).unwrap();
    let mut app = app_em(dir.path());
    app.abrir_pacote(&utse).unwrap();
    assert_eq!(app.pacote.as_ref().unwrap().pacote.id, "pacote_teste");

    // A package that is gone: back to the campaign with a message.
    std::fs::remove_file(&utse).unwrap();
    let app = app_em(dir.path());
    assert!(!app.pacote_externo());
    assert_eq!(app.pacote.as_ref().unwrap().pacote.id, "campanha");
    assert!(app.mensagem.as_deref().unwrap().contains("não abriu"));
}

#[test]
fn lista_os_pacotes_instalados() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let mut app = app_em(dir.path());
    assert!(app.salvar_identidade("Ana Lima", "2027001", ""));
    // As the launcher installs it: bin/utsim-ensino and pacotes/ side by side.
    let pacotes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ensino/pacotes");
    app.procurar_pacotes(&pacotes);
    let qs = app
        .pacotes_instalados
        .iter()
        .find(|p| p.id == "qualidade_seguranca")
        .expect("pacote de qualidade e segurança")
        .clone();
    frames(&mut app, &ctx, 2);
    app.abrir_pacote(&qs.caminho).unwrap();
    assert_eq!(
        app.pacote.as_ref().unwrap().pacote.id,
        "qualidade_seguranca"
    );
    frames(&mut app, &ctx, 2);
    // A folder with nothing readable lists nothing.
    app.procurar_pacotes(dir.path());
    assert!(app.pacotes_instalados.is_empty());
}

#[test]
fn prova_de_ponta_a_ponta() {
    use utsim_ensino_core::missao::Previsao;
    use utsim_ensino_core::prova::data_hora;
    let dir = tempfile::tempdir().unwrap();
    let turma_dir = dir.path().join("turma");
    let ctx = Context::default();
    let agora = 1_800_000_000_000;
    let prof_perfil = Perfil::em(&dir.path().join("prof"));
    let alunos =
        utsim_ensino_core::turma::importar_alunos("nome;registro\nAna;2027001\nBeto;2027002")
            .unwrap();
    criar_turma(
        &turma_dir,
        "Simulação",
        "2027-1",
        "notas",
        &prof_perfil.chave_professor().unwrap().publica(),
        &alunos,
    )
    .unwrap();

    // The teacher draws two missions of act 1, two runs each, and publishes.
    let mut prof = App::com(prof_perfil, Arc::new(NaHora), || 1_800_000_000_000);
    prof.salvar_identidade("Professor", "prof", "");
    prof.config.professor = true;
    prof.tela = Tela::Professor;
    prof.professor.abrir(&turma_dir).unwrap();
    {
        let p = &mut prof.professor.prova;
        p.id = "p1".into();
        p.inicio = data_hora(agora - 3_600_000, -180);
        p.fim = data_hora(agora + 3_600_000, -180);
        p.duracao_min = 60;
        p.tentativas = 2;
        p.semente = "5".into();
        p.por_ato.insert(1, 2);
    }
    let base = prof.pacote.clone().unwrap();
    let m = prof.professor.prova.montagem(&base, agora).unwrap();
    utsim_ensino_store::prova::publicar_prova(
        &utsim_ensino_store::pasta::PastaTurma {
            dir: turma_dir.clone(),
        },
        &prof.perfil,
        &prof.arquivos_do_pacote().unwrap(),
        &base,
        &m,
    )
    .unwrap();
    frames(&mut prof, &ctx, 2);

    // The student sees the exam in the class folder and opens it.
    let mut aluno = app_em(&dir.path().join("aluno"));
    aluno.salvar_identidade("Ana", "2027001", "ana");
    let c = utsim_ensino_store::arquivos::Convite::ler(&turma_dir.join("convite.utsc")).unwrap();
    aluno.config.turma = Some(utsim_ensino_store::perfil::Vinculo {
        convite: c,
        pasta: Some(turma_dir.clone()),
    });
    aluno.salvar_config();
    aluno.atualizar_pacotes();
    let p1 = aluno
        .pacotes_instalados
        .iter()
        .find(|p| p.id == "p1")
        .expect("prova na pasta da turma")
        .clone();
    aluno.abrir_pacote(&p1.caminho).unwrap();
    let pk = aluno.pacote.clone().unwrap();
    assert_eq!(pk.missoes.len(), 2);
    assert!(pk.missoes.iter().all(|m| m.missao.referencia.is_empty()));
    frames(&mut aluno, &ctx, 2);
    // Every mission of the exam is open, and the time counts from the first one opened.
    aluno.abrir_missao(1);
    assert_eq!(aluno.config.provas_abertas.get("p1"), Some(&agora));
    for _ in 0..2 {
        let m = aluno.missao.as_mut().unwrap();
        for pv in &m.missao().missao.previsao.clone() {
            let r = match pv {
                Previsao::Escolha { .. } => RespostaPrevisao::Escolha(0),
                _ => RespostaPrevisao::Valor(1.0),
            };
            m.tentativa.previsoes.insert(pv.id().to_string(), r);
        }
        m.simular("2027001".into(), &NaHora, None);
        frames(&mut aluno, &ctx, 2);
    }
    assert_eq!(aluno.missao.as_ref().unwrap().rodadas, 2);
    // A third run (a forged button) is ignored by the teacher.
    aluno
        .missao
        .as_mut()
        .unwrap()
        .simular("2027001".into(), &NaHora, None);
    frames(&mut aluno, &ctx, 2);
    let (pr, aberta) = aluno.prova().unwrap();
    assert!(pr.pode_rodar(agora, aberta).is_ok());
    assert!(pr.pode_rodar(agora + 61 * 60_000, aberta).is_err());

    // The teacher grades with the exam rebuilt from the seed.
    let pp = utsim_ensino_store::prova::pacote_da_prova(
        &prof.arquivos_do_pacote().unwrap(),
        &base,
        &utsim_ensino_store::prova::provas_salvas(&prof.perfil)[0],
    )
    .unwrap();
    assert_eq!(pp.hash, pk.hash, "o professor reconstrói o mesmo pacote");
    prof.professor.corrigir(Arc::new(pp), &NaHora, None);
    frames(&mut prof, &ctx, 3);
    let c = prof.professor.correcao.as_ref().expect("correção");
    assert_eq!(
        c.matriz.missoes,
        pk.missoes
            .iter()
            .map(|m| m.missao.id.clone())
            .collect::<Vec<_>>()
    );
    let ana = c
        .matriz
        .linhas
        .iter()
        .find(|l| l.aluno.registro == "2027001")
        .unwrap();
    assert!(
        ana.avisos.iter().any(|a| a.contains("além das 2")),
        "{:?}",
        ana.avisos
    );
    assert_eq!(ana.celulas[1].as_ref().unwrap().tentativas, 2);
    assert!(ana.celulas[0].is_none());
}

#[test]
fn desafio_e_equipes() {
    use utsim_ensino_core::desafio::{Desafio, comparar_equipes, ler_equipes};
    let dir = tempfile::tempdir().unwrap();
    let turma_dir = dir.path().join("turma");
    let ctx = Context::default();
    let prof_perfil = Perfil::em(&dir.path().join("prof"));
    let alunos = utsim_ensino_core::turma::importar_alunos(
        "nome;registro;apelido\nAna;2027001;raposa\nBeto;2027002;coruja\nCaio;2027003;lontra",
    )
    .unwrap();
    criar_turma(
        &turma_dir,
        "Simulação",
        "2027-1",
        "notas",
        &prof_perfil.chave_professor().unwrap().publica(),
        &alunos,
    )
    .unwrap();
    let c = utsim_ensino_store::arquivos::Convite::ler(&turma_dir.join("convite.utsc")).unwrap();
    // Ana and Beto find the reference of 1.1, Caio keeps the start.
    let mut caio_app = None;
    for (reg, nome, boa) in [
        ("2027001", "Ana", true),
        ("2027002", "Beto", true),
        ("2027003", "Caio", false),
    ] {
        let mut a = app_em(&dir.path().join(reg));
        a.salvar_identidade(nome, reg, "");
        a.config.turma = Some(utsim_ensino_store::perfil::Vinculo {
            convite: c.clone(),
            pasta: Some(turma_dir.clone()),
        });
        a.salvar_config();
        a.abrir_missao(0);
        let m = a.missao.as_mut().unwrap();
        let mc = m.missao().missao.clone();
        if boa {
            m.tentativa.decisoes = mc.completar(&mc.referencia);
        }
        m.tentativa
            .previsoes
            .insert("gargalo".into(), RespostaPrevisao::Escolha(1));
        m.simular(reg.into(), &NaHora, None);
        frames(&mut a, &ctx, 1);
        if reg == "2027003" {
            caio_app = Some(a);
        }
    }

    let mut prof = App::com(prof_perfil, Arc::new(NaHora), || 0);
    prof.salvar_identidade("Professor", "prof", "");
    prof.config.professor = true;
    prof.tela = Tela::Professor;
    prof.professor.abrir(&turma_dir).unwrap();
    let pacote = prof.pacote.clone().unwrap();
    prof.professor.corrigir(pacote.clone(), &NaHora, None);
    frames(&mut prof, &ctx, 3);
    let c = prof.professor.correcao.clone().unwrap();
    let missao = pacote.missoes[0].missao.id.clone();
    let linhas = utsim_ensino_store::desafio::placar_do_desafio(
        &c,
        &alunos,
        &Desafio {
            missao: missao.clone(),
            fim: None,
        },
        utsim_ensino_core::pacote::Registro::Melhor,
    );
    let v: Vec<(usize, bool, &str)> = linhas
        .iter()
        .map(|l| (l.posicao, l.empate, l.apelido.as_str()))
        .collect();
    assert_eq!(v[2], (3, false, "lontra"), "{linhas:#?}");
    assert_eq!(
        (v[0].0, v[0].1, v[1].0, v[1].1),
        (1, true, 1, true),
        "{linhas:#?}"
    );
    // A deadline before every attempt leaves the scoreboard empty.
    let antes = utsim_ensino_store::desafio::placar_do_desafio(
        &c,
        &alunos,
        &Desafio {
            missao: missao.clone(),
            fim: Some(0),
        },
        utsim_ensino_core::pacote::Registro::Melhor,
    );
    assert!(antes.is_empty());
    utsim_ensino_store::desafio::gravar_placar(
        &utsim_ensino_store::pasta::PastaTurma {
            dir: turma_dir.clone(),
        },
        &missao,
        &linhas,
    )
    .unwrap();
    let csv = &utsim_ensino_store::desafio::placares(&turma_dir)[0].1;
    assert!(csv.contains("raposa") && !csv.contains("Ana") && !csv.contains("2027001"));

    // Teams: Ana's solution against Caio's.
    let eqs = ler_equipes("Azul: 2027001\nVerde: 2027003").unwrap();
    let (sol, sem) = utsim_ensino_store::desafio::solucoes_das_equipes(
        &c,
        &eqs,
        &missao,
        utsim_ensino_core::pacote::Registro::Melhor,
    );
    assert!(sem.is_empty());
    let k = comparar_equipes(&pacote.missoes[0], &sol, &|_| true).unwrap();
    assert_eq!(k.posicoes, [1, 2], "{k:#?}");
    prof.professor.desafio.placar = Some(linhas);
    prof.professor.desafio.comparacao = Some(k);
    frames(&mut prof, &ctx, 2);

    // The student sees the scoreboard in the class tab.
    let mut caio = caio_app.unwrap();
    caio.tela = Tela::Turma;
    frames(&mut caio, &ctx, 2);

    // Pictures for review with `UTSIM_CAPTURAS=<pasta>`.
    if let Some(d) = std::env::var_os("UTSIM_CAPTURAS") {
        let d = std::path::PathBuf::from(d);
        std::fs::create_dir_all(&d).unwrap();
        let mut cap =
            utsim_ui_tema::captura::Captura::new(Context::default(), egui::vec2(1180.0, 2400.0));
        let img = cap.imagem(4, |ui| prof.draw(ui));
        img.salvar_png(&d.join("c0_desafio_professor.png")).unwrap();
        cap.size = egui::vec2(1180.0, 900.0);
        let img = cap.imagem(4, |ui| caio.draw(ui));
        img.salvar_png(&d.join("c1_placar_aluno.png")).unwrap();
    }
}

/// EN11: the 3D client hosts the same `App` with the thread executor (`App::new`, as the
/// bridge's `UtsimEnsino` does); a mission run there gets the same grade as in the window
/// tests, run inline.
#[test]
fn mesma_nota_nos_dois_hospedeiros() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let jogar = |app: &mut App| {
        app.salvar_identidade("Ana Lima", "2027001", "");
        app.abrir_missao(0);
        let m = app.missao.as_mut().unwrap();
        let mc = m.missao().missao.clone();
        m.tentativa.decisoes = mc.completar(&mc.referencia);
        m.tentativa
            .previsoes
            .insert("gargalo".into(), RespostaPrevisao::Escolha(1));
        m.tentativa.justificativas = vec![Some(0)];
    };
    let mut janela = app_em(&dir.path().join("janela"));
    jogar(&mut janela);
    janela
        .missao
        .as_mut()
        .unwrap()
        .simular("2027001".into(), &NaHora, None);
    frames(&mut janela, &ctx, 2);
    let nota_janela = janela
        .missao
        .as_ref()
        .unwrap()
        .resultado
        .as_ref()
        .unwrap()
        .nota;

    let mut ponte = App::new(Perfil::em(&dir.path().join("ponte")));
    jogar(&mut ponte);
    let ex = ponte.executor.clone();
    ponte
        .missao
        .as_mut()
        .unwrap()
        .simular("2027001".into(), ex.as_ref(), None);
    let inicio = std::time::Instant::now();
    while ponte.missao.as_ref().unwrap().resultado.is_none() {
        assert!(inicio.elapsed().as_secs() < 120, "a rodada não terminou");
        std::thread::sleep(std::time::Duration::from_millis(50));
        frames(&mut ponte, &ctx, 1);
    }
    let nota_ponte = ponte
        .missao
        .as_ref()
        .unwrap()
        .resultado
        .as_ref()
        .unwrap()
        .nota;
    assert_eq!(nota_janela, nota_ponte);
}

#[test]
fn pacote_com_requisito_ausente_abre_com_aviso() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let mut app = app_em(dir.path());
    assert!(app.salvar_identidade("Ana Lima", "2027001", ""));
    let mut a = pacote_do_professor();
    let p = String::from_utf8(a["pacote.ron"].clone()).unwrap();
    let p = p.replacen(
        r#"requer: ["nucleo >= 0.1"]"#,
        r#"requer: ["nucleo >= 0.1", "linha:mineracao"]"#,
        1,
    );
    a.insert("pacote.ron".into(), p.into_bytes());
    let raiz = dir.path().join("pacotes");
    let pasta = raiz.join("novo");
    for (k, v) in a {
        let f = pasta.join(&k);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, v).unwrap();
    }
    // The list shows the warning before it opens, and it opens all the same.
    app.procurar_pacotes(&raiz);
    let achado = app.pacotes_instalados[0].clone();
    assert_eq!(achado.faltando.len(), 1);
    assert_eq!(achado.faltando[0].requisito, "linha:mineracao");
    frames(&mut app, &ctx, 2);
    app.abrir_pacote(&pasta).unwrap();
    assert_eq!(app.pacote.as_ref().unwrap().faltando.len(), 1);
    frames(&mut app, &ctx, 2);
    app.abrir_missao(0);
    frames(&mut app, &ctx, 2);
}

#[test]
fn pacote_de_estatistica_e_qualidade_com_o_botao_de_analise() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let mut app = app_em(dir.path());
    assert!(app.salvar_identidade("Ana Lima", "2027001", ""));
    let pacotes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ensino/pacotes");
    app.procurar_pacotes(&pacotes);
    let eq = app
        .pacotes_instalados
        .iter()
        .find(|p| p.id == "estatistica_qualidade")
        .expect("pacote de estatística e qualidade")
        .clone();
    assert!(eq.faltando.is_empty());
    app.abrir_pacote(&eq.caminho).unwrap();
    // The apps as if the launcher had installed them: the buttons are drawn.
    app.apps_analise = vec!["estatistica".into(), "qualidade".into()];
    for i in 0..6 {
        app.abrir_missao(i);
        frames(&mut app, &ctx, 2);
    }
}

#[test]
fn editor_de_missoes_do_professor() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    let mut prof = App::com(Perfil::em(&dir.path().join("prof")), Arc::new(NaHora), || 0);
    prof.salvar_identidade("Professor", "prof", "");
    prof.config.professor = true;
    prof.tela = Tela::Editor;
    frames(&mut prof, &ctx, 1);
    let pasta = dir.path().join("Meu pacote");
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let e = &mut prof.editor;
    e.pasta = pasta.display().to_string();
    e.abrir_pasta();
    assert!(e.erros.is_empty(), "{:?}", e.erros);
    e.modelo = raiz
        .join("examples/tutorial_02_linha.ron")
        .display()
        .to_string();
    e.titulo_novo = "Linha do tutorial".into();
    e.nova_missao();
    let ed = e.edicao.as_mut().expect("rascunho");
    assert_eq!(ed.base.id, "prof/linha_do_tutorial");
    for a in ed.alavancas.iter_mut() {
        a.usar = a.alavanca.id() != "corte_capacidade";
        if let utsim_ensino_core::missao::Alavanca::Quantidade { ate, .. } = &mut a.alavanca {
            *ate -= 1;
        }
    }
    ed.base.orcamento = Some(10000.0);
    frames(&mut prof, &ctx, 2);
    // Stage B: measurements, one prediction of each editable kind, a justification, a
    // hint and a concept are drawn (and taken out again before the calibration).
    {
        use utsim_ensino_core::missao::{
            Calculo, Medicao, OpcaoPrevisao, Pergunta, Previsao, Resposta, Termo, Tolerancia,
        };
        let termo = |c: &str| Termo {
            chave: c.into(),
            fator: 1.0,
        };
        let ed = prof.editor.edicao.as_mut().unwrap();
        ed.base.medicoes.push(Medicao {
            rotulo: "Custo".into(),
            valor: vec![termo("custo")],
            unidade: "R$".into(),
            casas: 0,
        });
        ed.base.previsao = vec![
            Previsao::Escolha {
                id: "p1".into(),
                pergunta: "Qual?".into(),
                opcoes: vec![
                    OpcaoPrevisao {
                        rotulo: "A".into(),
                        valor: vec![termo("custo")],
                    },
                    OpcaoPrevisao {
                        rotulo: "B".into(),
                        valor: vec![termo("custo"), termo("custo")],
                    },
                ],
                resposta: Resposta::Maior,
            },
            Previsao::Valor {
                id: "p2".into(),
                pergunta: "Quanto?".into(),
                valor: vec![termo("custo")],
                unidade: "R$".into(),
                tolerancia: Tolerancia::Relativa(0.05),
            },
            Previsao::Calculo {
                id: "p3".into(),
                pergunta: "Quantas?".into(),
                calculo: Calculo::TempoNormal {
                    tempos: vec![1.0, 1.2],
                    ritmo: 1.1,
                },
                unidade: "min".into(),
                tolerancia: Tolerancia::Absoluta(0.1),
            },
        ];
        ed.base.justificativa = vec![Pergunta {
            pergunta: "Por quê?".into(),
            opcoes: vec!["Porque sim".into(), "Porque não".into()],
            certa: 1,
        }];
        ed.base.dicas = vec!["Pense no gargalo".into()];
        ed.base.conceitos = vec!["gargalo".into(), "conceito_fora_do_glossario".into()];
        prof.editor.previa = true;
        frames(&mut prof, &ctx, 3);
        assert!(prof.editor.glossario.is_some());
        assert!(!prof.editor.chaves.is_empty());
        let ed = prof.editor.edicao.as_mut().unwrap();
        ed.base.medicoes.clear();
        ed.base.previsao.clear();
        ed.base.justificativa.clear();
        ed.base.dicas.clear();
        ed.base.conceitos.clear();
        prof.editor.previa = false;
        frames(&mut prof, &ctx, 2);
    }
    prof.editor.calibrar(&NaHora, None);
    frames(&mut prof, &ctx, 2);
    let c = prof.editor.calibragem.clone().expect("calibragem");
    assert_eq!(c.conferencia.melhor.get("furacao_capacidade"), Some(&2.0));
    prof.editor.aplicar();
    prof.editor.gravar_missao();
    assert!(prof.editor.erros.is_empty(), "{:?}", prof.editor.erros);
    frames(&mut prof, &ctx, 2);
    // On disk, a package the student's app opens.
    assert!(pasta.join("pacote.ron").is_file());
    let a = utsim_ensino_store::arquivos::ler_pacote(&pasta).unwrap();
    let p = utsim_ensino_core::pacote::carregar(&a).unwrap();
    assert_eq!(p.missoes.len(), 1);
    assert_eq!(
        p.missoes[0].missao.referencia.get("furacao_capacidade"),
        Some(&2.0)
    );
    // Opened again from the folder, the mission is listed and opens.
    let mut outro = App::com(
        Perfil::em(&dir.path().join("prof2")),
        Arc::new(NaHora),
        || 0,
    );
    outro.config.professor = true;
    outro.tela = Tela::Editor;
    outro.editor.pasta = pasta.display().to_string();
    outro.editor.abrir_pasta();
    let (_, pp, _) = outro.editor.pacote.as_ref().unwrap();
    let lista = pp.missoes();
    assert_eq!(lista.len(), 1);
    assert!(pp.abrir(&lista[0].caminho).is_ok());
    frames(&mut outro, &ctx, 2);
}
