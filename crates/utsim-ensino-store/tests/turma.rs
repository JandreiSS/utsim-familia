//! A synthetic class of 60 students: deliveries by the class folder and by file, batch
//! grading in time, an edited grade without effect, a broken chain and a tampered file
//! flagged, the matrix exported as CSV and `.xlsx`.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use utsim_ensino_core::campanha::{arquivos, carregar_campanha};
use utsim_ensino_core::nota::{RespostaPrevisao, Tentativa};
use utsim_ensino_core::pacote::{PacoteCarregado, Registro};
use utsim_ensino_core::turma::{Aluno, PacotePublicado, Turma, importar_alunos};
use utsim_ensino_store::arquivos::{Entrega, FORMATO_ENTREGA, ler_utse};
use utsim_ensino_store::chave::Chave;
use utsim_ensino_store::correcao::corrigir_turma;
use utsim_ensino_store::diario::{RegistroTentativa, TentativaAssinada, assinar, verificar_diario};
use utsim_ensino_store::pasta::{PastaTurma, importar_entregas};
use utsim_ensino_store::perfil::{Config, Perfil, Vinculo};
use utsim_ensino_store::planilha::xlsx;

fn tentativa(furacao: f64, corte: f64) -> Tentativa {
    let mut t = Tentativa::default();
    t.decisoes.insert("furacao".into(), furacao);
    t.decisoes.insert("corte".into(), corte);
    t.previsoes
        .insert("gargalo".into(), RespostaPrevisao::Escolha(1));
    t.justificativas = vec![Some(0)];
    t.rodada = 1;
    t
}

fn registro(
    p: &PacoteCarregado,
    aluno: &str,
    t: Tentativa,
    nota: Option<f64>,
) -> RegistroTentativa {
    RegistroTentativa {
        aluno: aluno.into(),
        pacote: p.pacote.id.clone(),
        pacote_hash: p.hash.clone(),
        missao: "ato1/furadeira".into(),
        versao_nucleo: utsim_ensino_core::VERSAO_NUCLEO.into(),
        quando: 1_800_000_000_000,
        sequencia: 0,
        anterior: String::new(),
        tentativa: t,
        nota_informada: nota,
    }
}

fn diario(
    p: &PacoteCarregado,
    chave: &Chave,
    aluno: &str,
    ts: Vec<(Tentativa, Option<f64>)>,
) -> Vec<TentativaAssinada> {
    let mut out: Vec<TentativaAssinada> = Vec::new();
    for (t, n) in ts {
        let a = assinar(chave, registro(p, aluno, t, n), out.last()).unwrap();
        out.push(a);
    }
    out
}

fn seed(i: usize) -> [u8; 32] {
    let mut s = [7u8; 32];
    s[..8].copy_from_slice(&(i as u64).to_le_bytes());
    s
}

#[test]
fn turma_de_60_corrigida_em_lote() {
    let p = carregar_campanha().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let csv: String = std::iter::once("nome;registro;apelido".to_string())
        .chain((0..60).map(|i| format!("Aluno {i:02};ra{i:03};apelido{i}")))
        .collect::<Vec<_>>()
        .join("\n");
    let alunos = importar_alunos(&csv).unwrap();
    let prof = Chave::de_semente([9; 32]);
    let turma = Turma {
        id: "arranjo-2027-1".into(),
        formato: 1,
        nome: "Arranjo Físico".into(),
        periodo: "2027-1".into(),
        finalidade: "Avaliação da disciplina".into(),
        chave_professor: prof.publica(),
        pacotes: vec![PacotePublicado {
            arquivo: "campanha".into(),
            prazo: None,
        }],
        registro: Registro::Melhor,
        desafios: Vec::new(),
        equipes: Vec::new(),
    };
    let pasta = PastaTurma::criar(dir.path(), &turma, &alunos).unwrap();
    let pub_ = pasta.publicar_pacote("campanha", &arquivos()).unwrap();
    assert_eq!(ler_utse(&pub_).unwrap(), arquivos());
    let convite = pasta.convite().unwrap();
    assert_eq!(convite.chave_professor, prof.publica());

    // Half deliver through the folder, half by file.
    let offline = tempfile::tempdir().unwrap();
    for (i, a) in alunos.iter().enumerate() {
        let chave = Chave::de_semente(seed(i));
        let ts = match i % 3 {
            0 => vec![
                (tentativa(1.0, 2.0), None),
                (tentativa(2.0, 1.0), Some(100.0)),
            ],
            1 => vec![(tentativa(1.0, 2.0), Some(100.0))],
            _ => vec![(tentativa(2.0, 1.0), None)],
        };
        let d = diario(&p, &chave, &a.registro, ts);
        if i % 2 == 0 {
            for t in &d {
                pasta.gravar_tentativa(a, &chave.publica(), t).unwrap();
            }
        } else {
            Entrega {
                formato: FORMATO_ENTREGA,
                turma: turma.id.clone(),
                aluno: a.clone(),
                chave_publica: chave.publica(),
                tentativas: d,
            }
            .gravar(&offline.path().join(format!("{}.utsr", a.registro)))
            .unwrap();
        }
    }
    let (mut entregas, av1) = pasta.entregas();
    let (off, av2) = importar_entregas(offline.path());
    assert!(av1.is_empty() && av2.is_empty(), "{av1:?} {av2:?}");
    entregas.extend(off);
    assert_eq!(entregas.len(), 60);

    let inicio = Instant::now();
    let c = corrigir_turma(
        &p,
        Registro::Melhor,
        &alunos,
        &entregas,
        &|_, _| {},
        &AtomicBool::new(false),
    );
    let tempo = inicio.elapsed();
    assert!(tempo.as_secs() < 300, "{tempo:?}");
    eprintln!("60 alunos corrigidos em {tempo:?}");

    assert_eq!(c.matriz.linhas.len(), 60);
    for (i, l) in c.matriz.linhas.iter().enumerate() {
        assert!(l.avisos.is_empty(), "{i}: {:?}", l.avisos);
        let cel = l.celulas[0].as_ref().unwrap();
        let n: usize = l.aluno.registro[2..].parse().unwrap();
        match n % 3 {
            // Best of a wrong and a right attempt.
            0 => assert!(
                cel.resolvida && cel.tentativas == 2 && cel.nota > 90.0,
                "{cel:?}"
            ),
            // The wrong purchase with "100" written by the student: the grade is recomputed.
            1 => assert!(!cel.resolvida && cel.nota < 50.0, "{cel:?}"),
            _ => assert!(cel.resolvida, "{cel:?}"),
        }
    }
    let erros = &c.erros_comuns["ato1/furadeira"];
    assert!(erros[0].texto.contains("%"), "{erros:?}");

    let csv = c.matriz.csv();
    assert!(csv.starts_with("registro;nome;apelido;ato1/furadeira;ato1/lugar_errado;"));
    let (cab, linhas) = c.matriz.tabela();
    let bytes = xlsx(&[("Notas".into(), cab, linhas)]).unwrap();
    assert!(bytes.starts_with(b"PK"));
    pasta.gravar_correcao("notas.csv", csv.as_bytes()).unwrap();
}

#[test]
fn nota_editada_e_cadeia_quebrada() {
    let p = carregar_campanha().unwrap();
    let chave = Chave::de_semente(seed(1));
    let d = diario(
        &p,
        &chave,
        "ra001",
        vec![
            (tentativa(1.0, 2.0), Some(20.0)),
            (tentativa(2.0, 1.0), Some(100.0)),
        ],
    );
    assert!(
        verificar_diario(&chave.publica(), "ra001", &d)
            .avisos
            .is_empty()
    );

    // Grade or decisions edited by hand: the file no longer matches its hash.
    let mut editada = d.clone();
    editada[0].corpo = editada[0]
        .corpo
        .replace("\"nota_informada\":20.0", "\"nota_informada\":100.0");
    assert_ne!(editada[0].corpo, d[0].corpo);
    let v = verificar_diario(&chave.publica(), "ra001", &editada);
    assert_eq!(v.validas.len(), 1);
    assert!(
        v.avisos.iter().any(|a| a.contains("alterado")),
        "{:?}",
        v.avisos
    );

    // Hash recomputed but not the signature.
    let mut forjada = editada.clone();
    forjada[0].hash = utsim_ensino_store::diario::sha256_hex(forjada[0].corpo.as_bytes());
    let v = verificar_diario(&chave.publica(), "ra001", &forjada);
    assert!(
        v.avisos.iter().any(|a| a.contains("assinatura")),
        "{:?}",
        v.avisos
    );

    // An attempt removed: the chain is broken.
    let v = verificar_diario(&chave.publica(), "ra001", &d[1..]);
    assert!(
        v.avisos.iter().any(|a| a.contains("cadeia")),
        "{:?}",
        v.avisos
    );

    // The edited grade does not change the teacher's grade.
    let alunos = vec![Aluno {
        nome: "A".into(),
        registro: "ra001".into(),
        apelido: String::new(),
        email: String::new(),
    }];
    let e = |ts: Vec<TentativaAssinada>| Entrega {
        formato: FORMATO_ENTREGA,
        turma: String::new(),
        aluno: alunos[0].clone(),
        chave_publica: chave.publica(),
        tentativas: ts,
    };
    let so_primeira = diario(&p, &chave, "ra001", vec![(tentativa(1.0, 2.0), Some(20.0))]);
    let mut com_100 = so_primeira.clone();
    com_100[0].corpo = com_100[0].corpo.replace("20.0", "100.0");
    let nao = AtomicBool::new(false);
    let a = corrigir_turma(
        &p,
        Registro::Melhor,
        &alunos,
        &[e(so_primeira)],
        &|_, _| {},
        &nao,
    );
    let b = corrigir_turma(
        &p,
        Registro::Melhor,
        &alunos,
        &[e(com_100)],
        &|_, _| {},
        &nao,
    );
    let na = a.matriz.linhas[0].celulas[0].as_ref().unwrap().nota;
    assert!(
        b.matriz.linhas[0].celulas[0]
            .as_ref()
            .is_none_or(|c| c.nota <= na)
    );
    assert!(!b.matriz.linhas[0].avisos.is_empty());
}

#[test]
fn perfil_registra_e_entrega() {
    let p = carregar_campanha().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let turma_dir = tempfile::tempdir().unwrap();
    let perfil = Perfil::em(dir.path());
    let aluno = Aluno {
        nome: "Bia".into(),
        registro: "2027001".into(),
        apelido: "bia".into(),
        email: String::new(),
    };
    let t = Turma {
        id: "t1".into(),
        formato: 1,
        nome: "T".into(),
        periodo: String::new(),
        finalidade: String::new(),
        chave_professor: String::new(),
        pacotes: vec![],
        registro: Registro::Ultima,
        desafios: Vec::new(),
        equipes: Vec::new(),
    };
    let pasta = PastaTurma::criar(turma_dir.path(), &t, std::slice::from_ref(&aluno)).unwrap();
    perfil
        .salvar_config(&Config {
            identidade: Some(aluno.clone()),
            turma: Some(Vinculo {
                convite: pasta.convite().unwrap(),
                pasta: Some(turma_dir.path().to_path_buf()),
            }),
            ..Default::default()
        })
        .unwrap();
    for i in 0..3 {
        let (_, aviso) = perfil
            .registrar(&p, "ato1/furadeira", &tentativa(2.0, 1.0), Some(90.0), i)
            .unwrap();
        assert!(aviso.is_none());
    }
    let e = perfil.entrega().unwrap();
    assert_eq!(e.tentativas.len(), 3);
    assert!(
        verificar_diario(&e.chave_publica, "2027001", &e.tentativas)
            .avisos
            .is_empty()
    );
    let (na_pasta, _) = pasta.entregas();
    assert_eq!(na_pasta[0].tentativas.len(), 3);
    // The key persists between openings.
    assert_eq!(
        perfil.chave().unwrap().publica(),
        Perfil::em(dir.path()).chave().unwrap().publica()
    );
}

/// The acceptance measure of EN4: 60 students, every mission of the campaign, two
/// attempts each (the initial state and the reference). Slow in debug: run with
/// `cargo test --release -p utsim-ensino-store --test turma -- --ignored`.
#[test]
#[ignore]
fn turma_de_60_com_todas_as_missoes() {
    let p = carregar_campanha().unwrap();
    let alunos: Vec<Aluno> = (0..60)
        .map(|i| Aluno {
            nome: format!("Aluno {i:02}"),
            registro: format!("ra{i:03}"),
            apelido: String::new(),
            email: String::new(),
        })
        .collect();
    let entregas: Vec<Entrega> = alunos
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let chave = Chave::de_semente(seed(i));
            let mut d: Vec<TentativaAssinada> = Vec::new();
            // Missions decided by levers or pieces (questions and models are graded apart).
            for m in p.missoes.iter().filter(|m| !m.missao.metricas.is_empty()) {
                for decisoes in [m.missao.decisoes_iniciais(), m.missao.referencia.clone()] {
                    let mut r = registro(&p, &a.registro, Tentativa::default(), None);
                    r.missao = m.missao.id.clone();
                    r.tentativa.decisoes = decisoes;
                    let t = assinar(&chave, r, d.last()).unwrap();
                    d.push(t);
                }
            }
            Entrega {
                formato: FORMATO_ENTREGA,
                turma: String::new(),
                aluno: a.clone(),
                chave_publica: chave.publica(),
                tentativas: d,
            }
        })
        .collect();
    let inicio = Instant::now();
    let c = corrigir_turma(
        &p,
        Registro::Melhor,
        &alunos,
        &entregas,
        &|_, _| {},
        &AtomicBool::new(false),
    );
    let tempo = inicio.elapsed();
    eprintln!(
        "60 alunos x {} missões corrigidos em {tempo:?}",
        p.missoes.len()
    );
    assert!(tempo.as_secs() < 300, "{tempo:?}");
    let resolvidas = c
        .matriz
        .linhas
        .iter()
        .flat_map(|l| &l.celulas)
        .filter(|c| c.as_ref().is_some_and(|c| c.resolvida))
        .count();
    // The reference of each student's variant solves almost every mission.
    assert!(
        resolvidas as f64
            >= 0.9
                * (60
                    * p.missoes
                        .iter()
                        .filter(|m| !m.missao.metricas.is_empty())
                        .count()) as f64,
        "{resolvidas}"
    );
}

#[test]
fn ver_em_3d_grava_modelo_e_replay() {
    use utsim_ensino_store::tres_d::{argumentos, gravar_tentativa};
    let p = utsim_ensino_core::campanha::carregar_campanha().unwrap();
    let m = p.missao("ato1/furadeira").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let d = m.missao.completar(&m.missao.referencia);
    let (pm, pr) = gravar_tentativa(dir.path(), m, "ra042", &d).unwrap();
    // The files read back as the 3D client and the viewer read them.
    let modelo = utsim_model::Model::parse(&std::fs::read_to_string(&pm).unwrap()).unwrap();
    assert_eq!(
        modelo,
        utsim_ensino_core::motor::modelo_da_tentativa(m, "ra042", &d).unwrap()
    );
    let r: utsim_model::replay::Replay =
        serde_json::from_str(&std::fs::read_to_string(&pr).unwrap()).unwrap();
    assert!(!r.blocks.is_empty());
    // The 3D client opens the replay after `--`; the viewer opens the model.
    let a = argumentos(
        std::path::Path::new("3d/utsim-3d.exe"),
        &["--main-pack".into(), "x.pck".into()],
        &pm,
        &pr,
    );
    assert_eq!(&a[..3], ["--main-pack", "x.pck", "--"]);
    assert!(a[3].starts_with("--utsim-abrir=") && a[3].ends_with(".replay.json"));
    let v = argumentos(std::path::Path::new("bin/utsim-view.exe"), &[], &pm, &pr);
    assert_eq!(v, [pm.display().to_string()]);
}

#[test]
fn levar_para_o_utsim_grava_so_o_modelo() {
    use utsim_ensino_store::tres_d::{argumentos_modelo, gravar_modelo};
    let p = utsim_ensino_core::campanha::carregar_campanha().unwrap();
    let m = p.missao("ato1/furadeira").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let d = m.missao.completar(&m.missao.referencia);
    let pm = gravar_modelo(dir.path(), m, "ra042", &d).unwrap();
    let modelo = utsim_model::Model::parse(&std::fs::read_to_string(&pm).unwrap()).unwrap();
    assert_eq!(
        modelo,
        utsim_ensino_core::motor::modelo_da_tentativa(m, "ra042", &d).unwrap()
    );
    // Only the model: no replay beside it.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    // The 3D client opens the model to edit; the viewer gets the path.
    let a = argumentos_modelo(std::path::Path::new("3d/utsim-3d.exe"), &[], &pm);
    assert_eq!(a[0], "--");
    assert!(a[1].starts_with("--utsim-abrir=") && a[1].ends_with(".ron"));
    let v = argumentos_modelo(std::path::Path::new("bin/utsim-view.exe"), &[], &pm);
    assert_eq!(v, [pm.display().to_string()]);
}

#[test]
fn dados_da_missao_para_o_app_de_analise() {
    use utsim_ensino_store::analise::{argumentos, gravar_dados};
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ensino/pacotes/estatistica_qualidade");
    let p = utsim_ensino_core::pacote::carregar(
        &utsim_ensino_store::arquivos::ler_pacote(&raiz).unwrap(),
    )
    .unwrap();
    let m = p.missao("eq/capacidade").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let arqs = gravar_dados(dir.path(), m, "ra042").unwrap();
    assert_eq!(arqs.len(), 2);
    for (a, cj) in arqs.iter().zip(&m.missao.dados) {
        // The student's own data, the same the game grades from.
        assert_eq!(
            std::fs::read_to_string(a).unwrap(),
            cj.gerar(&m.missao.id, "ra042").csv()
        );
    }
    let args = argumentos(&["--x".into()], &arqs);
    assert_eq!(args[0], "--x");
    assert_eq!(args.len(), 3);
    assert!(args[1].starts_with("--utsim-abrir=") && args[1].ends_with("antes.csv"));
    assert!(args[2].ends_with("depois.csv"));
    // UTSim Qualidade gets the tool and the limits, each as its own item.
    assert_eq!(
        utsim_ensino_store::analise::opcoes("qualidade", m),
        [
            "--utsim-ferramenta=capacidade",
            "--utsim-lie=24.985",
            "--utsim-lse=25.015"
        ]
    );
    assert!(utsim_ensino_store::analise::opcoes("estatistica", m).is_empty());
}
