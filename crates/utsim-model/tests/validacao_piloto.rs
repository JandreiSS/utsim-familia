//! Validação do piloto: reproduz resultados conhecidos (fechados ou calculados de forma
//! independente) em situações além das cobertas por `queueing_theory.rs`.
//!
//! Cada teste deriva o valor esperado por conta própria (fórmula fechada ou uma cadeia de
//! Markov resolvida aqui mesmo) e compara com a média das replicações usando o intervalo de
//! confiança de 95% (`Summary`, t de Student). Critério de aceitação:
//!
//! `|média - esperado| <= 1.4 * meia_largura + piso * |esperado|`
//!
//! O fator 1.4 leva o IC95% a aproximadamente IC99% (t_0.995 / t_0.975 fica entre 1.35 e
//! 1.45 para 9 a 29 graus de liberdade), o que mantém a taxa de falso alarme baixa sem
//! esconder erros de modelagem. O `piso` (0.2% a 1%) cobre o viés residual de
//! aquecimento finito e de contagem de bordas. As sementes são fixas: os testes são
//! determinísticos.
//!
//! Cada linha de resultado é impressa como `VALIDACAO|caso|métrica|esperado|média|meia
//! largura|PASS ou FAIL`; `scripts/validacao_tabela.sh` junta essas linhas na tabela de
//! `docs/validacao.md`. Rode com `cargo test --release -p utsim-model --test
//! validacao_piloto -- --nocapture --test-threads=1`.

use std::collections::{HashMap, HashSet};

use utsim_core::stats::Summary;
use utsim_core::{RunReport, TraceEvent};
use utsim_model::Model;

// ------------------------------------------------------------------- utilidades

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn reports(text: &str) -> Vec<RunReport> {
    model(text).run_replications().expect("run ok")
}

fn values(reps: &[RunReport], f: impl Fn(&RunReport) -> f64) -> Vec<f64> {
    reps.iter().map(f).collect()
}

fn metric(r: &RunReport, key: &str) -> f64 {
    *r.metrics()
        .get(key)
        .unwrap_or_else(|| panic!("métrica '{key}' ausente"))
}

/// Coletor de verificações de um caso: imprime todas as linhas e só então falha.
struct Case {
    name: &'static str,
    failed: Vec<String>,
}

impl Case {
    fn new(name: &'static str) -> Self {
        Case {
            name,
            failed: Vec::new(),
        }
    }

    /// Compara a média de `xs` (uma amostra por replicação) com `expected`.
    fn ci(&mut self, what: &str, expected: f64, xs: &[f64], floor: f64) {
        let s = Summary::from_samples(xs);
        let tol = 1.4 * s.half_width + floor * expected.abs();
        let pass = (s.mean - expected).abs() <= tol;
        self.line(what, expected, s.mean, s.half_width, pass);
    }

    /// Valor determinístico (uma execução): tolerância absoluta explícita.
    fn exact(&mut self, what: &str, expected: f64, got: f64, tol: f64) {
        let pass = (got - expected).abs() <= tol;
        self.line(what, expected, got, 0.0, pass);
    }

    fn line(&mut self, what: &str, expected: f64, mean: f64, hw: f64, pass: bool) {
        println!(
            "VALIDACAO|{}|{what}|{expected:.6}|{mean:.6}|{hw:.6}|{}",
            self.name,
            if pass { "PASS" } else { "FAIL" }
        );
        if !pass {
            self.failed.push(format!(
                "{what}: esperado {expected:.6}, obtido {mean:.6} ± {hw:.6}"
            ));
        }
    }

    fn finish(self) {
        assert!(
            self.failed.is_empty(),
            "{}: {}",
            self.name,
            self.failed.join("; ")
        );
    }
}

// ----------------------------------------------- 1. filas em série com bloqueio

/// Solução estacionária de uma linha de máquinas exponenciais com buffers finitos,
/// bloqueio após o serviço (a máquina termina e segura a peça até haver lugar) e
/// primeira máquina sempre abastecida.
struct LineSolution {
    throughput: f64,
    /// Fração do tempo processando, por máquina.
    working: Vec<f64>,
    /// Fração do tempo bloqueada, por máquina.
    blocked: Vec<f64>,
    /// Conteúdo médio de cada buffer.
    buffer: Vec<f64>,
}

const IDLE: u8 = 0;
const WORKING: u8 = 1;
const BLOCKED: u8 = 2;

/// Estado: status de cada máquina seguido do conteúdo de cada buffer.
type LineState = Vec<u8>;

/// Resolve as consequências instantâneas de um evento até o estado ficar estável.
fn settle(state: &mut LineState, buf: &[usize]) {
    let m = buf.len() + 1;
    loop {
        let mut changed = false;
        for i in 0..m {
            if state[i] == BLOCKED {
                if state[i + 1] == IDLE {
                    state[i + 1] = WORKING;
                    state[i] = IDLE;
                    changed = true;
                } else if (state[m + i] as usize) < buf[i] {
                    state[m + i] += 1;
                    state[i] = IDLE;
                    changed = true;
                }
            }
            if state[i] == IDLE {
                if i == 0 {
                    state[0] = WORKING;
                    changed = true;
                } else if state[m + i - 1] > 0 {
                    state[m + i - 1] -= 1;
                    state[i] = WORKING;
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
}

/// Resolve `pi Q = 0`, `sum pi = 1` por eliminação de Gauss com pivô parcial.
fn stationary(q: &mut [Vec<f64>]) -> Vec<f64> {
    let n = q.len();
    // A transposta de Q com a última equação trocada pela normalização.
    let mut a = vec![vec![0.0; n + 1]; n];
    for i in 0..n {
        for j in 0..n {
            a[j][i] = q[i][j];
        }
    }
    a[n - 1] = vec![1.0; n + 1];
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))
            .unwrap();
        a.swap(col, pivot);
        let d = a[col][col];
        assert!(d.abs() > 1e-12, "sistema singular");
        let pivot_row = a[col].clone();
        for (row, line) in a.iter_mut().enumerate() {
            if row != col {
                let f = line[col] / d;
                if f != 0.0 {
                    for (x, p) in line.iter_mut().zip(&pivot_row).skip(col) {
                        *x -= f * p;
                    }
                }
            }
        }
    }
    (0..n).map(|i| a[i][n] / a[i][i]).collect()
}

fn solve_line(mu: &[f64], buf: &[usize]) -> LineSolution {
    let m = mu.len();
    assert_eq!(buf.len(), m - 1);
    let mut start: LineState = vec![0; 2 * m - 1];
    settle(&mut start, buf);
    let mut index: HashMap<LineState, usize> = HashMap::new();
    let mut states = vec![start.clone()];
    index.insert(start, 0);
    let mut transitions: Vec<(usize, usize, f64)> = Vec::new();
    let mut k = 0;
    while k < states.len() {
        let s = states[k].clone();
        for i in 0..m {
            if s[i] != WORKING {
                continue;
            }
            let mut next = s.clone();
            next[i] = if i == m - 1 { IDLE } else { BLOCKED };
            settle(&mut next, buf);
            let id = *index.entry(next.clone()).or_insert_with(|| {
                states.push(next);
                states.len() - 1
            });
            transitions.push((k, id, mu[i]));
        }
        k += 1;
    }
    let n = states.len();
    let mut q = vec![vec![0.0; n]; n];
    for (from, to, rate) in transitions {
        q[from][to] += rate;
        q[from][from] -= rate;
    }
    let pi = stationary(&mut q);
    let prob = |pred: &dyn Fn(&LineState) -> bool| -> f64 {
        states
            .iter()
            .zip(&pi)
            .filter(|(s, _)| pred(s))
            .map(|(_, p)| p)
            .sum()
    };
    LineSolution {
        throughput: mu[m - 1] * prob(&|s| s[m - 1] == WORKING),
        working: (0..m).map(|i| prob(&|s| s[i] == WORKING)).collect(),
        blocked: (0..m).map(|i| prob(&|s| s[i] == BLOCKED)).collect(),
        buffer: (0..m - 1)
            .map(|i| {
                states
                    .iter()
                    .zip(&pi)
                    .map(|(s, p)| f64::from(s[m + i]) * p)
                    .sum()
            })
            .collect(),
    }
}

/// Texto RON da linha saturada: fonte sem espera -> m1 -> [q1] -> m2 -> ... -> saída.
fn line_model(mu: &[f64], buf: &[usize], stop: f64, warmup: f64, reps: u32, seed: u64) -> String {
    let mut objects = String::from(
        r#"(name: "src", kind: Source(interarrival: Const(1e-4))),
"#,
    );
    let mut connections = Vec::new();
    let mut prev = "src".to_string();
    for (i, rate) in mu.iter().enumerate() {
        let name = format!("m{}", i + 1);
        objects.push_str(&format!(
            "(name: \"{name}\", kind: Processor(process: Exp(mean: {:?}))),\n",
            1.0 / rate
        ));
        connections.push(format!("(\"{prev}\", \"{name}\")"));
        prev = name;
        if i < buf.len() && buf[i] > 0 {
            let q = format!("q{}", i + 1);
            objects.push_str(&format!(
                "(name: \"{q}\", kind: Queue(capacity: Some({})))\n,",
                buf[i]
            ));
            connections.push(format!("(\"{prev}\", \"{q}\")"));
            prev = q;
        }
    }
    objects.push_str("(name: \"out\", kind: Sink),\n");
    connections.push(format!("(\"{prev}\", \"out\")"));
    format!(
        "Model(name: \"linha\", run: (stop_time: {stop:?}, warmup: {warmup:?}, seed: {seed}, replications: {reps}), objects: [{objects}], connections: [{}])",
        connections.join(", ")
    )
}

fn check_line(name: &'static str, mu: &[f64], buf: &[usize], seed: u64) {
    let exact = solve_line(mu, buf);
    let (stop, warmup, nrep) = (30_000.0, 1_000.0, 20);
    let reps = reports(&line_model(mu, buf, stop, warmup, nrep, seed));
    let mut case = Case::new(name);
    case.ci(
        "vazão",
        exact.throughput,
        &values(&reps, |r| {
            r.block("out").unwrap().input as f64 / (stop - warmup)
        }),
        0.002,
    );
    for i in 0..mu.len() {
        let m = format!("m{}", i + 1);
        case.ci(
            &format!("{m} processando"),
            exact.working[i],
            &values(&reps, |r| metric(r, &format!("{m}.state.processing"))),
            0.002,
        );
        if i < mu.len() - 1 {
            case.ci(
                &format!("{m} bloqueada"),
                exact.blocked[i],
                &values(&reps, |r| {
                    r.metrics()
                        .get(&format!("{m}.state.blocked"))
                        .copied()
                        .unwrap_or(0.0)
                }),
                0.002,
            );
        }
    }
    for (i, b) in buf.iter().enumerate() {
        if *b > 0 {
            case.ci(
                &format!("conteúdo médio de q{}", i + 1),
                exact.buffer[i],
                &values(&reps, |r| metric(r, &format!("q{}.content.mean", i + 1))),
                0.002,
            );
        }
    }
    case.finish();
}

#[test]
fn tandem_two_machines_without_buffer() {
    check_line(
        "1a. Série 2 máquinas, sem buffer (μ=1; 1)",
        &[1.0, 1.0],
        &[0],
        101,
    );
}

#[test]
fn tandem_two_machines_buffer_three() {
    check_line(
        "1b. Série 2 máquinas, buffer 3 (μ=1; 0.8)",
        &[1.0, 0.8],
        &[3],
        102,
    );
}

#[test]
fn tandem_three_machines_two_buffers() {
    check_line(
        "1c. Série 3 máquinas, buffers 1 e 2 (μ=1; 0.8; 1.25)",
        &[1.0, 0.8, 1.25],
        &[1, 2],
        103,
    );
}

#[test]
fn tandem_ctmc_solver_matches_known_closed_forms() {
    // Duas máquinas iguais, sem buffer: estados (T,T), (T,B), (B,T) -> vazão 2/3 μ.
    let s = solve_line(&[1.0, 1.0], &[0]);
    assert!((s.throughput - 2.0 / 3.0).abs() < 1e-12, "{}", s.throughput);
    // Duas máquinas iguais com buffer 1: vazão conhecida de 3/4 μ.
    let s = solve_line(&[1.0, 1.0], &[1]);
    assert!((s.throughput - 0.75).abs() < 1e-12, "{}", s.throughput);
    // Buffer b grande: vazão tende a min(μ) = 0.8.
    let s = solve_line(&[1.0, 0.8], &[60]);
    assert!((s.throughput - 0.8).abs() < 1e-3, "{}", s.throughput);
}

// --------------------------------------------------- 2. laço de retrabalho

fn rework_model(lambda: f64, p: f64, s: f64, stop: f64, seed: u64) -> String {
    format!(
        r#"Model(
            name: "retrabalho",
            run: (stop_time: {stop:?}, warmup: 2000.0, seed: {seed}, replications: 24),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: {:?}))),
                (name: "q", kind: Queue()),
                (name: "p", kind: Processor(process: Exp(mean: {s:?})), output: Weighted([{p:?}, {:?}])),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "q"), ("p", "out")],
        )"#,
        1.0 / lambda,
        1.0 - p
    )
}

fn check_rework(name: &'static str, lambda: f64, p: f64, s: f64, stop: f64, seed: u64) {
    // Rede de Jackson com realimentação: taxa efetiva λ/(1-p), ρ = λ_ef s, e o número no
    // sistema (fila + servidor) é M/M/1: L = ρ/(1-ρ). Little: W = L/λ.
    let visits = 1.0 / (1.0 - p);
    let rho = lambda * visits * s;
    let l = rho / (1.0 - rho);
    let reps = reports(&rework_model(lambda, p, s, stop, seed));
    let mut case = Case::new(name);
    case.ci(
        "razão de visitas (saídas de p / saídas do sistema)",
        visits,
        &values(&reps, |r| {
            r.block("p").unwrap().output as f64 / r.block("out").unwrap().input as f64
        }),
        0.005,
    );
    case.ci(
        "utilização ρ",
        rho,
        &values(&reps, |r| metric(r, "p.state.processing")),
        0.005,
    );
    case.ci(
        "número médio no sistema L",
        l,
        &values(&reps, |r| {
            metric(r, "q.content.mean") + metric(r, "p.content.mean")
        }),
        0.01,
    );
    case.ci(
        "fila Lq = ρ²/(1-ρ)",
        rho * rho / (1.0 - rho),
        &values(&reps, |r| metric(r, "q.content.mean")),
        0.01,
    );
    case.ci(
        "tempo no sistema W = L/λ",
        l / lambda,
        &values(&reps, |r| metric(r, "out.time_in_system.mean")),
        0.01,
    );
    case.finish();
}

#[test]
fn rework_loop_moderate() {
    check_rework(
        "2a. Retrabalho p=0.25 (λ=0.3, s=1.5, ρ=0.6)",
        0.3,
        0.25,
        1.5,
        60_000.0,
        201,
    );
}

#[test]
fn rework_loop_heavy_feedback() {
    check_rework(
        "2b. Retrabalho p=0.5 (λ=0.2, s=1.8, ρ=0.72)",
        0.2,
        0.5,
        1.8,
        100_000.0,
        202,
    );
}

// ------------------------------------------------------ 3. M/G/1 (Pollaczek-Khinchine)

fn check_mg1(name: &'static str, process: &str, cs2: f64, seed: u64, stop: f64) {
    // λ = 1, E[S] = 0.8, ρ = 0.8. Wq = λ E[S²] / (2(1-ρ)), E[S²] = E[S]² (1 + cs²).
    let (lambda, es): (f64, f64) = (1.0, 0.8);
    let rho = lambda * es;
    let es2 = es * es * (1.0 + cs2);
    let wq = lambda * es2 / (2.0 * (1.0 - rho));
    let text = format!(
        r#"Model(
            name: "mg1",
            run: (stop_time: {stop:?}, warmup: 2000.0, seed: {seed}, replications: 20),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue()),
                (name: "m", kind: Processor(process: {process})),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "fila"), ("fila", "m"), ("m", "out")],
        )"#
    );
    let reps = reports(&text);
    let mut case = Case::new(name);
    case.ci(
        "ρ",
        rho,
        &values(&reps, |r| metric(r, "m.state.processing")),
        0.005,
    );
    case.ci(
        "Wq",
        wq,
        &values(&reps, |r| metric(r, "fila.staytime.mean")),
        0.01,
    );
    case.ci(
        "Lq = λ Wq",
        lambda * wq,
        &values(&reps, |r| metric(r, "fila.content.mean")),
        0.01,
    );
    case.ci(
        "W = Wq + E[S]",
        wq + es,
        &values(&reps, |r| metric(r, "out.time_in_system.mean")),
        0.01,
    );
    case.finish();
}

#[test]
fn mg1_deterministic_service() {
    check_mg1("3a. M/D/1 (cs²=0)", "Const(0.8)", 0.0, 301, 60_000.0);
}

#[test]
fn mg1_erlang2_service() {
    check_mg1(
        "3b. M/E2/1 (cs²=1/2)",
        "Erlang(k: 2, mean: 0.8)",
        0.5,
        302,
        60_000.0,
    );
}

#[test]
fn mg1_erlang4_service() {
    check_mg1(
        "3c. M/E4/1 (cs²=1/4)",
        "Erlang(k: 4, mean: 0.8)",
        0.25,
        303,
        60_000.0,
    );
}

#[test]
fn mg1_uniform_service() {
    // Uniform(0.4, 1.2): variância (0.8)²/12, cs² = 1/12.
    check_mg1(
        "3d. M/U/1 (cs²=1/12)",
        "Uniform(min: 0.4, max: 1.2)",
        1.0 / 12.0,
        304,
        60_000.0,
    );
}

#[test]
fn mg1_high_variability_service() {
    // Gamma(shape 1/2): cs² = 1/shape = 2.
    check_mg1(
        "3e. M/G/1 Gama (cs²=2)",
        "Gamma(shape: 0.5, scale: 1.6)",
        2.0,
        305,
        150_000.0,
    );
}

// ------------------------------------------------------------ 4. quebras de máquina

/// Máquina saturada (fonte sem espera) com quebras: disponibilidade A = MTBF/(MTBF+MTTR);
/// o trabalho é retomado após o reparo, então a vazão é A / E[S].
fn check_breakdown(
    name: &'static str,
    process: &str,
    es: f64,
    mtbf: (&str, f64),
    mttr: (&str, f64),
    seed: u64,
) {
    let a = mtbf.1 / (mtbf.1 + mttr.1);
    let (stop, warmup) = (40_000.0, 1_000.0);
    let text = format!(
        r#"Model(
            name: "quebras",
            run: (stop_time: {stop:?}, warmup: {warmup:?}, seed: {seed}, replications: 20),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1e-4))),
                (name: "m", kind: Processor(process: {process}),
                 downtime: Some((mtbf: {}, mttr: {}))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "m"), ("m", "out")],
        )"#,
        mtbf.0, mttr.0
    );
    let reps = reports(&text);
    let mut case = Case::new(name);
    case.ci(
        "disponibilidade (fração processando)",
        a,
        &values(&reps, |r| metric(r, "m.state.processing")),
        0.003,
    );
    case.ci(
        "fração em falha",
        1.0 - a,
        &values(&reps, |r| metric(r, "m.state.down")),
        0.003,
    );
    case.ci(
        "vazão efetiva A/E[S]",
        a / es,
        &values(&reps, |r| {
            r.block("out").unwrap().input as f64 / (stop - warmup)
        }),
        0.003,
    );
    case.finish();
}

#[test]
fn breakdowns_exponential_up_and_down() {
    check_breakdown(
        "4a. Quebras Exp/Exp (MTBF 40, MTTR 10)",
        "Exp(mean: 1.0)",
        1.0,
        ("Exp(mean: 40.0)", 40.0),
        ("Exp(mean: 10.0)", 10.0),
        401,
    );
}

#[test]
fn breakdowns_availability_is_insensitive_to_the_distributions() {
    // Gama (forma 2) para o tempo até a falha e Uniforme para o reparo, processo fixo.
    check_breakdown(
        "4b. Quebras Gama/Uniforme (MTBF 40, MTTR 10), processo fixo",
        "Const(0.5)",
        0.5,
        ("Gamma(shape: 2.0, scale: 20.0)", 40.0),
        ("Uniform(min: 5.0, max: 15.0)", 10.0),
        402,
    );
}

#[test]
fn breakdowns_half_availability() {
    check_breakdown(
        "4c. Quebras Exp/Exp (MTBF 10, MTTR 10), processo Erlang-3",
        "Erlang(k: 3, mean: 0.6)",
        0.6,
        ("Exp(mean: 10.0)", 10.0),
        ("Exp(mean: 10.0)", 10.0),
        403,
    );
}

// ------------------------------------------------------------------- 5. turnos

#[test]
fn shifts_stochastic_saturated_throughput() {
    // Período 40, janelas fora de turno [10, 16) e [30, 34): 10/40 = 25% do tempo parada.
    // Máquina saturada com serviço Exp(0.5): vazão (1 - 0.25) / 0.5 = 1.5.
    // O aquecimento de 40 e o fim em 4040 deixam um número inteiro de ciclos medidos.
    let (stop, warmup) = (4_040.0, 40.0);
    let text = format!(
        r#"Model(
            name: "turnos",
            run: (stop_time: {stop:?}, warmup: {warmup:?}, seed: 501, replications: 20),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1e-4))),
                (name: "m", kind: Processor(process: Exp(mean: 0.5)),
                 shifts: Some((period: 40.0, off: [(start: 10.0, duration: 6.0), (start: 30.0, duration: 4.0)]))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "m"), ("m", "out")],
        )"#
    );
    let reps = reports(&text);
    let mut case = Case::new("5a. Turnos: vazão saturada (25% fora de turno, serviço Exp 0.5)");
    case.ci(
        "fração fora de turno",
        0.25,
        &values(&reps, |r| metric(r, "m.state.off_shift")),
        1e-6,
    );
    case.ci(
        "fração processando",
        0.75,
        &values(&reps, |r| metric(r, "m.state.processing")),
        1e-6,
    );
    case.ci(
        "vazão",
        1.5,
        &values(&reps, |r| {
            r.block("out").unwrap().input as f64 / (stop - warmup)
        }),
        0.003,
    );
    case.finish();
}

#[test]
fn shifts_deterministic_exact_count() {
    // Mesmo calendário, serviço fixo 0.7, 10 ciclos de 40 (0 a 400). Uptime por ciclo 30,
    // total 300; cada item consome 0.7 de uptime, retomando após cada janela:
    // floor(300 / 0.7) = 428 itens, e fração fora de turno exatamente 0.25.
    let text = r#"Model(
        name: "turnos-det",
        run: (stop_time: 400.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1e-4))),
            (name: "m", kind: Processor(process: Const(0.7)),
             shifts: Some((period: 40.0, off: [(start: 10.0, duration: 6.0), (start: 30.0, duration: 4.0)]))),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "m"), ("m", "out")],
    )"#;
    let r = reports(text).remove(0);
    let mut case = Case::new("5b. Turnos: contagem exata (serviço fixo 0.7, 10 ciclos)");
    case.exact(
        "itens produzidos",
        428.0,
        r.block("out").unwrap().input as f64,
        0.0,
    );
    case.exact(
        "fração fora de turno",
        0.25,
        metric(&r, "m.state.off_shift"),
        1e-9,
    );
    case.finish();
}

// ----------------------------------------------------------- 6. transportadores

/// Tempo de um trecho de comprimento `d` com velocidade máxima `v`, aceleração `a` e
/// desaceleração `dec`: perfil trapezoidal se couberem as rampas, triangular caso contrário.
fn leg_time(d: f64, v: f64, a: f64, dec: f64) -> f64 {
    let k = 1.0 / a + 1.0 / dec;
    if d >= v * v / 2.0 * k {
        d / v + v / 2.0 * k
    } else {
        (2.0 * d * k).sqrt()
    }
}

#[test]
fn trapezoid_formula_matches_hand_values() {
    // Mesmos valores calculados à mão em tests/transport.rs.
    assert!((leg_time(5.0, 2.0, 1.0, 1.0) - 4.5).abs() < 1e-12);
    assert!((leg_time(10.0, 2.0, 1.0, 2.0) - 6.5).abs() < 1e-12);
    assert!((leg_time(2.0, 2.0, 1.0, 1.0) - 2.0 * 2f64.sqrt()).abs() < 1e-12);
}

const FORKLIFT: &str = r#"Transporter(speed: 2.0, acceleration: 1.0, deceleration: 0.5, load_time: Const(1.5), unload_time: Const(1.0))"#;

#[test]
fn transporter_fixed_route_utilisation() {
    // Um item a cada 40 em q = (0,0,0); destino a 12 de distância; empilhadeira de casa em q.
    // Cada trecho: 12/2 + 1·(1/1 + 1/0.5) = 9. Ciclo = carga 1.5 + 9 + descarga 1 + 9 (volta) = 20.5.
    // Em 100 ciclos (4000): utilização = 20.5/40 = 0.5125, por estado: 1.5/40, 9/40, 1/40, 9/40.
    let homed = FORKLIFT.replace("Transporter(", "Transporter(home: true, ");
    let text = format!(
        r#"Model(
            name: "rota",
            run: (stop_time: 4000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(40.0), first_at_zero: true)),
                (name: "q", kind: Queue(), transport: Some("f"), position: (0.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (12.0, 0.0, 0.0)),
                (name: "f", kind: {homed}, position: (0.0, 0.0, 0.0)),
            ],
            connections: [("src", "q"), ("q", "out")],
        )"#
    );
    let r = reports(&text).remove(0);
    let leg = leg_time(12.0, 2.0, 1.0, 0.5);
    let cycle = 1.5 + leg + 1.0 + leg;
    let mut case = Case::new("6a. Transportador: rota fixa, perfil trapezoidal (ciclo 20.5)");
    case.exact("tempo de um trecho", 9.0, leg, 1e-12);
    case.exact(
        "itens entregues",
        100.0,
        r.block("out").unwrap().input as f64,
        0.0,
    );
    case.exact("loading", 1.5 / 40.0, metric(&r, "f.state.loading"), 1e-9);
    case.exact(
        "travel_loaded",
        leg / 40.0,
        metric(&r, "f.state.travel_loaded"),
        1e-9,
    );
    case.exact(
        "unloading",
        1.0 / 40.0,
        metric(&r, "f.state.unloading"),
        1e-9,
    );
    case.exact(
        "travel_empty",
        leg / 40.0,
        metric(&r, "f.state.travel_empty"),
        1e-9,
    );
    let busy: f64 = ["loading", "travel_loaded", "unloading", "travel_empty"]
        .iter()
        .map(|s| metric(&r, &format!("f.state.{s}")))
        .sum();
    case.exact("utilização = ciclo/intervalo", cycle / 40.0, busy, 1e-9);
    case.finish();
}

#[test]
fn transporter_poisson_load_is_an_md1_server() {
    // Chegadas Poisson λ = 0.03 em q = (0,0,0); a empilhadeira começa e termina cada viagem
    // no destino (12,0,0): todo serviço custa R = ida vazia + carga + viagem + descarga = 20.5
    // (determinístico), FIFO. M/D/1: ρ = λR, Wq = λR²/(2(1-ρ)), W = Wq + R.
    let (lambda, r_time): (f64, f64) = (0.03, 20.5);
    let rho = lambda * r_time;
    let wq = lambda * r_time * r_time / (2.0 * (1.0 - rho));
    let text = format!(
        r#"Model(
            name: "md1-transp",
            run: (stop_time: 300000.0, warmup: 5000.0, seed: 601, replications: 20),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: {:?}))),
                (name: "q", kind: Queue(), transport: Some("f"), position: (0.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (12.0, 0.0, 0.0)),
                (name: "f", kind: {FORKLIFT}, position: (12.0, 0.0, 0.0)),
            ],
            connections: [("src", "q"), ("q", "out")],
        )"#,
        1.0 / lambda
    );
    let reps = reports(&text);
    let mut case = Case::new("6b. Transportador: carga Poisson = servidor M/D/1 (R = 20.5)");
    case.ci(
        "utilização ρ = λR",
        rho,
        &values(&reps, |r| {
            ["loading", "travel_loaded", "unloading", "travel_empty"]
                .iter()
                .map(|s| metric(r, &format!("f.state.{s}")))
                .sum()
        }),
        0.005,
    );
    case.ci(
        "tempo no sistema W = Wq + R",
        wq + r_time,
        &values(&reps, |r| metric(r, "out.time_in_system.mean")),
        0.01,
    );
    case.finish();
}

// ------------------------------------------------------------------ 7. esteira

fn belt(length: f64, speed: f64, item: f64, accumulating: bool) -> String {
    format!(
        "Conveyor(length: {length:?}, speed: {speed:?}, item_length: {item:?}, accumulating: {accumulating})"
    )
}

#[test]
fn conveyor_free_flow_transit_time() {
    // Trânsito livre: L/v = 30/1.5 = 20, com espaçamento sempre acima de s/v = 0.4.
    for acc in [true, false] {
        let text = format!(
            r#"Model(
                name: "esteira-livre",
                run: (stop_time: 20000.0, warmup: 100.0, seed: 701),
                objects: [
                    (name: "src", kind: Source(interarrival: Uniform(min: 1.0, max: 9.0))),
                    (name: "belt", kind: {}),
                    (name: "out", kind: Sink),
                ],
                connections: [("src", "belt"), ("belt", "out")],
            )"#,
            belt(30.0, 1.5, 0.6, acc)
        );
        let r = reports(&text).remove(0);
        let t = &r.block("out").unwrap().extra["time_in_system"];
        let name: &'static str = if acc {
            "7a. Esteira acumuladora: trânsito livre = L/v"
        } else {
            "7b. Esteira não acumuladora: trânsito livre = L/v"
        };
        let mut case = Case::new(name);
        case.exact("tempo médio de trânsito", 20.0, t.mean, 1e-9);
        case.exact("trânsito mínimo", 20.0, t.min, 1e-9);
        case.exact("trânsito máximo", 20.0, t.max, 1e-9);
        case.finish();
    }
}

#[test]
fn conveyor_saturated_throughput_is_speed_over_item_length() {
    // Fonte sem espera: as peças entram a cada s/v = 0.4; vazão v/s = 2.5 por unidade de tempo.
    // Janela [100, 1100]: 2500 saídas (±1 de borda).
    for acc in [true, false] {
        let text = format!(
            r#"Model(
                name: "esteira-saturada",
                run: (stop_time: 1100.0, warmup: 100.0, seed: 702),
                objects: [
                    (name: "src", kind: Source(interarrival: Const(1e-4))),
                    (name: "belt", kind: {}),
                    (name: "out", kind: Sink),
                ],
                connections: [("src", "belt"), ("belt", "out")],
            )"#,
            belt(30.0, 1.5, 0.6, acc)
        );
        let r = reports(&text).remove(0);
        let name: &'static str = if acc {
            "7c. Esteira acumuladora saturada: vazão = v/s"
        } else {
            "7d. Esteira não acumuladora saturada: vazão = v/s"
        };
        let mut case = Case::new(name);
        case.exact(
            "saídas em 1000 unidades",
            2500.0,
            r.block("out").unwrap().input as f64,
            1.0,
        );
        case.finish();
    }
}

#[test]
fn conveyor_feeding_a_slow_machine_is_limited_by_the_machine() {
    // Esteira (vazão máxima 2.5) alimentando uma máquina Exp(média m = 2), com fonte saturada.
    // Quando a peça da frente sai, a seguinte está um passo s atrás e leva g = s/v = 0.4 para
    // chegar à ponta; se o serviço terminar antes disso, a máquina espera. O ciclo é
    // E[max(S, g)] = g + m·e^(-g/m) = 0.4 + 2·e^(-0.2) = 2.03746, nos dois modos de esteira
    // (na não acumuladora a esteira inteira para, mas o passo entre as peças é o mesmo).
    // Vazão = 1/ciclo e utilização da máquina = m/ciclo.
    let (m_s, g): (f64, f64) = (2.0, 0.6 / 1.5);
    let cycle = g + m_s * (-g / m_s).exp();
    for acc in [true, false] {
        let text = format!(
            r#"Model(
                name: "esteira-gargalo",
                run: (stop_time: 40000.0, warmup: 1000.0, seed: 703, replications: 20),
                objects: [
                    (name: "src", kind: Source(interarrival: Const(1e-4))),
                    (name: "belt", kind: {}),
                    (name: "m", kind: Processor(process: Exp(mean: 2.0))),
                    (name: "out", kind: Sink),
                ],
                connections: [("src", "belt"), ("belt", "m"), ("m", "out")],
            )"#,
            belt(30.0, 1.5, 0.6, acc)
        );
        let reps = reports(&text);
        let name: &'static str = if acc {
            "7e. Esteira acumuladora + máquina Exp(2): ciclo E[max(S, s/v)]"
        } else {
            "7f. Esteira não acumuladora + máquina Exp(2): ciclo E[max(S, s/v)]"
        };
        let mut case = Case::new(name);
        case.ci(
            "vazão = 1/(g + m e^(-g/m))",
            1.0 / cycle,
            &values(&reps, |r| r.block("out").unwrap().input as f64 / 39_000.0),
            0.003,
        );
        case.ci(
            "utilização da máquina = m/ciclo",
            m_s / cycle,
            &values(&reps, |r| metric(r, "m.state.processing")),
            0.003,
        );
        case.finish();
    }
}

// ---------------------------------------------------- 8. transferência por distância

/// Média no tempo do número de itens em trânsito entre `warmup` e `stop`, pelo rastro.
fn in_transit_mean(trace: &[utsim_core::TraceRecord], warmup: f64, stop: f64) -> f64 {
    let mut travelling = HashSet::new();
    let (mut integral, mut last, mut count) = (0.0, warmup, 0i64);
    for rec in trace {
        let delta = match rec.event {
            TraceEvent::Transit { item, .. } => {
                travelling.insert(item);
                1
            }
            TraceEvent::Moved { item, .. } if travelling.remove(&item) => -1,
            _ => continue,
        };
        if rec.t > warmup {
            let t = rec.t.min(stop);
            integral += count as f64 * (t - last);
            last = t;
        }
        count += delta;
    }
    integral += count as f64 * (stop - last);
    integral / (stop - warmup)
}

#[test]
fn transfer_by_distance_adds_delay_and_littles_law_holds() {
    // Fonte Poisson λ = 0.2 em (0,0,0) -> fila em (18,24,0) (distância 30) -> máquina Exp(3).
    // Transferência: atraso 1.5 + 30/2 = 16.5. O atraso fixo não altera o processo de
    // chegadas, então a fila a jusante é M/M/1 (ρ = 0.6): W = 16.5 + 1/(μ-λ) = 24.
    // Little no sistema todo: L = λW = 4.8 = 3.3 em trânsito (λ·16.5) + 1.5 (M/M/1).
    let (stop, warmup) = (30_000.0, 2_000.0);
    let text = format!(
        r#"Model(
            name: "transfer-mm1",
            run: (stop_time: {stop:?}, warmup: {warmup:?}, seed: 801, replications: 12),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 5.0)),
                 transfer: Some((speed: 2.0, delay: 1.5))),
                (name: "fila", kind: Queue(), position: (18.0, 24.0, 0.0)),
                (name: "m", kind: Processor(process: Exp(mean: 3.0)), position: (18.0, 24.0, 0.0)),
                (name: "out", kind: Sink, position: (18.0, 24.0, 0.0)),
            ],
            connections: [("src", "fila"), ("fila", "m"), ("m", "out")],
        )"#
    );
    let m = model(&text);
    let mut reps = Vec::new();
    let mut flying = Vec::new();
    for k in 0..12 {
        let mut sim = m.build(k);
        sim.enable_trace();
        reps.push(m.drive(&mut sim).expect("run ok"));
        flying.push(in_transit_mean(&sim.take_trace(), warmup, stop));
    }
    let span = stop - warmup;
    let mut case =
        Case::new("8. Transferência por distância: atraso + d/v, Little com itens em trânsito");
    case.ci(
        "tempo no sistema W = 16.5 + 7.5",
        24.0,
        &values(&reps, |r| metric(r, "out.time_in_system.mean")),
        0.005,
    );
    case.ci("itens em trânsito = λ·16.5", 3.3, &flying, 0.005);
    case.ci(
        "L total (blocos + trânsito)",
        4.8,
        &reps
            .iter()
            .zip(&flying)
            .map(|(r, f)| {
                ["src", "fila", "m", "out"]
                    .iter()
                    .map(|n| r.block(n).unwrap().content_mean)
                    .sum::<f64>()
                    + f
            })
            .collect::<Vec<_>>(),
        0.005,
    );
    case.ci(
        "Little: L total / (λ_medido · W_medido)",
        1.0,
        &reps
            .iter()
            .zip(&flying)
            .map(|(r, f)| {
                let l: f64 = ["src", "fila", "m", "out"]
                    .iter()
                    .map(|n| r.block(n).unwrap().content_mean)
                    .sum::<f64>()
                    + f;
                let lambda = r.block("out").unwrap().input as f64 / span;
                l / (lambda * metric(r, "out.time_in_system.mean"))
            })
            .collect::<Vec<_>>(),
        0.005,
    );
    case.finish();
}

// ------------------------------------------------------------------ 9. combinador

#[test]
fn combiner_batch_throughput_limited_by_components() {
    // Contêineres sempre disponíveis; componentes Poisson λ = 1; lote de 4. A montagem leva 2
    // (capacidade 0.5 lote/unidade > λ/n = 0.25), então a vazão de lotes é λ/n e a máquina
    // fica processando (λ/n)·2 = 50% do tempo.
    let (stop, warmup) = (40_000.0, 1_000.0);
    let text = format!(
        r#"Model(
            name: "lotes",
            run: (stop_time: {stop:?}, warmup: {warmup:?}, seed: 901, replications: 20),
            objects: [
                (name: "conteineres", kind: Source(interarrival: Const(1e-4))),
                (name: "pecas", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue()),
                (name: "mont", kind: Combiner(components: [4], mode: Pack, process: Const(2.0))),
                (name: "out", kind: Sink),
            ],
            connections: [
                ("conteineres", "mont"), ("pecas", "fila"), ("fila", "mont"), ("mont", "out"),
            ],
        )"#
    );
    let reps = reports(&text);
    let mut case = Case::new("9a. Combinador: lote de 4 componentes Poisson λ=1");
    case.ci(
        "lotes por unidade de tempo = λ/n",
        0.25,
        &values(&reps, |r| {
            r.block("out").unwrap().input as f64 / (stop - warmup)
        }),
        0.003,
    );
    case.ci(
        "fração processando = (λ/n)·2",
        0.5,
        &values(&reps, |r| metric(r, "mont.state.processing")),
        0.003,
    );
    case.finish();
}

#[test]
fn combiner_batch_throughput_limited_by_assembly() {
    // Componentes e contêineres sempre disponíveis: ciclo = preparo 0.5 + processo Exp(2) = 2.5,
    // vazão 1/2.5 = 0.4 lote por unidade de tempo.
    let (stop, warmup) = (40_000.0, 1_000.0);
    let text = format!(
        r#"Model(
            name: "lotes-saturados",
            run: (stop_time: {stop:?}, warmup: {warmup:?}, seed: 902, replications: 20),
            objects: [
                (name: "conteineres", kind: Source(interarrival: Const(1e-4))),
                (name: "pecas", kind: Source(interarrival: Const(1e-4))),
                (name: "mont", kind: Combiner(components: [3], mode: Join, setup: Some(Const(0.5)), process: Exp(mean: 2.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("conteineres", "mont"), ("pecas", "mont"), ("mont", "out")],
        )"#
    );
    let reps = reports(&text);
    let mut case = Case::new("9b. Combinador saturado: ciclo = preparo 0.5 + Exp(2)");
    case.ci(
        "lotes por unidade de tempo = 1/2.5",
        0.4,
        &values(&reps, |r| {
            r.block("out").unwrap().input as f64 / (stop - warmup)
        }),
        0.003,
    );
    case.ci(
        "fração em preparo = 0.5/2.5",
        0.2,
        &values(&reps, |r| metric(r, "mont.state.setup")),
        0.003,
    );
    case.ci(
        "fração processando = 2/2.5",
        0.8,
        &values(&reps, |r| metric(r, "mont.state.processing")),
        0.003,
    );
    case.finish();
}

// ------------------------------------------------------------------- 10. fluidos

fn fluid_run(stop: f64, warmup: f64, fluids: &str) -> HashMap<String, f64> {
    let text = format!(
        r#"Model(
            name: "fluidos",
            run: (stop_time: {stop:?}, warmup: {warmup:?}),
            objects: [
                (name: "fonte0", kind: Source(interarrival: Const(1.0), max_arrivals: Some(0))),
                (name: "fim0", kind: Sink),
            ],
            connections: [("fonte0", "fim0")],
            fluids: Some((name: "fluidos", {fluids})),
        )"#
    );
    reports(&text).remove(0).metrics().into_iter().collect()
}

const FILL: &str = r#"
    nodes: [
        (name: "fonte", kind: Source(rate: 3.0)),
        (name: "tanque", kind: Tank(capacity: 12.0, initial: 2.0)),
        (name: "tubo", kind: Pipe(max_rate: 1.0)),
        (name: "ralo", kind: Sink()),
    ],
    links: [("fonte", "tanque"), ("tanque", "tubo"), ("tubo", "ralo")],
"#;

const DRAIN: &str = r#"
    nodes: [
        (name: "fonte", kind: Source(rate: 1.0)),
        (name: "tanque", kind: Tank(capacity: 20.0, initial: 8.0)),
        (name: "tubo", kind: Pipe(max_rate: 2.5)),
        (name: "ralo", kind: Sink()),
    ],
    links: [("fonte", "tanque"), ("tanque", "tubo"), ("tubo", "ralo")],
"#;

const CASCADE: &str = r#"
    nodes: [
        (name: "t1", kind: Tank(capacity: 10.0, initial: 6.0, max_out: Some(2.0))),
        (name: "t2", kind: Tank(capacity: 4.0, max_out: Some(1.0))),
        (name: "ralo", kind: Sink()),
    ],
    links: [("t1", "t2"), ("t2", "ralo")],
"#;

#[test]
fn fluid_tank_fill_time_is_exact() {
    // Fonte 3/h, ralo máx. 1/h: o tanque (cap. 12, inicial 2) sobe 2/h e enche em t = 5.
    // Nível(t) = 2 + 2t até t = 5, depois fica em 12 e a fonte é estrangulada para 1/h.
    let mut case = Case::new("10a. Tanque enchendo: nível exato antes e depois do evento (t=5)");
    for (stop, level) in [(4.9, 11.8), (5.0, 12.0), (5.1, 12.0), (9.0, 12.0)] {
        let m = fluid_run(stop, 0.0, FILL);
        case.exact(
            &format!("nível em t={stop}"),
            level,
            m["fluidos.tanque.nivel_final"],
            1e-9,
        );
    }
    let m = fluid_run(9.0, 0.0, FILL);
    // Média: (integral de 2+2t em [0,5] = 35, mais 12·4) / 9. Volumes: fonte 3·5 + 1·4 = 19.
    case.exact(
        "nível médio em t=9",
        83.0 / 9.0,
        m["fluidos.tanque.nivel_medio"],
        1e-9,
    );
    case.exact(
        "volume da fonte em t=9",
        19.0,
        m["fluidos.fonte.volume"],
        1e-9,
    );
    case.exact("volume no ralo em t=9", 9.0, m["fluidos.ralo.volume"], 1e-9);
    case.finish();
}

#[test]
fn fluid_tank_empty_time_is_exact() {
    // Inicial 8, fonte 1/h, tubo 2.5/h: desce 1.5/h e esvazia em t = 16/3.
    // Depois a saída é limitada pela entrada (1/h) e o nível fica em 0.
    let t_empty = 16.0 / 3.0;
    let mut case =
        Case::new("10b. Tanque esvaziando: nível exato antes e depois do evento (t=16/3)");
    for (stop, level) in [(5.0, 8.0 - 1.5 * 5.0), (5.5, 0.0), (10.0, 0.0)] {
        let m = fluid_run(stop, 0.0, DRAIN);
        case.exact(
            &format!("nível em t={stop}"),
            level,
            m["fluidos.tanque.nivel_final"],
            1e-9,
        );
    }
    let m = fluid_run(10.0, 0.0, DRAIN);
    // Média: integral de 8 - 1.5t em [0, 16/3] = 8·16/3 - 0.75·(16/3)² = 64/3, dividida por 10.
    case.exact(
        "nível médio em t=10",
        (8.0 * t_empty - 0.75 * t_empty * t_empty) / 10.0,
        m["fluidos.tanque.nivel_medio"],
        1e-9,
    );
    // Conservação: 8 iniciais + 1·10 entregues ao ralo.
    case.exact("volume no ralo", 18.0, m["fluidos.ralo.volume"], 1e-9);
    case.finish();
}

#[test]
fn fluid_tank_cascade_events_are_exact() {
    // t1 (6, saída máx. 2/h) -> t2 (cap. 4, saída máx. 1/h) -> ralo.
    // t1 esvazia em t = 3 (6/2); t2 sobe 1/h até 3, depois desce 1/h e esvazia em t = 6.
    let mut case = Case::new("10c. Tanques em cascata: eventos em t=3 e t=6");
    let expect = [
        (2.9, 0.2, 2.9),
        (3.0, 0.0, 3.0),
        (3.1, 0.0, 2.9),
        (5.9, 0.0, 0.1),
        (6.0, 0.0, 0.0),
        (7.0, 0.0, 0.0),
    ];
    for (stop, l1, l2) in expect {
        let m = fluid_run(stop, 0.0, CASCADE);
        case.exact(
            &format!("t1 em t={stop}"),
            l1,
            m["fluidos.t1.nivel_final"],
            1e-9,
        );
        case.exact(
            &format!("t2 em t={stop}"),
            l2,
            m["fluidos.t2.nivel_final"],
            1e-9,
        );
    }
    let m = fluid_run(10.0, 0.0, CASCADE);
    case.exact("volume total no ralo", 6.0, m["fluidos.ralo.volume"], 1e-9);
    // Média de t2: sobe de 0 a 3 (área 4.5) e desce de 3 a 0 (área 4.5): 9/10.
    case.exact("nível médio de t2", 0.9, m["fluidos.t2.nivel_medio"], 1e-9);
    case.finish();
}

// ------------------------------------------------- 11. perda de Erlang (extra)

/// Fórmula B de Erlang para c servidores e carga a (recorrência estável).
fn erlang_b(c: u32, a: f64) -> f64 {
    (1..=c).fold(1.0, |b, k| a * b / (f64::from(k) + a * b))
}

fn check_erlang_loss(name: &'static str, process: &str, seed: u64) {
    // Sistema de perda M/G/c/c: λ = 1, E[S] = 2.4, c = 3, a = 2.4. A probabilidade de perda
    // é a de Erlang B para qualquer distribuição de serviço (insensibilidade).
    let (stop, warmup) = (30_000.0, 1_000.0);
    let text = format!(
        r#"Model(
            name: "erlang-b",
            run: (stop_time: {stop:?}, warmup: {warmup:?}, seed: {seed}, replications: 20),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 1.0), on_blocked: Drop)),
                (name: "m", kind: Processor(capacity: 3, process: {process})),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "m"), ("m", "out")],
        )"#
    );
    let reps = reports(&text);
    let (a, b) = (2.4, erlang_b(3, 2.4));
    let mut case = Case::new(name);
    case.ci(
        "probabilidade de perda (Erlang B)",
        b,
        &values(&reps, |r| {
            let s = r.block("src").unwrap();
            let lost = s.extra.get("lost").map_or(0, |t| t.count) as f64;
            lost / (lost + s.output as f64)
        }),
        0.01,
    );
    case.ci(
        "servidores ocupados em média a(1-B)",
        a * (1.0 - b),
        &values(&reps, |r| metric(r, "m.content.mean")),
        0.005,
    );
    case.finish();
}

#[test]
fn erlang_loss_exponential_service() {
    check_erlang_loss(
        "11a. Perda de Erlang M/M/3/3 (a=2.4)",
        "Exp(mean: 2.4)",
        1101,
    );
}

#[test]
fn erlang_loss_is_insensitive_to_the_service_distribution() {
    check_erlang_loss("11b. Perda de Erlang M/D/3/3 (a=2.4)", "Const(2.4)", 1102);
}
