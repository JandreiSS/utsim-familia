//! Sistemas de capacidade finita com perda de chegadas (`on_blocked: Drop` na Source).
//!
//! M/M/1/K: λ taxa de chegada, μ taxa de serviço, ρ = λ/μ, K = capacidade total do
//! sistema (fila + servidor). Com ρ ≠ 1:
//! P_n = (1-ρ) ρ^n / (1-ρ^(K+1)), n = 0..K.
//! A probabilidade de bloqueio é P_K e a taxa efetiva de chegada é λ(1-P_K).
//! L = Σ n P_n, Lq = L - (1-P_0), W = L/λ_ef e Wq = Lq/λ_ef (Little).
//! M/M/1/1 (perda de Erlang com um servidor): P_bloqueio = ρ/(1+ρ).

use utsim_core::{RunReport, summarize};
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn assert_close(what: &str, got: f64, want: f64, rel_tol: f64) {
    let rel = (got - want).abs() / want.abs().max(1e-12);
    println!(
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
    assert!(
        rel <= rel_tol,
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
}

/// Fração de chegadas perdidas, agregada nas replicações: perdidas / (perdidas + saídas da Source).
fn loss_fraction(reports: &[RunReport], source: &str) -> f64 {
    let (mut lost, mut left) = (0.0, 0.0);
    for r in reports {
        let b = r.block(source).expect("source exists");
        lost += b.extra.get("lost").map_or(0, |t| t.count) as f64;
        left += b.output as f64;
    }
    lost / (lost + left)
}

#[test]
fn mm1k_matches_closed_form() {
    // K = 5: fila com capacidade 4 + processador com capacidade 1.
    let m = model(
        r#"Model(
            name: "M/M/1/5",
            run: (stop_time: 40000.0, warmup: 2000.0, seed: 41, replications: 8),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0), on_blocked: Drop)),
                (name: "fila", kind: Queue(capacity: Some(4))),
                (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#,
    );
    let reports = m.run_replications().unwrap();
    let s = summarize(&reports);

    let (lambda, mu, k) = (1.0_f64, 1.25_f64, 5_i32);
    let rho = lambda / mu;
    let norm = 1.0 - rho.powi(k + 1);
    let p = |n: i32| (1.0 - rho) * rho.powi(n) / norm;
    let p0 = p(0);
    let pk = p(k);
    let lambda_ef = lambda * (1.0 - pk);
    let l: f64 = (0..=k).map(|n| f64::from(n) * p(n)).sum();
    let lq = l - (1.0 - p0);
    let w = l / lambda_ef;
    let wq = lq / lambda_ef;

    assert_close(
        "fração perdida (P_K)",
        loss_fraction(&reports, "chegada"),
        pk,
        0.02,
    );
    assert_close(
        "utilização (1-P_0)",
        s["maquina.state.processing"].mean,
        1.0 - p0,
        0.02,
    );
    assert_close("Lq", s["fila.content.mean"].mean, lq, 0.08);
    assert_close("Wq", s["fila.staytime.mean"].mean, wq, 0.08);
    assert_close("W", s["saida.time_in_system.mean"].mean, w, 0.05);
}

#[test]
fn mm11_matches_erlang_loss() {
    // Sem fila: Source -> Processor. Perda = ρ/(1+ρ), com ρ = λ/μ = 0.8.
    let m = model(
        r#"Model(
            name: "M/M/1/1",
            run: (stop_time: 40000.0, warmup: 2000.0, seed: 43, replications: 8),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0), on_blocked: Drop)),
                (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "maquina"), ("maquina", "saida")],
        )"#,
    );
    let reports = m.run_replications().unwrap();
    let s = summarize(&reports);
    let rho = 1.0_f64 / 1.25;
    let loss = rho / (1.0 + rho);

    assert_close(
        "fração perdida",
        loss_fraction(&reports, "chegada"),
        loss,
        0.02,
    );
    // Utilização = 1 - P_0 = ρ/(1+ρ), igual à perda (PASTA).
    assert_close("utilização", s["maquina.state.processing"].mean, loss, 0.02);
}

#[test]
fn deterministic_losses_are_exact() {
    // Chegadas em t = 1, 2, 3, ...; o servidor (2.5) aceita em t = 1, 4, 7, ... e
    // recusa as outras duas de cada três. Em 100 unidades há 100 chegadas: 34 aceitas
    // (t = 1, 4, ..., 100) e 66 perdidas.
    let m = model(
        r#"Model(
            name: "perda determinística",
            run: (stop_time: 100.0, warmup: 0.0, seed: 1, replications: 1),
            objects: [
                (name: "chegada", kind: Source(interarrival: Const(1.0), on_blocked: Drop)),
                (name: "maquina", kind: Processor(process: Const(2.5))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "maquina"), ("maquina", "saida")],
        )"#,
    );
    let reports = m.run_replications().unwrap();
    let r = &reports[0];
    let src = r.block("chegada").unwrap();
    assert_eq!(src.extra["lost"].count, 66);
    assert_eq!(src.output, 34);
    // Nenhum item perdido chegou ao servidor.
    assert_eq!(r.block("maquina").unwrap().input, src.output);
}
