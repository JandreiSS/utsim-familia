//! The Lean Six Sigma case (EN8): each student gets data of their own, the planted cause
//! is recovered by the reduced tools of the game and, through the training project, by the
//! analyses the Seis Sigma app runs on the exported files, with the same numbers.

use utsim_ensino_core::campanha::carregar_campanha;
use utsim_ensino_core::dados::Gerador;
use utsim_ensino_core::missao::{Previsao, Resposta};
use utsim_ensino_core::nota::{RespostaPrevisao, Tentativa, corrigir};
use utsim_ensino_core::pacote::MissaoCarregada;
use utsim_ensino_core::questoes::QuestaoDados;
use utsim_lss_core::dados::datasets;
use utsim_lss_store::ProjectFolder;
use utsim_lss_store::user::Identity;

#[test]
fn causa_plantada_por_aluno() {
    let p = carregar_campanha().unwrap();
    let m = p.missao("ato8/analisar").unwrap();
    let cj = m.missao.conjunto("estratos").unwrap();
    let mut afetados = std::collections::BTreeSet::new();
    for aluno in [
        "ra001", "ra002", "ra003", "ra004", "ra005", "ra006", "ra007", "ra008",
    ] {
        let receita = cj.receita(&m.missao.id, aluno);
        let Gerador::MediaDeslocada { afetado, .. } = receita.gerador else {
            panic!("gerador")
        };
        afetados.insert(afetado);
        // The game finds the shifted group the recipe planted.
        let r = QuestaoDados::GrupoDeslocado
            .resposta(&receita.gerar())
            .unwrap();
        assert_eq!(r as usize, afetado, "{aluno}");
    }
    assert!(
        afetados.len() >= 2,
        "o efeito precisa mudar de aluno para aluno"
    );
}

fn respostas_certas(m: &MissaoCarregada, aluno: &str) -> Tentativa {
    let mut t = Tentativa {
        aluno: aluno.into(),
        justificativas: m
            .missao
            .justificativa
            .iter()
            .map(|q| Some(q.certa))
            .collect(),
        ..Default::default()
    };
    for pv in &m.missao.previsao {
        match pv {
            Previsao::Dados {
                id,
                conjunto,
                questao,
                ..
            } => {
                let c = m
                    .missao
                    .conjunto(conjunto)
                    .unwrap()
                    .gerar(&m.missao.id, aluno);
                let v = questao.resposta(&c).unwrap();
                let r = if questao.e_escolha() {
                    RespostaPrevisao::Escolha(v as usize)
                } else {
                    RespostaPrevisao::Valor(v)
                };
                t.previsoes.insert(id.clone(), r);
            }
            Previsao::Escolha {
                id,
                resposta: Resposta::Fixa(i),
                ..
            } => {
                t.previsoes
                    .insert(id.clone(), RespostaPrevisao::Escolha(*i));
            }
            _ => {}
        }
    }
    t
}

#[test]
fn respostas_certas_resolvem_as_fases() {
    let p = carregar_campanha().unwrap();
    for id in [
        "ato8/definir",
        "ato8/medir",
        "ato8/analisar",
        "ato8/controlar",
    ] {
        let m = p.missao(id).unwrap();
        let sim =
            utsim_ensino_core::motor::simular(m, "ra042", &Default::default(), &|_| true).unwrap();
        let t = respostas_certas(m, "ra042");
        let r = corrigir(&m.missao, &p.pacote.pesos, &sim, &t).unwrap();
        assert!(r.resolvida && r.nota == 100.0, "{id}: {r:#?}");
    }
    // A colleague's right answers do not solve this student's data questions.
    let m = p.missao("ato8/medir").unwrap();
    let sim =
        utsim_ensino_core::motor::simular(m, "ra042", &Default::default(), &|_| true).unwrap();
    let mut colega = respostas_certas(m, "ra043");
    colega.aluno = "ra042".into();
    let r = corrigir(&m.missao, &p.pacote.pesos, &sim, &colega).unwrap();
    assert!(r.nota < 100.0, "{r:#?}");
}

#[test]
fn caminho_pelo_seis_sigma_da_os_mesmos_numeros() {
    let p = carregar_campanha().unwrap();
    let dir = tempfile::tempdir().unwrap();
    for id in ["ato8/medir", "ato8/analisar"] {
        let m = p.missao(id).unwrap();
        let caminho =
            utsim_ensino_store::seis_sigma::exportar(dir.path(), m, "ra042", "Ana Lima").unwrap();
        assert!(
            caminho
                .join("modelos")
                .join("modelo_da_fabrica.ron")
                .exists()
        );
        // Open the training project as the Seis Sigma app does.
        let me = Identity {
            person: utsim_lss_core::Id::v7(0, [1; 10]),
            name: "Professor".into(),
            email: String::new(),
        };
        let pf = ProjectFolder::open_full(&caminho, &me).unwrap();
        let ds = datasets(pf.state(), pf.project_id());
        assert_eq!(ds.len(), m.missao.dados.len());
        for cj in &m.missao.dados {
            let d = ds.iter().find(|d| d.name == cj.nome).unwrap();
            assert_eq!(d.origin, utsim_lss_core::dados::DataOrigin::Simulated);
            let texto = pf.read_data(&d.file).unwrap();
            let tabela = utsim_quality::data::parse(&texto);
            let jogo = cj.gerar(&m.missao.id, "ra042");
            match &cj.gerador {
                Gerador::Msa { .. } => {
                    let ms = utsim_quality::data::gage(&tabela).unwrap();
                    let data = utsim_quality::msa::GageData::from_measurements(&ms).unwrap();
                    let rr = utsim_quality::msa::gage_anova(&data, &Default::default()).unwrap();
                    let seis_sigma = rr
                        .components
                        .iter()
                        .find(|k| k.name == utsim_quality::msa::GRR)
                        .unwrap()
                        .pct_study_var;
                    let no_jogo = QuestaoDados::RrPercentual.resposta(&jogo).unwrap();
                    assert!(
                        (seis_sigma - no_jogo).abs() < 1e-9,
                        "{seis_sigma} {no_jogo}"
                    );
                }
                Gerador::MediaDeslocada { .. } => {
                    // Means per group from the exported file.
                    let mut soma: std::collections::BTreeMap<String, (f64, f64)> =
                        Default::default();
                    for row in &tabela.rows {
                        let v = utsim_quality::data::number(&row[1], tabela.decimal_comma).unwrap();
                        let e = soma.entry(row[0].clone()).or_default();
                        e.0 += v;
                        e.1 += 1.0;
                    }
                    let medias: Vec<(String, f64)> =
                        soma.into_iter().map(|(k, (s, n))| (k, s / n)).collect();
                    let geral = medias.iter().map(|x| x.1).sum::<f64>() / medias.len() as f64;
                    let maior = medias
                        .iter()
                        .max_by(|a, b| (a.1 - geral).abs().total_cmp(&(b.1 - geral).abs()))
                        .unwrap()
                        .0
                        .clone();
                    let i = QuestaoDados::GrupoDeslocado.resposta(&jogo).unwrap() as usize;
                    assert_eq!(maior, cj.grupos()[i]);
                }
                _ => {}
            }
        }
    }
}

#[test]
fn missoes_resolvidas_viram_pratica_no_programa() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use utsim_ensino_core::progresso::{Progresso, RegistroMissao};
    use utsim_ensino_store::seis_sigma::registrar_competencias;
    use utsim_lss_core::mentoria;
    use utsim_lss_store::Clock;

    struct Relogio(AtomicU64, u8);
    impl Clock for Relogio {
        fn now_ms(&self) -> i64 {
            self.0.fetch_add(1, Ordering::Relaxed) as i64
        }
        fn random(&self) -> [u8; 10] {
            let n = self.0.load(Ordering::Relaxed);
            let mut r = [self.1; 10];
            r[2..].copy_from_slice(&n.to_be_bytes());
            r
        }
    }
    let pessoa = |n: u8, nome: &str| Identity {
        person: utsim_lss_core::Id::v7(1_700_000_000_000, [n; 10]),
        name: nome.into(),
        email: format!("{n}@empresa.com"),
    };
    let (mbb, ana, beto) = (pessoa(1, "MBB"), pessoa(2, "Ana"), pessoa(3, "Beto"));
    let (c1, c2) = (
        Relogio(AtomicU64::new(1_793_000_000_000), 1),
        Relogio(AtomicU64::new(1_793_000_000_000), 2),
    );
    let dir = tempfile::tempdir().unwrap();
    let raiz = ProjectFolder::create_program(&dir.path().join("Programa"), "Programa", &mbb, &c1)
        .unwrap()
        .root()
        .to_path_buf();
    ProjectFolder::open(&raiz, &ana, None)
        .unwrap()
        .register_me(&c2)
        .unwrap();

    let p = carregar_campanha().unwrap();
    let mut progresso = Progresso::default();
    for id in ["ato8/medir", "ato1/furadeira"] {
        progresso.missoes.insert(
            id.into(),
            RegistroMissao {
                tentativas: 1,
                melhor_nota: 92.0,
                ultima_nota: 92.0,
                estrelas: 2,
                resolvida: true,
                ..Default::default()
            },
        );
    }
    let r = registrar_competencias(&raiz, &ana, &c2, &p, &progresso, None).unwrap();
    assert!(r.erros.is_empty(), "{:?}", r.erros);
    assert_eq!(
        r.gravadas,
        [
            "capacidade (ato8/medir)",
            "msa (ato8/medir)",
            "vsm (ato1/furadeira)"
        ]
    );
    // Sent again: nothing repeats.
    registrar_competencias(&raiz, &ana, &c2, &p, &progresso, None).unwrap();
    let pf = ProjectFolder::open(&raiz, &mbb, None).unwrap();
    let e = mentoria::entries(pf.state());
    let dela: Vec<_> = e.iter().filter(|x| x.person == Some(ana.person)).collect();
    assert_eq!(dela.len(), 3, "{dela:#?}");
    assert!(dela.iter().all(|x| x.source == mentoria::Source::Teaching
        && x.level == mentoria::Level::Practiced
        && x.evidence.contains("UTSim Ensino")));
    // Someone outside the program is refused with the reason.
    let err = registrar_competencias(&raiz, &beto, &c2, &p, &progresso, None).unwrap_err();
    assert!(err.contains("Beto"), "{err}");
}

/// Every tool of the `competencias` tables (campaign and the packages of `ensino/pacotes`)
/// is a key of the competence map of UTSim LSS, so no entry is refused for an
/// unknown tool.
#[test]
fn ferramentas_das_tabelas_existem_no_mapa_do_seis_sigma() {
    let todas = utsim_lss_core::mentoria::all_tools();
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ensino/pacotes");
    let mut pacotes = vec![carregar_campanha().unwrap()];
    for d in std::fs::read_dir(&raiz).unwrap() {
        let d = d.unwrap().path();
        let arq = utsim_ensino_store::arquivos::ler_pacote(&d).unwrap();
        pacotes.push(utsim_ensino_core::pacote::carregar(&arq).unwrap());
    }
    assert!(pacotes.len() >= 3);
    for p in &pacotes {
        for (conceito, fs) in &p.pacote.competencias {
            for f in fs {
                assert!(
                    todas.contains(f),
                    "{}: '{conceito}' liga a ferramenta '{f}', que não está no mapa",
                    p.pacote.id
                );
            }
        }
    }
}

#[test]
fn o_professor_registra_a_turma_no_programa() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use utsim_ensino_core::turma::{Aluno, Celula, LinhaMatriz, Matriz};
    use utsim_ensino_store::seis_sigma::registrar_turma;
    use utsim_lss_core::mentoria;
    use utsim_lss_store::Clock;

    struct Relogio(AtomicU64, u8);
    impl Clock for Relogio {
        fn now_ms(&self) -> i64 {
            self.0.fetch_add(1, Ordering::Relaxed) as i64
        }
        fn random(&self) -> [u8; 10] {
            let n = self.0.load(Ordering::Relaxed);
            let mut r = [self.1; 10];
            r[2..].copy_from_slice(&n.to_be_bytes());
            r
        }
    }
    let pessoa = |n: u8, nome: &str| Identity {
        person: utsim_lss_core::Id::v7(1_700_000_000_000, [n; 10]),
        name: nome.into(),
        email: format!("p{n}@empresa.com"),
    };
    let (mbb, ana) = (pessoa(1, "MBB"), pessoa(2, "Ana"));
    let (c1, c2) = (
        Relogio(AtomicU64::new(1_793_000_000_000), 1),
        Relogio(AtomicU64::new(1_793_000_000_000), 2),
    );
    let dir = tempfile::tempdir().unwrap();
    let raiz = ProjectFolder::create_program(&dir.path().join("Programa"), "Programa", &mbb, &c1)
        .unwrap()
        .root()
        .to_path_buf();
    ProjectFolder::open(&raiz, &ana, None)
        .unwrap()
        .register_me(&c2)
        .unwrap();

    let p = carregar_campanha().unwrap();
    let aluno = |nome: &str, registro: &str, email: &str| Aluno {
        nome: nome.into(),
        registro: registro.into(),
        apelido: String::new(),
        email: email.into(),
    };
    let resolvida = |nota| {
        Some(Celula {
            nota,
            estrelas: 2,
            tentativas: 1,
            dicas: 0,
            resolvida: true,
        })
    };
    let linha = |a: Aluno, avisos: Vec<String>| LinhaMatriz {
        aluno: a,
        celulas: vec![resolvida(92.0), None],
        avisos,
    };
    let matriz = Matriz {
        missoes: vec!["ato8/medir".into(), "ato1/furadeira".into()],
        titulos: vec!["Medir".into(), "Furadeira".into()],
        linhas: vec![
            linha(aluno("Ana Lima", "ra1", " P2@Empresa.com"), vec![]),
            linha(aluno("Beto Reis", "ra2", "beto@fora.com"), vec![]),
            linha(aluno("Caio Melo", "ra3", ""), vec![]),
            linha(
                aluno("Dora Luz", "ra4", "p2@empresa.com"),
                vec!["cadeia quebrada".into()],
            ),
        ],
    };
    let r = registrar_turma(&raiz, &mbb, &c1, &p, &matriz).unwrap();
    assert_eq!((r.enviadas, r.alunos), (2, 1), "{r:?}");
    assert_eq!(r.sem_pessoa, ["Beto Reis (ra2)", "Caio Melo (ra3)"]);
    assert_eq!(r.com_avisos, ["ra4"]);
    // Again: nothing repeats.
    registrar_turma(&raiz, &mbb, &c1, &p, &matriz).unwrap();
    let pf = ProjectFolder::open(&raiz, &mbb, None).unwrap();
    let e = mentoria::entries(pf.state());
    assert_eq!(e.len(), 2, "{e:#?}");
    assert!(e.iter().all(|x| x.person == Some(ana.person)
        && x.source == mentoria::Source::Exercise
        && x.level == mentoria::Level::Practiced
        && x.by == Some(mbb.person)
        && x.evidence.contains("UTSim Ensino")
        && x.evidence.contains("nota 92")
        && !x.evidence.contains("Ana")
        && !x.evidence.contains("ra1")));
    // A common belt cannot read the e-mails nor record the class.
    let err = registrar_turma(&raiz, &ana, &c2, &p, &matriz).unwrap_err();
    assert!(!err.is_empty());
}
