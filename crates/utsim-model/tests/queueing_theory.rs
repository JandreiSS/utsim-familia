//! Validação do motor contra fórmulas fechadas da teoria das filas.
//!
//! Notação: λ taxa de chegada, μ taxa de serviço por servidor, a = λ/μ (carga em
//! Erlangs), ρ = λ/(cμ) utilização por servidor, Lq e Wq tamanho e espera na fila,
//! W tempo no sistema. Lei de Little: L = λW e Lq = λWq.
//!
//! - M/M/c (Erlang C): P_w = [a^c/c! · 1/(1-ρ)] / [Σ_{k<c} a^k/k! + a^c/c! · 1/(1-ρ)],
//!   Lq = P_w · ρ/(1-ρ), Wq = Lq/λ, W = Wq + 1/μ, e o número médio de servidores
//!   ocupados é a.
//! - Rede de Jackson em série (filas M/M/1 independentes): Lq_i = ρ_i²/(1-ρ_i),
//!   W = Σ 1/(μ_i - λ).
//! - Disciplina LIFO em M/M/1 é conservativa: o número médio na fila continua
//!   Lq = ρ²/(1-ρ), e a permanência média segue Little, Wq = Lq/λ.

use utsim_core::summarize;
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn assert_close(what: &str, got: f64, want: f64, rel_tol: f64) {
    let rel = (got - want).abs() / want.abs().max(1e-12);
    assert!(
        rel <= rel_tol,
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
}

/// Probabilidade de espera Erlang C para c servidores e carga a = λ/μ (exige a < c).
fn erlang_c(c: u32, a: f64) -> f64 {
    let rho = a / f64::from(c);
    // a^k/k! computed incrementally to avoid overflow for moderate c.
    let mut term = 1.0; // a^0/0!
    let mut sum = 0.0;
    for k in 0..c {
        sum += term;
        term *= a / f64::from(k + 1);
    }
    // After the loop, `term` is a^c/c!.
    let tail = term / (1.0 - rho);
    tail / (sum + tail)
}

#[test]
fn mmc_matches_erlang_c() {
    // M/M/3 com λ = 1, μ = 1/2.4, a = 2.4, ρ = 0.8.
    let m = model(
        r#"Model(
            name: "M/M/3",
            run: (stop_time: 60000.0, warmup: 2000.0, seed: 11, replications: 8),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue()),
                (name: "maquina", kind: Processor(capacity: 3, process: Exp(mean: 2.4))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#,
    );
    let s = summarize(&m.run_replications().unwrap());
    let (lambda, mu, c) = (1.0_f64, 1.0 / 2.4, 3_u32);
    let a = lambda / mu;
    let rho = a / f64::from(c);
    let pw = erlang_c(c, a);
    let lq = pw * rho / (1.0 - rho);
    let wq = lq / lambda;
    let w = wq + 1.0 / mu;

    assert_close(
        "ocupação média dos servidores (a)",
        s["maquina.content.mean"].mean,
        a,
        0.02,
    );
    assert_close("Lq", s["fila.content.mean"].mean, lq, 0.08);
    assert_close("Wq", s["fila.staytime.mean"].mean, wq, 0.08);
    assert_close("W", s["saida.time_in_system.mean"].mean, w, 0.05);
}

#[test]
fn tandem_jackson_network() {
    // Fonte λ = 1 -> q1 -> p1 (μ1 = 1/0.7) -> q2 -> p2 (μ2 = 1/0.8) -> saída.
    let m = model(
        r#"Model(
            name: "jackson",
            run: (stop_time: 60000.0, warmup: 2000.0, seed: 23, replications: 8),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "q1", kind: Queue()),
                (name: "p1", kind: Processor(process: Exp(mean: 0.7))),
                (name: "q2", kind: Queue()),
                (name: "p2", kind: Processor(process: Exp(mean: 0.8))),
                (name: "saida", kind: Sink),
            ],
            connections: [
                ("chegada", "q1"),
                ("q1", "p1"),
                ("p1", "q2"),
                ("q2", "p2"),
                ("p2", "saida"),
            ],
        )"#,
    );
    let s = summarize(&m.run_replications().unwrap());
    let lambda = 1.0_f64;
    let (mu1, mu2) = (1.0 / 0.7, 1.0 / 0.8);
    let (rho1, rho2) = (lambda / mu1, lambda / mu2);
    let lq1 = rho1 * rho1 / (1.0 - rho1);
    let lq2 = rho2 * rho2 / (1.0 - rho2);
    let w = 1.0 / (mu1 - lambda) + 1.0 / (mu2 - lambda);

    assert_close("rho1", s["p1.state.processing"].mean, rho1, 0.02);
    assert_close("rho2", s["p2.state.processing"].mean, rho2, 0.02);
    assert_close("Lq1", s["q1.content.mean"].mean, lq1, 0.08);
    assert_close("Lq2", s["q2.content.mean"].mean, lq2, 0.08);
    assert_close("W total", s["saida.time_in_system.mean"].mean, w, 0.05);
}

#[test]
fn lifo_has_same_mean_queue_length() {
    // M/M/1 com λ = 1, μ = 1/0.8, disciplina LIFO na fila.
    let m = model(
        r#"Model(
            name: "mm1-lifo",
            run: (stop_time: 60000.0, warmup: 2000.0, seed: 37, replications: 8),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue(discipline: Lifo)),
                (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#,
    );
    let s = summarize(&m.run_replications().unwrap());
    let (lambda, mu) = (1.0_f64, 1.25_f64);
    let rho = lambda / mu;
    let lq = rho * rho / (1.0 - rho);
    let wq = lq / lambda;

    assert_close("rho", s["maquina.state.processing"].mean, rho, 0.02);
    assert_close("Lq (LIFO)", s["fila.content.mean"].mean, lq, 0.08);
    assert_close("Wq (LIFO)", s["fila.staytime.mean"].mean, wq, 0.08);
}
