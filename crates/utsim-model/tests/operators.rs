//! Processors that need operators (phase 4, front C): exact hand-computed timings and
//! an M/M/1 sanity check with a dedicated operator.
//!
//! All walking uses speed 1 and straight lines, so time = distance. Sources deliver one
//! item at t = 0 (`first_at_zero`) and the run lasts 100, so a state that lasts `d`
//! shows up as the fraction `d / 100`.

use utsim_core::{RunReport, summarize};
use utsim_model::Model;

fn run(text: &str) -> RunReport {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m.run_replications().unwrap().remove(0)
}

fn near(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: obtido {got}, esperado {want}"
    );
}

fn state(r: &RunReport, block: &str, state: &str) -> f64 {
    r.block(block)
        .unwrap()
        .states
        .get(state)
        .copied()
        .unwrap_or(0.0)
}

fn stay(r: &RunReport, block: &str) -> f64 {
    r.block(block).unwrap().staytime.mean
}

/// Two machines 10 apart, one source each (one item at t = 0), one operator that starts
/// at machine 1. `m1`/`m2` are the processor definitions' kind text.
fn two_machines(m1: &str, m2: &str) -> String {
    format!(
        r#"Model(
            name: "celula",
            run: (stop_time: 100.0),
            objects: [
                (name: "s1", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: Some(1))),
                (name: "s2", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: Some(1))),
                (name: "m1", kind: {m1}, position: (0.0, 0.0, 0.0)),
                (name: "m2", kind: {m2}, position: (10.0, 0.0, 0.0)),
                (name: "op", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0)),
                (name: "out", kind: Sink),
            ],
            connections: [("s1", "m1"), ("s2", "m2"), ("m1", "out"), ("m2", "out")],
        )"#
    )
}

#[test]
fn one_operator_walks_between_two_machines() {
    // Process 5 on both, operator needed for the whole process, starting at m1.
    //  t=0   both items enter; m1's request is first, the operator is already there
    //        (distance 0): m1 processes 0..5.
    //  t=5   m1 done, releases; the operator walks 10 to m2 (t=15); m2 processes 15..20.
    let p = r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "op")))"#;
    let r = run(&two_machines(p, p));
    near("m1 staytime", stay(&r, "m1"), 5.0);
    near("m2 staytime", stay(&r, "m2"), 20.0);
    near("m1 processing", state(&r, "m1", "processing"), 0.05);
    near(
        "m2 waiting_operator",
        state(&r, "m2", "waiting_operator"),
        0.15,
    );
    near("m2 processing", state(&r, "m2", "processing"), 0.05);
    near("m1 never waits", state(&r, "m1", "waiting_operator"), 0.0);
    // Operator: utilize 0..5 and 15..20, travel_empty 5..15.
    near("op utilize", state(&r, "op", "utilize"), 0.10);
    near("op travel_empty", state(&r, "op", "travel_empty"), 0.10);
    assert_eq!(r.block("out").unwrap().input, 2);
}

#[test]
fn setup_only_operator_is_released_before_processing() {
    // Setup 3, process 7, operator only for setup.
    //  m1: operator there at 0; setup 0..3, then released; process 3..10 alone.
    //  The operator walks to m2 right at t=3 (not at 10): arrives 13, setup 13..16,
    //  process 16..23. Had it been kept during process it would arrive at 20.
    let p = r#"Processor(setup: Some(Const(3.0)), process: Const(7.0),
        operators: Some(OperatorUse(team: "op", process: false)))"#;
    let r = run(&two_machines(p, p));
    near("m1 staytime", stay(&r, "m1"), 10.0);
    near("m2 staytime", stay(&r, "m2"), 23.0);
    near(
        "m2 waiting_operator",
        state(&r, "m2", "waiting_operator"),
        0.13,
    );
    near("m2 setup", state(&r, "m2", "setup"), 0.03);
    near("m2 processing", state(&r, "m2", "processing"), 0.07);
    near("op utilize", state(&r, "op", "utilize"), 0.06); // 0..3 and 13..16
}

#[test]
fn operator_is_kept_from_setup_to_process() {
    // Same machines, operator for both phases ("same as setup"): m1 setup 0..3,
    // process 3..10 with the same operator, no new request. It reaches m2 at 10 + 10 = 20:
    // setup 20..23, process 23..30.
    let p = r#"Processor(setup: Some(Const(3.0)), process: Const(7.0),
        operators: Some(OperatorUse(team: "op")))"#;
    let r = run(&two_machines(p, p));
    near("m1 staytime", stay(&r, "m1"), 10.0);
    near("m2 staytime", stay(&r, "m2"), 30.0);
    near(
        "m2 waiting_operator",
        state(&r, "m2", "waiting_operator"),
        0.20,
    );
    near("op utilize", state(&r, "op", "utilize"), 0.20); // 0..10 and 20..30
}

#[test]
fn process_only_operator_requested_after_setup() {
    // Setup 3 without operator, process 7 with: the request starts at t=3. The operator
    // is at the station already (distance 0): m1 setup 0..3, process 3..10.
    let p = r#"Processor(setup: Some(Const(3.0)), process: Const(7.0),
        operators: Some(OperatorUse(team: "op", setup: false)))"#;
    let q = r#"Processor(process: Const(1.0))"#;
    let r = run(&two_machines(p, q));
    near("m1 staytime", stay(&r, "m1"), 10.0);
    near("m1 setup", state(&r, "m1", "setup"), 0.03);
    near("m1 processing", state(&r, "m1", "processing"), 0.07);
    near("op utilize", state(&r, "op", "utilize"), 0.07);
}

#[test]
fn two_operators_from_a_dispatcher_team() {
    // Machine at (10,0). p1 at (0,0) is 10 away, p2 at (10,20) is 20 away. count = 2:
    // FirstAvailable gives the first request to p1 and the second to p2. The timer starts
    // when both are there (t=20), process 4 -> done at 24; both are released then.
    let r = run(r#"Model(
            name: "dupla",
            run: (stop_time: 100.0),
            objects: [
                (name: "s", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: Some(1))),
                (name: "m", kind: Processor(process: Const(4.0),
                    operators: Some(OperatorUse(team: "equipe", count: 2))), position: (10.0, 0.0, 0.0)),
                (name: "p1", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0)),
                (name: "p2", kind: Transporter(speed: 1.0), position: (10.0, 20.0, 0.0)),
                (name: "equipe", kind: Dispatcher(team: ["p1", "p2"])),
                (name: "out", kind: Sink),
            ],
            connections: [("s", "m"), ("m", "out")],
        )"#);
    near("staytime", stay(&r, "m"), 24.0);
    near("waiting_operator", state(&r, "m", "waiting_operator"), 0.20);
    near("processing", state(&r, "m", "processing"), 0.04);
    near("p1 travel", state(&r, "p1", "travel_empty"), 0.10);
    near("p1 utilize", state(&r, "p1", "utilize"), 0.14); // 10..24
    near("p2 travel", state(&r, "p2", "travel_empty"), 0.20);
    near("p2 utilize", state(&r, "p2", "utilize"), 0.04); // 20..24
}

#[test]
fn capacity_two_with_one_operator_serves_in_turn() {
    // Machine (capacity 2) at (10,0), operator at (0,0), process 5. Items arrive at t=0
    // and t=1 (interarrival 1).
    //  A: request at 0, operator walks 10 -> processes 10..15, leaves at 15 (stay 15).
    //  B: enters at 1, its request waits behind A's; at 15 the operator is already at the
    //     machine (distance 0) -> processes 15..20, leaves at 20 (stay 19).
    // State: waiting_operator 0..10 (only A waits), processing 10..20.
    let r = run(r#"Model(
            name: "cap2",
            run: (stop_time: 100.0),
            objects: [
                (name: "s", kind: Source(interarrival: Const(1.0), first_at_zero: true, max_arrivals: Some(2))),
                (name: "m", kind: Processor(capacity: 2, process: Const(5.0),
                    operators: Some(OperatorUse(team: "op"))), position: (10.0, 0.0, 0.0)),
                (name: "op", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0)),
                (name: "out", kind: Sink),
            ],
            connections: [("s", "m"), ("m", "out")],
        )"#);
    assert_eq!(r.block("out").unwrap().input, 2);
    near("mean staytime", stay(&r, "m"), 17.0); // (15 + 19) / 2
    near("waiting_operator", state(&r, "m", "waiting_operator"), 0.10);
    near("processing", state(&r, "m", "processing"), 0.10);
    near("op utilize", state(&r, "op", "utilize"), 0.10); // 10..20
}

#[test]
fn dedicated_operator_reproduces_mm1() {
    // λ = 1, mean service 0.8 (μ = 1.25), operator standing at the machine (distance 0):
    // the operator never delays anything, so it is M/M/1: ρ = 0.8, Lq = ρ²/(1-ρ) = 3.2.
    let m = Model::parse(
        r#"Model(
            name: "mm1-operador",
            run: (stop_time: 60000.0, warmup: 2000.0, seed: 51, replications: 8),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue()),
                (name: "maquina", kind: Processor(process: Exp(mean: 0.8),
                    operators: Some(OperatorUse(team: "op")))),
                (name: "op", kind: Transporter(speed: 1.0)),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#,
    )
    .unwrap();
    m.validate().unwrap();
    let s = summarize(&m.run_replications().unwrap());
    let check = |what: &str, got: f64, want: f64, tol: f64| {
        let rel = (got - want).abs() / want;
        assert!(rel <= tol, "{what}: obtido {got:.4}, esperado {want:.4}");
    };
    check("rho", s["maquina.state.processing"].mean, 0.8, 0.02);
    check("rho do operador", s["op.state.utilize"].mean, 0.8, 0.02);
    check("Lq", s["fila.content.mean"].mean, 3.2, 0.08);
    check("Wq", s["fila.staytime.mean"].mean, 3.2, 0.08);
}
