//! The grade at the edges: interval touching the goal, inside and outside the tolerance,
//! performance limited to 0 and 1, the reference that pays a cost, the gate, the variant
//! and the package that refuses an unknown key.

use utsim_ensino_core::campanha::arquivos;
use utsim_ensino_core::missao::{Metrica, Objetivo, Termo, Tolerancia};
use utsim_ensino_core::nota::{Intervalo, Situacao, dinheiro, fracao, situacao};
use utsim_ensino_core::pacote::carregar;
use utsim_ensino_core::variante;

fn metrica(objetivo: Objetivo, meta: f64, tol: Tolerancia) -> Metrica {
    Metrica {
        nome: "m".into(),
        rotulo: "M".into(),
        unidade: String::new(),
        valor: vec![Termo {
            chave: "x".into(),
            fator: 1.0,
        }],
        divisor: Vec::new(),
        objetivo,
        meta: Some(meta),
        peso: 1.0,
        tolerancia: tol,
        casas: 2,
    }
}

fn iv(media: f64, meia: f64) -> Intervalo {
    Intervalo {
        media,
        meia_largura: meia,
        n: 10,
    }
}

#[test]
fn meta_menor_nos_limites() {
    let m = metrica(Objetivo::Menor, 20.0, Tolerancia::Relativa(0.02));
    // Whole interval below the goal.
    assert_eq!(situacao(&m, &iv(18.0, 1.9)), Situacao::Atingida);
    // Interval touching the goal from below: it contains the goal, mean inside the band.
    assert_eq!(situacao(&m, &iv(18.0, 2.0)), Situacao::DentroDaTolerancia);
    // Mean above the goal but within 2 % (20.4) and interval containing the goal.
    assert_eq!(situacao(&m, &iv(20.4, 1.0)), Situacao::DentroDaTolerancia);
    // Mean just past the band.
    assert_eq!(situacao(&m, &iv(20.41, 1.0)), Situacao::NaoAtingida);
    // Whole interval above the goal, even if the mean is inside the band.
    assert_eq!(situacao(&m, &iv(20.3, 0.2)), Situacao::NaoAtingida);
}

#[test]
fn meta_maior_nos_limites() {
    let m = metrica(Objetivo::Maior, 100.0, Tolerancia::Absoluta(3.0));
    assert_eq!(situacao(&m, &iv(105.0, 4.0)), Situacao::Atingida);
    assert_eq!(situacao(&m, &iv(97.0, 4.0)), Situacao::DentroDaTolerancia);
    assert_eq!(situacao(&m, &iv(96.9, 4.0)), Situacao::NaoAtingida);
    assert_eq!(situacao(&m, &iv(98.0, 1.0)), Situacao::NaoAtingida);
    // One replication: no interval, the point decides.
    assert_eq!(situacao(&m, &iv(100.5, 0.0)), Situacao::Atingida);
}

#[test]
fn desempenho_limitado() {
    // Lower is better: base 50, reference 20.
    assert_eq!(fracao(Objetivo::Menor, 35.0, 50.0, 20.0, 0.0).0, 0.5);
    assert_eq!(fracao(Objetivo::Menor, 60.0, 50.0, 20.0, 0.0).0, 0.0);
    let (d, bruta) = fracao(Objetivo::Menor, 10.0, 50.0, 20.0, 0.0);
    assert_eq!(d, 1.0);
    assert!(bruta > 1.0);
    // Higher is better.
    assert_eq!(fracao(Objetivo::Maior, 15.0, 10.0, 20.0, 0.0).0, 0.5);
    // The reference pays a cost here: at least as good as it is 1, worse is 0.
    assert_eq!(fracao(Objetivo::Menor, 90.0, 80.0, 100.0, 0.0).0, 1.0);
    assert_eq!(fracao(Objetivo::Menor, 110.0, 80.0, 100.0, 0.0).0, 0.0);
    assert_eq!(fracao(Objetivo::Menor, 101.0, 80.0, 100.0, 2.0).0, 1.0);
}

#[test]
fn variante_estavel_e_diferente() {
    let p = carregar(&arquivos()).unwrap();
    let m = &p.missao("ato1/furadeira").unwrap().missao;
    let a = variante::valores(m, "2024001");
    assert_eq!(a, variante::valores(m, "2024001"));
    assert_ne!(a, variante::valores(m, "2024002"));
    for v in &a {
        assert!((3.9..=4.1).contains(v));
    }
    // No student: the middle of the range.
    assert_eq!(variante::valores(m, ""), vec![4.0]);
}

#[test]
fn chave_inexistente_recusada_com_razao() {
    let mut arq = arquivos();
    let path = "ato1/furadeira.ron";
    let txt = String::from_utf8(arq[path].clone())
        .unwrap()
        .replace("furacao.capacity", "furadeira.capacity");
    arq.insert(path.into(), txt.into_bytes());
    let erros = carregar(&arq).unwrap_err();
    let tudo = erros.join("\n");
    assert!(tudo.contains("ato1/furadeira.ron"), "{tudo}");
    assert!(tudo.contains("furadeira"), "{tudo}");
    assert!(tudo.contains("não existe"), "{tudo}");
}

#[test]
fn formato_novo_recusado() {
    let mut arq = arquivos();
    let txt = String::from_utf8(arq["pacote.ron"].clone())
        .unwrap()
        .replace("formato: 1", "formato: 9");
    arq.insert("pacote.ron".into(), txt.into_bytes());
    let erros = carregar(&arq).unwrap_err();
    assert!(erros[0].contains("formato 9"), "{erros:?}");
}

#[test]
fn metrica_inexistente_recusada() {
    let mut arq = arquivos();
    let path = "ato1/furadeira.ron";
    let txt = String::from_utf8(arq[path].clone())
        .unwrap()
        .replace("saida.time_in_system.mean", "saida.tempo_total.mean");
    arq.insert(path.into(), txt.into_bytes());
    let tudo = carregar(&arq).unwrap_err().join("\n");
    assert!(tudo.contains("saida.tempo_total.mean"), "{tudo}");
}

#[test]
fn dinheiro_em_reais() {
    assert_eq!(dinheiro(80000.0), "R$ 80.000");
    assert_eq!(dinheiro(1234567.0), "R$ 1.234.567");
    assert_eq!(dinheiro(500.0), "R$ 500");
}
