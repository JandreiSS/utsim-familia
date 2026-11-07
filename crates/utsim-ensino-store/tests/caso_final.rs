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
