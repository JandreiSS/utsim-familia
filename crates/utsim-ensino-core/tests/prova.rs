//! Exam mode (EN9): the same seed gives the same exam, the student's package carries no
//! reference, hints or answer key, the teacher grades with the rebuilt package, and copied
//! pairs in a synthetic class are pointed.

use std::collections::BTreeSet;

use utsim_ensino_core::campanha::{arquivos, carregar_campanha};
use utsim_ensino_core::pacote::{carregar, hash_arquivos};
use utsim_ensino_core::prova::{
    Montagem, Prova, Rastro, Regra, data_hora, ler_data_hora, montar, pacote_do_professor,
    semelhancas, sortear,
};

fn montagem(semente: u64) -> Montagem {
    let p = carregar_campanha().unwrap();
    Montagem {
        id: "prova1".into(),
        titulo: "Prova 1".into(),
        pacote_base: p.pacote.id.clone(),
        hash_base: p.hash.clone(),
        regras: vec![
            Regra {
                ato: Some(1),
                conceito: None,
                quantidade: 2,
            },
            Regra {
                ato: None,
                conceito: Some("fila".into()),
                quantidade: 1,
            },
        ],
        prova: Prova {
            semente,
            inicio: 1_800_000_000_000,
            fim: 1_800_000_000_000 + 3 * 3_600_000,
            duracao_min: 90,
            tentativas: 2,
            fuso_min: -180,
        },
    }
}

#[test]
fn mesma_semente_mesma_prova() {
    let p = carregar_campanha().unwrap();
    let m = montagem(42);
    let a = sortear(&p, &m.regras, 42).unwrap();
    assert_eq!(a, sortear(&p, &m.regras, 42).unwrap());
    assert_eq!(a.len(), 3);
    assert_eq!(a.iter().collect::<BTreeSet<_>>().len(), 3, "sem repetição");
    assert!(a[..2].iter().all(|id| id.starts_with("ato1/")));
    assert!(
        p.missao(&a[2])
            .unwrap()
            .missao
            .conceitos
            .contains(&"fila".to_string())
    );
    // Other seeds give other exams.
    let outras: BTreeSet<Vec<String>> = (0..20)
        .map(|s| sortear(&p, &m.regras, s).unwrap())
        .collect();
    assert!(outras.len() > 3, "{outras:?}");
    // Act 7 asks for a model: out of the draw.
    let r = [Regra {
        ato: Some(7),
        conceito: None,
        quantidade: 2,
    }];
    assert!(sortear(&p, &r, 1).is_err());
    let r = [Regra {
        ato: Some(1),
        conceito: None,
        quantidade: 9,
    }];
    assert!(sortear(&p, &r, 1).unwrap_err().contains("há 4"));
}

#[test]
fn pacote_do_aluno_nao_leva_gabarito() {
    let p = carregar_campanha().unwrap();
    let pm = montar(&arquivos(), &p, &montagem(7)).unwrap();
    let aluno = carregar(&pm.aluno).unwrap_or_else(|e| panic!("{}", e.join("\n")));
    assert_eq!(aluno.pacote.id, "prova1");
    assert!(aluno.pacote.prova.is_some());
    assert_eq!(
        aluno
            .missoes
            .iter()
            .map(|m| m.missao.id.clone())
            .collect::<Vec<_>>(),
        pm.missoes
    );
    for m in &aluno.missoes {
        assert!(m.missao.referencia.is_empty(), "{}", m.missao.id);
        assert!(m.missao.dicas.is_empty(), "{}", m.missao.id);
        assert!(m.missao.justificativa.iter().all(|q| q.certa == 0));
    }
    // No file of a mission left out, and the statements of the chosen ones stay.
    let base = arquivos();
    let fora: Vec<&String> = base
        .keys()
        .filter(|k| k.starts_with("ato") && !pm.aluno.contains_key(*k))
        .collect();
    assert!(!fora.is_empty());
    assert!(
        pm.aluno
            .keys()
            .filter(|k| k.starts_with("ato") && k.ends_with(".ron"))
            .count()
            == 3
    );
    // The teacher's package has the reference and the hash of the student's package.
    let prof = pacote_do_professor(&pm).unwrap();
    assert_eq!(prof.hash, hash_arquivos(&pm.aluno));
    assert!(prof.missoes.iter().all(|m| !m.missao.referencia.is_empty()));
}

#[test]
fn professor_corrige_com_o_pacote_reconstruido() {
    use utsim_ensino_core::nota::{Tentativa, corrigir};
    let p = carregar_campanha().unwrap();
    let m = montagem(7);
    let pm = montar(&arquivos(), &p, &m).unwrap();
    // The teacher rebuilds the same exam from the seed.
    let de_novo = montar(&arquivos(), &p, &m).unwrap();
    assert_eq!(hash_arquivos(&de_novo.aluno), hash_arquivos(&pm.aluno));
    let prof = pacote_do_professor(&de_novo).unwrap();
    let mc = &prof.missoes[0];
    let t = Tentativa {
        aluno: "ra042".into(),
        decisoes: mc.missao.completar(&mc.missao.referencia),
        justificativas: mc
            .missao
            .justificativa
            .iter()
            .map(|q| Some(q.certa))
            .collect(),
        ..Default::default()
    };
    let sim = utsim_ensino_core::motor::simular(mc, "ra042", &t.decisoes, &|_| true).unwrap();
    let r = corrigir(&mc.missao, &prof.pacote.pesos, &sim, &t).unwrap();
    assert!(r.resolvida, "{r:#?}");
}

#[test]
fn janela_e_tempo() {
    let pr = montagem(1).prova;
    assert!(pr.pode_rodar(pr.inicio - 1, None).is_err());
    assert!(pr.pode_rodar(pr.inicio + 10, None).is_ok());
    assert!(pr.pode_rodar(pr.fim + 1, None).is_err());
    let aberta = pr.inicio + 60_000;
    assert!(pr.pode_rodar(aberta + 89 * 60_000, Some(aberta)).is_ok());
    assert!(
        pr.pode_rodar(aberta + 91 * 60_000, Some(aberta))
            .unwrap_err()
            .contains("90 min")
    );
    assert_eq!(pr.restante(aberta, Some(aberta)), 90 * 60_000);
    // Dates in Brasília time.
    let t = ler_data_hora("2027-03-15 19:30", -180).unwrap();
    assert_eq!(data_hora(t, -180), "2027-03-15 19:30");
    assert_eq!(data_hora(t, 0), "2027-03-15 22:30");
    assert_eq!(data_hora(0, 0), "1970-01-01 00:00");
    assert_eq!(
        ler_data_hora("2028-02-29 00:00", 0)
            .map(|t| data_hora(t, 0))
            .as_deref(),
        Some("2028-02-29 00:00")
    );
    assert!(ler_data_hora("2027-02-29 10:00", 0).is_none());
    assert!(ler_data_hora("ontem", 0).is_none());
}

#[test]
fn pares_copiados_sao_apontados() {
    let d = |v: f64| [("x".to_string(), v)].into_iter().collect();
    let mut rastros = Vec::new();
    let t0 = 1_800_000_000_000;
    // ana and beto: the same three decisions, at the same moments.
    for (k, v) in [1.0, 3.0, 2.0].into_iter().enumerate() {
        let q = t0 + k as i64 * 300_000;
        rastros.push(Rastro {
            aluno: "ana".into(),
            missao: "m1".into(),
            quando: q,
            decisoes: d(v),
        });
        rastros.push(Rastro {
            aluno: "beto".into(),
            missao: "m1".into(),
            quando: q + 20_000,
            decisoes: d(v),
        });
    }
    // caio: his own path, at his own pace.
    for (k, v) in [2.0, 2.0, 3.0].into_iter().enumerate() {
        rastros.push(Rastro {
            aluno: "caio".into(),
            missao: "m1".into(),
            quando: t0 + 7_000_000 + k as i64 * 400_000,
            decisoes: d(v),
        });
    }
    let com: BTreeSet<String> = ["m1".to_string()].into();
    let s = semelhancas(&rastros, &com, 60_000);
    assert_eq!(s.len(), 1, "{s:#?}");
    assert_eq!((s[0].a.as_str(), s[0].b.as_str()), ("ana", "beto"));
    assert!(s[0].motivo.contains("mesmas 3 decisões") && s[0].motivo.contains("mesmos instantes"));
    // Without a per-student variant, equal decisions are expected: only the times count.
    let s = semelhancas(&rastros, &BTreeSet::new(), 10_000);
    assert!(s.is_empty(), "{s:#?}");
}
