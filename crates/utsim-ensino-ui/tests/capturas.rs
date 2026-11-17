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

#[test]
fn telas_das_pecas() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::com(Perfil::em(dir.path()), Arc::new(NaHora), || 0);
    app.salvar_identidade("Ana Lima", "2027001", "ana");
    let mut cap = Captura::new(Context::default(), vec2(1180.0, 1700.0));
    for (id, nome) in [
        ("ato4/ventilador", "6_balanceamento"),
        ("ato5/oficina", "7_arranjo"),
        ("ato4/mix", "8_mix"),
        ("ato2/urgente", "9_urgente"),
    ] {
        let p = app.pacote.clone().unwrap();
        let i = p.indice(id).unwrap();
        app.abrir_missao(i);
        let img = cap.imagem(3, |ui| app.draw(ui));
        salvar(&img, &format!("{nome}_antes"));
        assert!(distintas(&img) > 10);
        {
            let m = app.missao.as_mut().unwrap();
            let r = m.missao().missao.completar(&m.missao().missao.referencia);
            m.tentativa.decisoes = r;
            let ids: Vec<String> = m
                .missao()
                .missao
                .previsao
                .iter()
                .map(|p| p.id().to_string())
                .collect();
            for id in ids {
                m.tentativa
                    .previsoes
                    .insert(id, RespostaPrevisao::Escolha(0));
            }
            m.simular("2027001".into(), &NaHora, None);
        }
        let img = cap.imagem(3, |ui| app.draw(ui));
        salvar(&img, &format!("{nome}_resultado"));
        let r = app
            .missao
            .as_ref()
            .unwrap()
            .resultado
            .as_ref()
            .expect("resultado");
        assert!(r.resolvida, "{id}: {r:#?}");
    }
}

#[test]
fn tela_da_modelagem() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::com(Perfil::em(dir.path()), Arc::new(NaHora), || 0);
    app.salvar_identidade("Ana Lima", "2027001", "ana");
    let mut cap = Captura::new(Context::default(), vec2(1180.0, 2000.0));
    let p = app.pacote.clone().unwrap();
    app.abrir_missao(p.indice("ato7/padaria").unwrap());
    {
        let m = app.missao.as_mut().unwrap();
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../ensino/campanha/modelos/padaria_gabarito.ron");
        // A chamber of 4 places instead of 10: the capacity criterion fails.
        let txt = std::fs::read_to_string(caminho)
            .unwrap()
            .replace("capacity: 10,", "capacity: 4,");
        m.tentativa.modelo = Some(txt);
        m.tentativa
            .previsoes
            .insert("gargalo".into(), RespostaPrevisao::Escolha(1));
        m.simular("2027001".into(), &NaHora, None);
    }
    let img = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "8_modelagem");
    let r = app
        .missao
        .as_ref()
        .unwrap()
        .resultado
        .as_ref()
        .expect("resultado");
    assert!(!r.resolvida);
    assert!(
        r.criterios
            .iter()
            .any(|c| !c.ok && c.rotulo.contains("câmara")),
        "{:#?}",
        r.criterios
    );
}

#[test]
fn tela_do_caso_final() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::com(Perfil::em(dir.path()), Arc::new(NaHora), || 0);
    app.salvar_identidade("Ana Lima", "2027001", "ana");
    let mut cap = Captura::new(Context::default(), vec2(1180.0, 1500.0));
    let p = app.pacote.clone().unwrap();
    app.abrir_missao(p.indice("ato8/medir").unwrap());
    let img = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "9_caso_final");
    assert!(distintas(&img) > 10);
}

#[test]
fn telas_do_pacote_de_qualidade_e_seguranca() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::com(Perfil::em(dir.path()), Arc::new(NaHora), || 0);
    app.salvar_identidade("Ana Lima", "2027001", "ana");
    let pasta = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ensino/pacotes/qualidade_seguranca");
    app.abrir_pacote(&pasta).unwrap();
    let mut cap = Captura::new(Context::default(), vec2(1180.0, 800.0));
    let img = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "a0_pacote_qs");
    assert!(distintas(&img) > 3);
    let p = app.pacote.clone().unwrap();
    cap.size = vec2(1180.0, 2300.0);
    for (i, m) in p.missoes.iter().enumerate() {
        app.abrir_missao(i);
        {
            let e = app.missao.as_mut().unwrap();
            e.tentativa.decisoes = m.missao.completar(&m.missao.referencia);
            e.simular("2027001".into(), &NaHora, None);
        }
        let _ = cap.imagem(3, |ui| app.draw(ui));
        if let Some(a) = app.missao.as_mut().and_then(|m| m.animacao.as_mut()) {
            a.ir_para(120.0);
            a.pausado = true;
        }
        let img = cap.imagem(3, |ui| app.draw(ui));
        let nome = m.missao.id.replace('/', "_");
        salvar(&img, &format!("a{}_{nome}", i + 1));
        assert!(distintas(&img) > 10, "{nome}");
    }
}

#[test]
fn tela_da_prova() {
    use utsim_ensino_core::campanha::{arquivos, carregar_campanha};
    use utsim_ensino_core::prova::{Montagem, Prova, Regra, montar};
    let dir = tempfile::tempdir().unwrap();
    let base = carregar_campanha().unwrap();
    let agora = 1_800_000_000_000;
    let m = Montagem {
        id: "prova1".into(),
        titulo: "Prova 1".into(),
        pacote_base: base.pacote.id.clone(),
        hash_base: base.hash.clone(),
        regras: vec![Regra {
            ato: Some(2),
            conceito: None,
            quantidade: 2,
        }],
        prova: Prova {
            semente: 3,
            inicio: agora - 600_000,
            fim: agora + 7_200_000,
            duracao_min: 50,
            tentativas: 2,
            fuso_min: -180,
        },
    };
    let pm = montar(&arquivos(), &base, &m).unwrap();
    let utse = dir.path().join("prova1.utse");
    utsim_ensino_store::arquivos::gravar_utse(&pm.aluno, &utse).unwrap();
    let mut app = App::com(
        Perfil::em(&dir.path().join("aluno")),
        Arc::new(NaHora),
        || 1_800_000_000_000,
    );
    app.salvar_identidade("Ana Lima", "2027001", "ana");
    app.abrir_pacote(&utse).unwrap();
    let mut cap = Captura::new(Context::default(), vec2(1180.0, 700.0));
    let img = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "b0_prova_mapa");
    app.abrir_missao(0);
    {
        let e = app.missao.as_mut().unwrap();
        for pv in &e.missao().missao.previsao.clone() {
            let r = match pv {
                utsim_ensino_core::missao::Previsao::Escolha { .. } => RespostaPrevisao::Escolha(0),
                _ => RespostaPrevisao::Valor(1.0),
            };
            e.tentativa.previsoes.insert(pv.id().to_string(), r);
        }
        e.simular("2027001".into(), &NaHora, None);
    }
    cap.size = vec2(1180.0, 2000.0);
    let _ = cap.imagem(3, |ui| app.draw(ui));
    let img = cap.imagem(3, |ui| app.draw(ui));
    salvar(&img, "b1_prova_missao");
    assert!(distintas(&img) > 10);
}
