//! Arrivals and teams (`docs/plano_chegadas_e_equipes_3d.md`): request priorities,
//! arrival tables, batches, item types, dispatch rules, preemption, home and break
//! places, team metrics. Small models with hand-computed answers.

use utsim_core::{RunReport, TraceEvent, TraceRecord};
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn run(text: &str) -> RunReport {
    model(text).run_replications().unwrap().remove(0)
}

/// Runs replication 0 with the trace on; returns the trace and the block names.
fn traced(text: &str) -> (Vec<TraceRecord>, Vec<String>) {
    let m = model(text);
    let mut sim = m.build(0);
    sim.enable_trace();
    sim.run().unwrap();
    let names = m.objects.iter().map(|o| o.name.clone()).collect();
    (sim.take_trace(), names)
}

/// Names of the blocks items moved into, in order, keeping only `wanted`.
fn deliveries(trace: &[TraceRecord], names: &[String], wanted: &[&str]) -> Vec<String> {
    trace
        .iter()
        .filter_map(|r| match &r.event {
            TraceEvent::Moved { to, .. } if wanted.contains(&names[*to].as_str()) => {
                Some(names[*to].clone())
            }
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------- E2 priorities

/// Two queues share one forklift through a dispatcher; all six items are there at t = 0.
fn two_queues(extra_a: &str, extra_b: &str, op: &str) -> String {
    format!(
        r#"Model(
            name: "prioridades",
            run: (stop_time: 1000.0),
            objects: [
                (name: "srcA", kind: Source(interarrival: Const(0.0), first_at_zero: true, max_arrivals: 3)),
                (name: "srcB", kind: Source(interarrival: Const(0.0), first_at_zero: true, max_arrivals: 3)),
                (name: "qA", kind: Queue(), transport: "equipe", position: (0.0, 0.0, 0.0){extra_a}),
                (name: "qB", kind: Queue(), transport: "equipe", position: (0.0, 2.0, 0.0){extra_b}),
                (name: "sA", kind: Sink, position: (10.0, 0.0, 0.0)),
                (name: "sB", kind: Sink, position: (10.0, 2.0, 0.0)),
                (name: "f", kind: Transporter(speed: 1.0), position: (0.0, 1.0, 0.0)),
                (name: "equipe", kind: Dispatcher(team: ["f"]{op})),
            ],
            connections: [("srcA", "qA"), ("srcB", "qB"), ("qA", "sA"), ("qB", "sB")],
        )"#
    )
}

#[test]
fn without_priority_requests_are_fifo() {
    // The sources take turns at t = 0, so the requests alternate.
    let (trace, names) = traced(&two_queues("", "", ""));
    assert_eq!(
        deliveries(&trace, &names, &["sA", "sB"]),
        ["sA", "sB", "sA", "sB", "sA", "sB"]
    );
}

#[test]
fn transport_priority_goes_first_when_both_wait() {
    // The first request (qA, t = 0) finds the forklift free; the other five wait and the
    // three of qB (priority 5) pass the two of qA (priority 0).
    let text = two_queues("", ", transport_priority: 5", "");
    let r = run(&text);
    assert_eq!(r.block("sA").unwrap().input + r.block("sB").unwrap().input, 6);
    let (trace, names) = traced(&text);
    assert_eq!(
        deliveries(&trace, &names, &["sA", "sB"]),
        ["sA", "sB", "sB", "sB", "sA", "sA"]
    );
}

#[test]
fn priority_label_replaces_the_fixed_value() {
    // qB says 5, but its items carry `urg = -1`, which wins: qB now goes after qA.
    let text = two_queues(
        "",
        ", transport_priority: 5, transport_priority_label: \"urg\"",
        "",
    )
    .replace(
        r#"(name: "srcB", kind: Source(interarrival: Const(0.0), first_at_zero: true, max_arrivals: 3)),"#,
        r#"(name: "srcB", kind: Source(interarrival: Const(0.0), first_at_zero: true, max_arrivals: 3, labels: {"urg": Const(-1.0)})),"#,
    );
    let (trace, names) = traced(&text);
    assert_eq!(
        deliveries(&trace, &names, &["sA", "sB"]),
        ["sA", "sA", "sA", "sB", "sB", "sB"]
    );
    // Without the label on the items, the fixed value applies.
    let text = two_queues(
        "",
        ", transport_priority: 5, transport_priority_label: \"urg\"",
        "",
    );
    let (trace, names) = traced(&text);
    assert_eq!(
        deliveries(&trace, &names, &["sA", "sB"]),
        ["sA", "sB", "sB", "sB", "sA", "sA"]
    );
}

#[test]
fn operator_priority_orders_the_team_queue() {
    // Two machines need the single operator; every item is there at t = 0. "mA" works
    // on two items at once (two requests), "mB" on one; the requests come in the order
    // mA, mB, mA. With priority 3 on mA, its second request passes mB's.
    let text = |prio: &str| {
        format!(
            r#"Model(
            name: "operadores",
            run: (stop_time: 1000.0),
            objects: [
                (name: "srcA", kind: Source(interarrival: Const(0.0), first_at_zero: true, max_arrivals: 2)),
                (name: "srcB", kind: Source(interarrival: Const(0.0), first_at_zero: true, max_arrivals: 1)),
                (name: "qA", kind: Queue()),
                (name: "qB", kind: Queue()),
                (name: "mA", kind: Processor(capacity: 2, process: Const(5.0), operators: Some(OperatorUse(team: "equipe"{prio}))), position: (0.0, 0.0, 0.0)),
                (name: "mB", kind: Processor(process: Const(5.0), operators: Some(OperatorUse(team: "equipe"))), position: (0.0, 0.0, 0.0)),
                (name: "sA", kind: Sink),
                (name: "sB", kind: Sink),
                (name: "op", kind: Transporter(speed: 1.0)),
                (name: "equipe", kind: Dispatcher(team: ["op"])),
            ],
            connections: [("srcA", "qA"), ("srcB", "qB"), ("qA", "mA"), ("qB", "mB"), ("mA", "sA"), ("mB", "sB")],
        )"#
        )
    };
    let (trace, names) = traced(&text(""));
    assert_eq!(
        deliveries(&trace, &names, &["sA", "sB"]),
        ["sA", "sB", "sA"]
    );
    let (trace, names) = traced(&text(", priority: 3"));
    assert_eq!(
        deliveries(&trace, &names, &["sA", "sB"]),
        ["sA", "sA", "sB"]
    );
}

#[test]
fn priority_fields_are_validated() {
    let bad = two_queues("", "", "").replace(
        r#"(name: "sA", kind: Sink, position: (10.0, 0.0, 0.0)),"#,
        r#"(name: "sA", kind: Sink, position: (10.0, 0.0, 0.0), transport_priority: 2),"#,
    );
    let err = Model::parse(&bad)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    assert!(err.contains("transport_priority"), "{err}");
}
