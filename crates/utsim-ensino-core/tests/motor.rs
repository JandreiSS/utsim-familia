//! The engine and the grade over mission 1.1: the same mission, seed and decisions give
//! the same grade; the right purchase solves it; the wrong one does not; time limit.

use std::time::Instant;

use utsim_ensino_core::campanha::carregar_campanha;
use utsim_ensino_core::missao::Decisoes;
use utsim_ensino_core::motor::{replay, simular};
use utsim_ensino_core::nota::{RespostaPrevisao, Tentativa, corrigir};
use utsim_ensino_core::pacote::PacoteCarregado;

fn pacote() -> PacoteCarregado {
    carregar_campanha().unwrap_or_else(|e| panic!("{}", e.join("\n")))
}

fn tentativa(decisoes: &[(&str, f64)]) -> Tentativa {
    Tentativa {
        decisoes: decisoes.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        ..Default::default()
    }
}

#[test]
fn compra_certa_resolve_e_errada_nao() {
    let p = pacote();
    let m = p.missao("ato1/furadeira").unwrap();
    let inicio = Instant::now();
    let mut t = tentativa(&[("furacao", 2.0)]);
    t.previsoes
        .insert("gargalo".into(), RespostaPrevisao::Escolha(1));
    t.justificativas = vec![Some(0)];
    let sim = simular(m, "ra123", &t.decisoes, &|_| true).unwrap();
    let r = corrigir(&m.missao, &p.pacote.pesos, &sim, &t).unwrap();
    let tempo = inicio.elapsed();
    assert!(r.resolvida, "{r:#?}");
    assert_eq!(r.estrelas, 3);
    assert_eq!(r.nota, 100.0);
    assert!(r.previsoes[0].certa);
    // Act 1 missions answer in under 3 s (the debug build is slower than the release one).
    assert!(
        tempo.as_secs_f64() < 3.0 * if cfg!(debug_assertions) { 4.0 } else { 1.0 },
        "{tempo:?}"
    );

    let t2 = tentativa(&[("corte", 2.0)]);
    let sim2 = simular(m, "ra123", &t2.decisoes, &|_| true).unwrap();
    let r2 = corrigir(&m.missao, &p.pacote.pesos, &sim2, &t2).unwrap();
    assert!(!r2.resolvida);
    assert_eq!(r2.estrelas, 0);
    assert!(r2.nota < 40.0, "{}", r2.nota);
    assert!(!r2.dicas_automaticas.is_empty());

    // Over budget: the gate fails, no performance points.
    let t3 = tentativa(&[("corte", 2.0), ("furacao", 2.0)]);
    let sim3 = simular(m, "ra123", &t3.decisoes, &|_| true).unwrap();
    let r3 = corrigir(&m.missao, &p.pacote.pesos, &sim3, &t3).unwrap();
    assert!(r3.portao.is_some());
    assert_eq!(r3.pontos_desempenho, 0.0);
    assert!(!r3.resolvida);
}

#[test]
fn mesma_entrada_mesma_nota() {
    let p = pacote();
    let m = p.missao("ato1/furadeira").unwrap();
    let t = tentativa(&[("pintura", 2.0)]);
    let a = simular(m, "ra7", &t.decisoes, &|_| true).unwrap();
    let b = simular(m, "ra7", &t.decisoes, &|_| true).unwrap();
    let ra = corrigir(&m.missao, &p.pacote.pesos, &a, &t).unwrap();
    let rb = corrigir(&m.missao, &p.pacote.pesos, &b, &t).unwrap();
    assert_eq!(
        serde_json::to_string(&ra).unwrap(),
        serde_json::to_string(&rb).unwrap()
    );
    // Another student, another variant: other numbers.
    let c = simular(m, "ra8", &t.decisoes, &|_| true).unwrap();
    let rc = corrigir(&m.missao, &p.pacote.pesos, &c, &t).unwrap();
    assert_ne!(ra.metricas[0].aluno.media, rc.metricas[0].aluno.media);
}

#[test]
fn dicas_descontam() {
    let p = pacote();
    let m = p.missao("ato1/furadeira").unwrap();
    let mut t = tentativa(&[("furacao", 2.0)]);
    t.previsoes
        .insert("gargalo".into(), RespostaPrevisao::Escolha(1));
    t.justificativas = vec![Some(0)];
    let sim = simular(m, "", &t.decisoes, &|_| true).unwrap();
    for (nivel, esperado) in [(0u8, 100.0), (1, 100.0), (2, 97.0), (3, 92.0)] {
        t.dicas = nivel;
        let r = corrigir(&m.missao, &p.pacote.pesos, &sim, &t).unwrap();
        assert_eq!(r.nota, esperado, "nível {nivel}");
    }
}

#[test]
fn cancelar_para() {
    let p = pacote();
    let m = p.missao("ato1/furadeira").unwrap();
    let e = simular(m, "", &Decisoes::new(), &|_| false);
    assert!(matches!(
        e,
        Err(utsim_ensino_core::motor::ErroMotor::Cancelada)
    ));
}

#[test]
fn replay_da_tentativa() {
    let p = pacote();
    let m = p.missao("ato1/furadeira").unwrap();
    let r = replay(m, "ra1", &tentativa(&[("furacao", 2.0)]).decisoes).unwrap();
    assert!(r.stop_time <= 480.0 + 1e-9);
    assert!(!r.records.is_empty());
    assert_eq!(r.blocks.len(), m.modelo.objects.len());
}
