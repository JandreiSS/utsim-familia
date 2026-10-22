//! Token flow engine: every expected value below is computed by hand in the comment
//! above the test. A flow is a block named "fluxo"; its statistics are tallies:
//! `<atividade>.espera`, `<recurso>.ocupado` and `<sink>.tempo_no_fluxo`.

use utsim_core::{RunReport, TallySummary, summarize};
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    if let Err(e) = m.validate() {
        panic!("model is invalid: {e}");
    }
    m
}

fn run(text: &str) -> RunReport {
    model(text).run_replications().unwrap().remove(0)
}

/// A tally of the flow block; panics with the available names when absent.
fn tally<'a>(r: &'a RunReport, name: &str) -> &'a TallySummary {
    let b = r.block("fluxo").unwrap();
    b.extra
        .get(name)
        .unwrap_or_else(|| panic!("sem '{name}'; existem {:?}", b.extra.keys()))
}

fn count(r: &RunReport, name: &str) -> u64 {
    r.block("fluxo")
        .unwrap()
        .extra
        .get(name)
        .map_or(0, |t| t.count)
}

fn near(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: obtido {got}, esperado {want}"
    );
}

fn assert_close(what: &str, got: f64, want: f64, rel_tol: f64) {
    let rel = (got - want).abs() / want.abs().max(1e-12);
    assert!(
        rel <= rel_tol,
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
}

/// A model with the given objects/connections and one flow named "fluxo".
fn with_flow(stop: f64, objects: &str, connections: &str, resources: &str, acts: &str) -> String {
    with_run(
        &format!("stop_time: {stop}"),
        objects,
        connections,
        resources,
        acts,
    )
}

fn with_run(run: &str, objects: &str, connections: &str, resources: &str, acts: &str) -> String {
    format!(
        r#"Model(
            name: "teste de fluxo",
            run: ({run}),
            objects: [{objects}],
            connections: [{connections}],
            flows: [(name: "fluxo", resources: [{resources}], activities: [{acts}])],
        )"#
    )
}

/// The model validator wants at least one object, so flow-only models get an idle
/// sink as anchor (finding: a model made only of flows is rejected as "empty").
const ANCHOR: &str = r#"(name: "ancora", kind: Sink)"#;

fn flow_only(stop: f64, resources: &str, acts: &str) -> String {
    with_flow(stop, ANCHOR, "", resources, acts)
}

#[test]
fn source_delay_sink_counts() {
    // Tokens at 0, 2, 4, 6, 8, 10 (first_at_zero, every 2). Each takes exactly 3, so
    // they end at 3, 5, 7, 9, 11, 13: four end by t = 10. Time in flow and Delay wait = 3.
    let r = run(&flow_only(
        10.0,
        "",
        r#"(name: "chega", kind: Source(interarrival: Const(2.0), first_at_zero: true), next: ["espera"]),
           (name: "espera", kind: Delay(time: Const(3.0)), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    assert_eq!(tally(&r, "fim.tempo_no_fluxo").count, 4);
    near("tempo no fluxo", tally(&r, "fim.tempo_no_fluxo").mean, 3.0);
    assert_eq!(tally(&r, "espera.espera").count, 4);
    near("espera", tally(&r, "espera.espera").mean, 3.0);
    // Idle only when nothing is alive: two tokens (t = 8 and 10) are still inside.
    assert!(r.block("fluxo").unwrap().states["processing"] > 0.99);
}

#[test]
fn source_limit_stops_creation() {
    // limit 5, interarrival 1 (first at t = 1): tokens at 1..=5, each delayed 0.5.
    let r = run(&flow_only(
        100.0,
        "",
        r#"(name: "chega", kind: Source(interarrival: Const(1.0), limit: Some(5)), next: ["d"]),
           (name: "d", kind: Delay(time: Const(0.5)), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    assert_eq!(tally(&r, "fim.tempo_no_fluxo").count, 5);
    // Busy 5 x 0.5 = 2.5 of 100 time units.
    near(
        "fração processando",
        r.block("fluxo").unwrap().states["processing"],
        0.025,
    );
}

#[test]
fn schedule_creates_tokens_at_listed_times() {
    // 2 tokens at t = 1 and 3 at t = 4, each delayed 1: 5 tokens end, at 2 (x2) and 5 (x3).
    let r = run(&flow_only(
        10.0,
        "",
        r#"(name: "horarios", kind: Schedule(times: [(1.0, 2), (4.0, 3)]), next: ["d"]),
           (name: "d", kind: Delay(time: Const(1.0)), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    assert_eq!(tally(&r, "fim.tempo_no_fluxo").count, 5);
    near("tempo", tally(&r, "fim.tempo_no_fluxo").mean, 1.0);
    // Stopping at 4.5 only the first two have ended.
    let r = run(&flow_only(
        4.5,
        "",
        r#"(name: "horarios", kind: Schedule(times: [(1.0, 2), (4.0, 3)]), next: ["d"]),
           (name: "d", kind: Delay(time: Const(1.0)), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    assert_eq!(tally(&r, "fim.tempo_no_fluxo").count, 2);
}

#[test]
fn decide_by_probability() {
    // 20000 tokens, weights 7:3 -> P(A) = 0.7. Standard deviation of the fraction is
    // sqrt(0.21 / 20000) = 0.0032; the 0.015 tolerance is more than 4.5 sigma.
    let r = run(&flow_only(
        30000.0,
        "",
        r#"(name: "chega", kind: Source(interarrival: Const(1.0), limit: Some(20000)), next: ["d"]),
           (name: "d", kind: Decide(rule: Probability([7.0, 3.0])), next: ["a", "b"]),
           (name: "a", kind: Sink),
           (name: "b", kind: Sink)"#,
    ));
    let (a, b) = (
        count(&r, "a.tempo_no_fluxo") as f64,
        count(&r, "b.tempo_no_fluxo") as f64,
    );
    near("total", a + b, 20000.0);
    assert!(
        (a / 20000.0 - 0.7).abs() < 0.015,
        "fração de A: {}",
        a / 20000.0
    );
}

#[test]
fn assign_then_decide_by_label() {
    // Two tokens get tipo = 1 (t = 1), three get tipo = 2 (t = 2): branches 1 and 2.
    let r = run(&flow_only(
        10.0,
        "",
        r#"(name: "s1", kind: Schedule(times: [(1.0, 2)]), next: ["a1"]),
           (name: "s2", kind: Schedule(times: [(2.0, 3)]), next: ["a2"]),
           (name: "a1", kind: Assign(labels: {"tipo": "1"}), next: ["d"]),
           (name: "a2", kind: Assign(labels: {"tipo": "2"}), next: ["d"]),
           (name: "d", kind: Decide(rule: ByLabel("tipo")), next: ["x", "y"]),
           (name: "x", kind: Sink),
           (name: "y", kind: Sink)"#,
    ));
    assert_eq!(count(&r, "x.tempo_no_fluxo"), 2);
    assert_eq!(count(&r, "y.tempo_no_fluxo"), 3);
}

#[test]
fn decide_by_condition_first_true_wins_and_none_goes_last() {
    // Conditions [token.a, token.b, token.b] over three groups of tokens:
    //  - a = 1, b = 7 (4 tokens): first is true       -> branch 1
    //  - a = 0, b = 7 (2 tokens): second is true      -> branch 2
    //  - a = 0, b = 0 (3 tokens): none is true -> the last branch (3)
    let r = run(&flow_only(
        10.0,
        "",
        r#"(name: "g1", kind: Schedule(times: [(1.0, 4)]), next: ["p1"]),
           (name: "g2", kind: Schedule(times: [(1.0, 2)]), next: ["p2"]),
           (name: "g3", kind: Schedule(times: [(1.0, 3)]), next: ["p3"]),
           (name: "p1", kind: Assign(labels: {"a": "1", "b": "7"}), next: ["d"]),
           (name: "p2", kind: Assign(labels: {"a": "0", "b": "7"}), next: ["d"]),
           (name: "p3", kind: Assign(labels: {"a": "0", "b": "0"}), next: ["d"]),
           (name: "d", kind: Decide(rule: Condition(["token.a", "token.b", "token.b"])),
            next: ["r1", "r2", "r3"]),
           (name: "r1", kind: Sink),
           (name: "r2", kind: Sink),
           (name: "r3", kind: Sink)"#,
    ));
    assert_eq!(count(&r, "r1.tempo_no_fluxo"), 4);
    assert_eq!(count(&r, "r2.tempo_no_fluxo"), 2);
    assert_eq!(count(&r, "r3.tempo_no_fluxo"), 3);
}

#[test]
fn acquire_release_third_token_waits_for_the_first() {
    // Resource of capacity 2; three tokens arrive at t = 0 and hold a unit for 5.
    //  - tokens 1 and 2 acquire at 0 (wait 0), release at 5
    //  - token 3 waits 0 -> 5 (exactly 5), holds 5 -> 10
    // Acquire wait: 0, 0, 5 -> mean 5/3, max 5. Time in flow: 5, 5, 10 -> mean 20/3.
    // Holding time: 5, 5, 5.
    let r = run(&flow_only(
        20.0,
        r#"(name: "servidor", capacity: 2)"#,
        r#"(name: "chega", kind: Schedule(times: [(0.0, 3)]), next: ["pega"]),
           (name: "pega", kind: Acquire(resource: "servidor"), next: ["usa"]),
           (name: "usa", kind: Delay(time: Const(5.0)), next: ["solta"]),
           (name: "solta", kind: Release(resource: "servidor"), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    let w = tally(&r, "pega.espera");
    assert_eq!(w.count, 3);
    near("espera média", w.mean, 5.0 / 3.0);
    near("espera máxima", w.max, 5.0);
    near("espera mínima", w.min, 0.0);
    let f = tally(&r, "fim.tempo_no_fluxo");
    assert_eq!(f.count, 3);
    near("tempo no fluxo", f.mean, 20.0 / 3.0);
    let h = tally(&r, "servidor.ocupado");
    assert_eq!(h.count, 3);
    near("tempo segurando", h.mean, 5.0);
    near("tempo segurando (desvio)", h.std_dev, 0.0);
}

#[test]
fn acquire_is_fifo_with_multi_unit_requests() {
    // Capacity 2. At t = 0: token A asks 2 units (gets them), then B asks 1, C asks 1
    // (both wait). All hold for 4. A releases at 4: B and C both get a unit at 4 and
    // hold until 8. Waits: A = 0, B = 4, C = 4. Different `count` per activity: A and
    // (B, C) come from two schedules at the same instant in this order.
    let r = run(&flow_only(
        20.0,
        r#"(name: "r", capacity: 2)"#,
        r#"(name: "sa", kind: Schedule(times: [(0.0, 1)]), next: ["pa"]),
           (name: "sbc", kind: Schedule(times: [(0.0, 2)]), next: ["pb"]),
           (name: "pa", kind: Acquire(resource: "r", count: 2), next: ["ua"]),
           (name: "pb", kind: Acquire(resource: "r", count: 1), next: ["ub"]),
           (name: "ua", kind: Delay(time: Const(4.0)), next: ["ra"]),
           (name: "ub", kind: Delay(time: Const(4.0)), next: ["rb"]),
           (name: "ra", kind: Release(resource: "r", count: 2), next: ["fim"]),
           (name: "rb", kind: Release(resource: "r", count: 1), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    near("espera de A", tally(&r, "pa.espera").mean, 0.0);
    assert_eq!(tally(&r, "pb.espera").count, 2);
    near("espera de B e C", tally(&r, "pb.espera").mean, 4.0);
    // Time in flow: A = 4, B = C = 8.
    let f = tally(&r, "fim.tempo_no_fluxo");
    assert_eq!(f.count, 3);
    near("tempo no fluxo", f.mean, 20.0 / 3.0);
}

#[test]
fn wait_for_and_on_event_react_to_a_physical_queue() {
    // Source every 1 starting at 0 -> queue "fila" -> long processor. Items enter the
    // queue at t = 0, 1, ..., 10 (11 entries up to the end) and move on at once.
    let objects = r#"
        (name: "chegada", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
        (name: "fila", kind: Queue()),
        (name: "maquina", kind: Processor(capacity: 100, process: Const(1000.0))),
        (name: "saida", kind: Sink)"#;
    let conns = r#"("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")"#;

    // OnEvent: one token per entry -> 11 tokens.
    let r = run(&with_flow(
        10.0,
        objects,
        conns,
        "",
        r#"(name: "evento", kind: OnEvent(block: "fila", event: Entry), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    assert_eq!(tally(&r, "fim.tempo_no_fluxo").count, 11);

    // Same with Exit events.
    let r = run(&with_flow(
        10.0,
        objects,
        conns,
        "",
        r#"(name: "evento", kind: OnEvent(block: "fila", event: Exit), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    assert_eq!(tally(&r, "fim.tempo_no_fluxo").count, 11);

    // WaitFor: 3 tokens appear at t = 0.5 and each takes the next entry (t = 1, 2, 3):
    // waits 0.5, 1.5, 2.5 (mean 1.5). The carried item then gets a label that a
    // condition on the item reads back: all three go to "ok".
    let r = run(&with_flow(
        10.0,
        objects,
        conns,
        "",
        r#"(name: "cria", kind: Schedule(times: [(0.5, 3)]), next: ["espera"]),
           (name: "espera", kind: WaitFor(block: "fila", event: Entry), next: ["marca"]),
           (name: "marca", kind: Assign(labels: {"item.marcado": "1", "visto": "now"}), next: ["d"]),
           (name: "d", kind: Decide(rule: Condition(["item.marcado", "true"])), next: ["ok", "ruim"]),
           (name: "ok", kind: Sink),
           (name: "ruim", kind: Sink)"#,
    ));
    let w = tally(&r, "espera.espera");
    assert_eq!(w.count, 3);
    near("espera média", w.mean, 1.5);
    near("espera máxima", w.max, 2.5);
    assert_eq!(count(&r, "ok.tempo_no_fluxo"), 3);
    assert_eq!(count(&r, "ruim.tempo_no_fluxo"), 0);
}

#[test]
fn create_item_injects_into_a_queue_and_items_reach_the_sink() {
    // 5 tokens (t = 1..=5) each create an item in "fila", which hands it to "saida".
    let r = run(&with_flow(
        20.0,
        r#"(name: "fila", kind: Queue()), (name: "saida", kind: Sink)"#,
        r#"("fila", "saida")"#,
        "",
        r#"(name: "chega", kind: Source(interarrival: Const(1.0), limit: Some(5)), next: ["cria"]),
           (name: "cria", kind: CreateItem(into: "fila", labels: {"origem": "\"fluxo\""}), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    assert_eq!(r.block("fila").unwrap().input, 5);
    assert_eq!(r.block("saida").unwrap().input, 5);
    assert_eq!(count(&r, "fim.tempo_no_fluxo"), 5);
}

#[test]
fn create_item_labels_choose_the_physical_route() {
    // Items made with `item.tipo` set by Assign: tipo 1 (2 items) and 2 (3 items) go
    // to different sinks through the queue's ByLabel rule (ports are 1-based).
    let r = run(&with_flow(
        20.0,
        r#"(name: "fila", kind: Queue(), output: ByLabel("tipo")),
           (name: "s1", kind: Sink), (name: "s2", kind: Sink)"#,
        r#"("fila", "s1"), ("fila", "s2")"#,
        "",
        r#"(name: "g1", kind: Schedule(times: [(1.0, 2)]), next: ["c1"]),
           (name: "g2", kind: Schedule(times: [(2.0, 3)]), next: ["c2"]),
           (name: "c1", kind: CreateItem(into: "fila", labels: {"tipo": "1"}), next: ["fim"]),
           (name: "c2", kind: CreateItem(into: "fila", labels: {"tipo": "2"}), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    assert_eq!(r.block("s1").unwrap().input, 2);
    assert_eq!(r.block("s2").unwrap().input, 3);
}

#[test]
fn batch_of_three_keeps_the_first_token() {
    // 7 tokens at t = 0..=6; batches of 3 complete at t = 2 (tokens born 0, 1, 2) and
    // t = 5 (born 3, 4, 5). Token 6 is left waiting. Batch waits: 2, 1, 0 twice ->
    // 6 observations, mean 1, max 2. Survivors were born at 0 and 3 and end at 2 and 5:
    // time in flow 2 and 2.
    let r = run(&flow_only(
        20.0,
        "",
        r#"(name: "chega", kind: Source(interarrival: Const(1.0), first_at_zero: true, limit: Some(7)), next: ["lote"]),
           (name: "lote", kind: Batch(size: 3), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    let b = tally(&r, "lote.espera");
    assert_eq!(b.count, 6);
    near("espera média no lote", b.mean, 1.0);
    near("espera máxima no lote", b.max, 2.0);
    let f = tally(&r, "fim.tempo_no_fluxo");
    assert_eq!(f.count, 2);
    near("tempo no fluxo dos sobreviventes", f.mean, 2.0);
}

#[test]
fn zero_time_loop_is_reported() {
    let m = model(&flow_only(
        5.0,
        "",
        r#"(name: "chega", kind: Schedule(times: [(1.0, 1)]), next: ["a"]),
           (name: "a", kind: Assign(labels: {"x": "1"}), next: ["b"]),
           (name: "b", kind: Assign(labels: {"x": "2"}), next: ["a"])"#,
    ));
    let err = m.build(0).run().unwrap_err().to_string();
    assert!(err.contains("laço de tempo zero"), "{err}");
}

#[test]
fn invalid_expressions_fail_validation() {
    let text = flow_only(
        5.0,
        "",
        r#"(name: "chega", kind: Schedule(times: [(1.0, 1)]), next: ["a"]),
           (name: "a", kind: Assign(labels: {"x": "1 +"}), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    );
    let err = Model::parse(&text)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("fluxo.a") && err.contains("expressão"),
        "{err}"
    );
}

#[test]
fn mm1_built_only_with_a_flow_matches_queueing_theory() {
    // lambda = 1, mean service 0.8 (mu = 1.25), rho = 0.8:
    // Wq = rho / (mu - lambda) = 0.8 / 0.25 = 3.2, and W = Wq + 0.8 = 4.0.
    let m = model(&with_run(
        "stop_time: 60000.0, warmup: 2000.0, seed: 31, replications: 8",
        ANCHOR,
        "",
        r#"(name: "servidor", capacity: 1)"#,
        r#"(name: "chega", kind: Source(interarrival: Exp(mean: 1.0)), next: ["pega"]),
           (name: "pega", kind: Acquire(resource: "servidor"), next: ["serve"]),
           (name: "serve", kind: Delay(time: Exp(mean: 0.8)), next: ["solta"]),
           (name: "solta", kind: Release(resource: "servidor"), next: ["fim"]),
           (name: "fim", kind: Sink)"#,
    ));
    let s = summarize(&m.run_replications().unwrap());
    println!(
        "Wq = {:.4}, W = {:.4}, ocupado = {:.4}",
        s["fluxo.pega.espera.mean"].mean,
        s["fluxo.fim.tempo_no_fluxo.mean"].mean,
        s["fluxo.servidor.ocupado.mean"].mean
    );
    assert_close("Wq", s["fluxo.pega.espera.mean"].mean, 3.2, 0.08);
    assert_close("W", s["fluxo.fim.tempo_no_fluxo.mean"].mean, 4.0, 0.08);
    // Utilisation = mean holding time x tokens / window; check the holding time = 0.8 + ...
    assert_close(
        "tempo médio segurando",
        s["fluxo.servidor.ocupado.mean"].mean,
        0.8,
        0.05,
    );
}
