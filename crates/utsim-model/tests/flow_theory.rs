//! Validação de modelos construídos com fluxos de tokens e scripts contra a teoria das
//! filas. Cada teste tem a previsão calculada no comentário acima dele.
//!
//! Notação igual à de `queueing_theory.rs`: λ taxa de chegada, μ taxa de serviço,
//! ρ = λ/(cμ), Lq e Wq tamanho e espera na fila, W tempo no sistema.
//!
//! - M/M/c (Erlang C): Wq = Lq/λ com Lq = P_w·ρ/(1-ρ); W = Wq + 1/μ.
//! - Jackson em série com M/M/1: Wq_i = ρ_i/(μ_i - λ), W = Σ 1/(μ_i - λ).
//! - Lei de Little: L = λW. Para um atraso Uniform(a, b), E[atraso] = (a+b)/2.

use utsim_core::{RunReport, TallySummary, TraceEvent, summarize};
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    if let Err(e) = m.validate() {
        panic!("model is invalid: {e}");
    }
    m
}

fn reps(text: &str) -> Vec<RunReport> {
    model(text).run_replications().unwrap()
}

/// Same layout as the flow tests: a flow named "fluxo" plus an anchor Sink (flow-only
/// models are rejected as empty without at least one object).
fn flow_model(run: &str, objects: &str, connections: &str, resources: &str, acts: &str) -> String {
    format!(
        r#"Model(
            name: "teoria com fluxos",
            run: ({run}),
            objects: [{objects}],
            connections: [{connections}],
            flows: [(name: "fluxo", resources: [{resources}], activities: [{acts}])],
        )"#
    )
}

const ANCHOR: &str = r#"(name: "ancora", kind: Sink)"#;

fn tally<'a>(r: &'a RunReport, name: &str) -> &'a TallySummary {
    let b = r.block("fluxo").expect("bloco do fluxo");
    b.extra
        .get(name)
        .unwrap_or_else(|| panic!("sem '{name}'; existem {:?}", b.extra.keys()))
}

fn count(r: &RunReport, name: &str) -> u64 {
    r.block("fluxo")
        .expect("bloco do fluxo")
        .extra
        .get(name)
        .map_or(0, |t| t.count)
}

fn assert_close(what: &str, got: f64, want: f64, rel_tol: f64) {
    let rel = (got - want).abs() / want.abs().max(1e-12);
    println!(
        "{what}: medido {got:.4}, teoria {want:.4}, erro {:.2}% (tolerância {:.0}%)",
        rel * 100.0,
        rel_tol * 100.0
    );
    assert!(
        rel <= rel_tol,
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
}

/// Erlang C: probability that an arrival waits, for c servers and load a = λ/μ (a < c).
fn erlang_c(c: u32, a: f64) -> f64 {
    let rho = a / f64::from(c);
    let mut term = 1.0; // a^0 / 0!
    let mut sum = 0.0;
    for k in 0..c {
        sum += term;
        term *= a / f64::from(k + 1);
    }
    // `term` is now a^c / c!.
    let tail = term / (1.0 - rho);
    tail / (sum + tail)
}

#[test]
fn mmc_flow_matches_erlang_c() {
    // M/M/3 com fluxo: λ = 1, μ = 1/2.4, a = 2.4, ρ = 0.8.
    //  - P_w = erlang_c(3, 2.4), Lq = P_w·ρ/(1-ρ), Wq = Lq/λ, W = Wq + 2.4.
    //  - utilização = mean(ocupado)·count / (janela·3), que deve dar a/c = 0.8.
    let text = flow_model(
        "stop_time: 60000.0, warmup: 2000.0, seed: 41, replications: 8",
        ANCHOR,
        "",
        r#"(name: "servidores", capacity: 3)"#,
        r#"(name: "chega", kind: Source(interarrival: Exp(mean: 1.0)), next: ["pega"]),
           (name: "pega", kind: Acquire(resource: "servidores", count: 1), next: ["atende"]),
           (name: "atende", kind: Delay(time: Exp(mean: 2.4)), next: ["solta"]),
           (name: "solta", kind: Release(resource: "servidores", count: 1), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    );
    let rs = reps(&text);
    let s = summarize(&rs);
    let (lambda, mu, c) = (1.0_f64, 1.0 / 2.4, 3_u32);
    let a = lambda / mu;
    let rho = a / f64::from(c);
    let pw = erlang_c(c, a);
    let lq = pw * rho / (1.0 - rho);
    let wq = lq / lambda;
    let w = wq + 1.0 / mu;

    // Utilisation per replication, from the release tally (count = released chunks).
    let util: f64 = rs
        .iter()
        .map(|r| {
            let t = tally(r, "servidores.ocupado");
            t.mean * t.count as f64 / ((r.end_time - r.stats_from) * f64::from(c))
        })
        .sum::<f64>()
        / rs.len() as f64;

    assert_close(
        "Wq do Acquire (Erlang C)",
        s["fluxo.pega.espera.mean"].mean,
        wq,
        0.08,
    );
    assert_close(
        "W no fluxo",
        s["fluxo.fim.tempo_no_fluxo.mean"].mean,
        w,
        0.05,
    );
    assert_close("utilização dos servidores", util, rho, 0.02);
}

#[test]
fn tandem_flow_with_two_resources_matches_jackson() {
    // Fonte λ = 1, r1 (μ1 = 1/0.7) e depois r2 (μ2 = 1/0.8), cada um com 1 unidade.
    //  - Wq1 = ρ1/(μ1-λ) = 0.7/0.4286 = 1.6333; Wq2 = ρ2/(μ2-λ) = 0.8/0.25 = 3.2
    //  - W total = Σ (Wq_i + 1/μ_i) = 1/(μ1-λ) + 1/(μ2-λ) = 6.3333
    let text = flow_model(
        "stop_time: 60000.0, warmup: 2000.0, seed: 43, replications: 8",
        ANCHOR,
        "",
        r#"(name: "r1", capacity: 1), (name: "r2", capacity: 1)"#,
        r#"(name: "chega", kind: Source(interarrival: Exp(mean: 1.0)), next: ["pega1"]),
           (name: "pega1", kind: Acquire(resource: "r1", count: 1), next: ["usa1"]),
           (name: "usa1", kind: Delay(time: Exp(mean: 0.7)), next: ["solta1"]),
           (name: "solta1", kind: Release(resource: "r1", count: 1), next: ["pega2"]),
           (name: "pega2", kind: Acquire(resource: "r2", count: 1), next: ["usa2"]),
           (name: "usa2", kind: Delay(time: Exp(mean: 0.8)), next: ["solta2"]),
           (name: "solta2", kind: Release(resource: "r2", count: 1), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    );
    let (lambda, mu1, mu2) = (1.0_f64, 1.0 / 0.7, 1.0 / 0.8);
    let wq1 = (lambda / mu1) / (mu1 - lambda);
    let wq2 = (lambda / mu2) / (mu2 - lambda);
    let w = wq1 + 0.7 + wq2 + 0.8;
    assert!((w - (1.0 / (mu1 - lambda) + 1.0 / (mu2 - lambda))).abs() < 1e-9);

    let s = summarize(&reps(&text));
    assert_close("Wq1", s["fluxo.pega1.espera.mean"].mean, wq1, 0.08);
    assert_close("Wq2", s["fluxo.pega2.espera.mean"].mean, wq2, 0.08);
    assert_close("W total", s["fluxo.fim.tempo_no_fluxo.mean"].mean, w, 0.05);
}

/// Probability model, script model: both must route ~30% of tokens to branch "a".
fn routing(rule: &str, stop: f64, seed: u64) -> String {
    flow_model(
        &format!("stop_time: {stop}, warmup: 0.0, seed: {seed}, replications: 1"),
        ANCHOR,
        "",
        "",
        &format!(
            r#"(name: "chega", kind: Source(interarrival: Const(1.0), limit: Some(20000)), next: ["decide"]),
               (name: "decide", kind: Decide(rule: {rule}), next: ["a", "b"]),
               (name: "a", kind: Sink),
               (name: "b", kind: Sink)"#
        ),
    )
}

fn assert_binomial(what: &str, hits: f64, n: f64, p: f64) {
    let frac = hits / n;
    let sd = (p * (1.0 - p) / n).sqrt();
    println!(
        "{what}: fração {frac:.4}, teoria {p:.4}, desvio-padrão {sd:.5}, |erro|/dp = {:.2}",
        (frac - p).abs() / sd
    );
    assert!(
        (frac - p).abs() <= 4.0 * sd,
        "{what}: fração {frac:.4} fora de {p} ± 4 dp ({:.5})",
        4.0 * sd
    );
}

#[test]
fn script_random_routing_matches_probability_rule() {
    let n = 20000.0;
    let script = reps(&routing(
        r#"Condition(["random() < 0.3", "true"])"#,
        30000.0,
        53,
    ));
    let prob = reps(&routing("Probability([3.0, 7.0])", 30000.0, 53));

    let a_script = count(&script[0], "a.tempo_no_fluxo") as f64;
    let a_prob = count(&prob[0], "a.tempo_no_fluxo") as f64;
    println!("contagem do ramo 1: script {a_script}, Probability {a_prob}");
    assert_eq!(
        count(&script[0], "a.tempo_no_fluxo") + count(&script[0], "b.tempo_no_fluxo"),
        20000
    );
    assert_binomial("ramo 1 (script random())", a_script, n, 0.3);
    assert_binomial("ramo 1 (Probability [3,7])", a_prob, n, 0.3);
}

#[test]
fn script_routing_is_reproducible_with_the_same_seed() {
    let text = routing(r#"Condition(["random() < 0.3", "true"])"#, 30000.0, 59);
    let r1 = reps(&text).remove(0);
    let r2 = reps(&text).remove(0);
    let (a1, a2) = (
        count(&r1, "a.tempo_no_fluxo"),
        count(&r2, "a.tempo_no_fluxo"),
    );
    let (b1, b2) = (
        count(&r1, "b.tempo_no_fluxo"),
        count(&r2, "b.tempo_no_fluxo"),
    );
    println!("mesma semente: ramo 1 = {a1} e {a2}, ramo 2 = {b1} e {b2}");
    assert_eq!(a1, a2, "ramo 1 muda entre rodadas com a mesma semente");
    assert_eq!(b1, b2, "ramo 2 muda entre rodadas com a mesma semente");
    assert!(a1 > 0 && b1 > 0);
}

#[test]
fn little_law_on_a_delay_with_uniform_time() {
    // λ = 2 (Exp com média 0.5), atraso Uniform(2, 6): E[atraso] = 4.
    //  - L = λ·W = 2·4 = 8 tokens em média no Delay.
    // Medimos L pelo rastro (integral do número no Delay / T), λ pela contagem de
    // saídas do Delay / T e W pelo tally `espera.espera`; todas devem bater com 8.
    let text = flow_model(
        "stop_time: 20000.0, warmup: 0.0, seed: 61, replications: 1",
        ANCHOR,
        "",
        "",
        r#"(name: "chega", kind: Source(interarrival: Exp(mean: 0.5)), next: ["espera"]),
           (name: "espera", kind: Delay(time: Uniform(min: 2.0, max: 6.0)), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    );
    let rep = model(&text).replay(0).expect("replay");
    let flow = rep
        .blocks
        .iter()
        .position(|b| b.name == "fluxo")
        .expect("bloco do fluxo no rastro");
    let delay_activity = 1; // order in the model: chega (0), espera (1), fim (2)

    let mut area = 0.0;
    let mut last_t = 0.0;
    let mut current = 0u32;
    let mut changes = 0usize;
    for rec in &rep.records {
        if let TraceEvent::Tokens {
            flow: f,
            activity,
            count,
        } = &rec.event
        {
            if *f == flow && *activity == delay_activity {
                area += f64::from(current) * (rec.t - last_t);
                last_t = rec.t;
                current = *count;
                changes += 1;
            }
        }
    }
    area += f64::from(current) * (rep.stop_time - last_t);
    assert!(changes > 0, "nenhum registro Tokens para o Delay");
    let l_trace = area / rep.stop_time;

    let espera = tally(&rep.report, "espera.espera");
    let lambda = espera.count as f64 / rep.stop_time;
    let w = espera.mean;
    println!("λ medido = {lambda:.4} (teoria 2), W medido = {w:.4} (teoria 4)");

    assert_close("λ (saídas do Delay / T)", lambda, 2.0, 0.05);
    assert_close("W (tally espera)", w, 4.0, 0.05);
    assert_close("L pelo rastro (tempo médio no Delay)", l_trace, 8.0, 0.05);
    assert_close("λ·W", lambda * w, 8.0, 0.05);
}

#[test]
fn on_event_entry_counts_exactly_the_queue_input() {
    // Cada entrada na fila física cria exatamente um token: o número de tokens que
    // chegam ao Sink tem de ser igual a `fila.input`, em cada replicação.
    let text = r#"Model(
            name: "fila fisica com fluxo",
            run: (stop_time: 2000.0, warmup: 0.0, seed: 67, replications: 4),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue()),
                (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
            flows: [(name: "fluxo", activities: [
                (name: "evento", kind: OnEvent(block: "fila", event: Entry), next: ["fim"]),
                (name: "fim", kind: Sink),
            ])],
        )"#;
    for r in reps(text) {
        let entradas = r.block("fila").expect("fila").input;
        let tokens = count(&r, "fim.tempo_no_fluxo");
        println!(
            "replicação {}: entradas na fila = {entradas}, tokens no Sink = {tokens}",
            r.replication
        );
        assert!(entradas > 0);
        assert_eq!(tokens, entradas, "replicação {}", r.replication);
    }
}
