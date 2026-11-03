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
    assert_eq!(
        r.block("sA").unwrap().input + r.block("sB").unwrap().input,
        6
    );
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

// ---------------------------------------------------------------- C1 arrival table

/// A source with `source` fields feeding a sink, or two sinks by the label `tipo`.
fn table_model(source: &str, stop: f64, by_type: bool) -> String {
    let (output, sinks, links) = if by_type {
        (
            r#", output: ByLabel("tipo")"#,
            r#"(name: "s1", kind: Sink), (name: "s2", kind: Sink),"#,
            r#"("src", "s1"), ("src", "s2")"#,
        )
    } else {
        ("", r#"(name: "s1", kind: Sink),"#, r#"("src", "s1")"#)
    };
    format!(
        r#"Model(
            format: 2,
            name: "tabela",
            run: (stop_time: {stop}),
            objects: [
                (name: "src", kind: Source({source}){output}),
                {sinks}
            ],
            connections: [{links}],
        )"#
    )
}

/// Instants at which the source created items.
fn created_at(trace: &[TraceRecord]) -> Vec<f64> {
    trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::Created { block: 0, .. }))
        .map(|r| r.t)
        .collect()
}

#[test]
fn times_table_repeats_by_cycle() {
    let src = r#"interarrival: Const(1.0), table: (mode: Times, repeat: Cycle(100.0), rows: [
        (at: 10.0, count: 2), (at: 50.0, count: 3, labels: {"k": 7.0}),
    ])"#;
    let text = table_model(src, 340.0, false);
    let (trace, _) = traced(&text);
    let want: Vec<f64> = [10.0, 50.0, 110.0, 150.0, 210.0, 250.0, 310.0]
        .iter()
        .zip([2, 3, 2, 3, 2, 3, 2])
        .flat_map(|(t, n)| std::iter::repeat_n(*t, n))
        .collect();
    assert_eq!(created_at(&trace), want);
    let r = run(&text);
    let src = r.block("src").unwrap();
    assert_eq!(src.custom["arrivals.count"], 7.0);
    assert_eq!(src.custom["entered.count"], 17.0);
    assert_eq!(r.block("s1").unwrap().input, 17);
}

#[test]
fn times_table_once_stops_idle() {
    let src =
        r#"interarrival: Const(1.0), table: (rows: [(at: 0.0, count: 1), (at: 5.0, count: 4)])"#;
    let text = table_model(src, 100.0, false);
    let (trace, _) = traced(&text);
    assert_eq!(created_at(&trace), [0.0, 5.0, 5.0, 5.0, 5.0]);
    let r = run(&text);
    let src = r.block("src").unwrap();
    // Idle from t = 5 on.
    assert!((src.states["idle"] - 0.95).abs() < 1e-9, "{:?}", src.states);
}

#[test]
fn sequence_table_follows_the_rows_and_loops() {
    // Arrivals at 0, 5, 10, 15, 20: rows 1, 2, 1, 2, 1. Row 1 makes one item of type 1,
    // row 2 two items of type 2.
    let src = r#"interarrival: Const(5.0), first_at_zero: true, table: (mode: Sequence, repeat: Loop, rows: [
        (count: 1, labels: {"tipo": 1.0}), (count: 2, labels: {"tipo": 2.0}),
    ])"#;
    let text = table_model(src, 22.0, true);
    let (trace, names) = traced(&text);
    assert_eq!(
        deliveries(&trace, &names, &["s1", "s2"]),
        ["s1", "s2", "s2", "s1", "s2", "s2", "s1"]
    );
    assert_eq!(created_at(&trace), [0.0, 5.0, 5.0, 10.0, 15.0, 15.0, 20.0]);
    // Once: the two rows, then idle.
    let once = text.replace("repeat: Loop, ", "");
    let (trace, _) = traced(&once);
    assert_eq!(created_at(&trace), [0.0, 5.0, 5.0]);
}

#[test]
fn sequence_waits_for_the_items_to_leave() {
    // With on_blocked: Wait (the default) the gap to the next row starts when the last
    // item of the row has left. A slow machine (5 per item) after the source: row 1 has
    // two items (leave at 0 and 5), so row 2 arrives at 5 + 1.
    let text = r#"Model(
        format: 2,
        name: "espera",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true,
                table: (mode: Sequence, rows: [(count: 2), (count: 1)]))),
            (name: "m", kind: Processor(process: Const(5.0))),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "m"), ("m", "out")],
    )"#;
    let (trace, _) = traced(text);
    assert_eq!(created_at(&trace), [0.0, 0.0, 6.0]);
}

#[test]
fn table_is_validated() {
    let bad = |src: &str| {
        Model::parse(&table_model(src, 10.0, false))
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string()
    };
    let e = bad(r#"interarrival: Const(1.0), table: (rows: [(at: 5.0), (at: 2.0)])"#);
    assert!(e.contains("em ordem de at"), "{e}");
    let e = bad(r#"interarrival: Const(1.0), table: (repeat: Cycle(10.0), rows: [(at: 12.0)])"#);
    assert!(e.contains("dentro do ciclo"), "{e}");
    let e = bad(r#"interarrival: Const(1.0), table: (rows: [(count: 1)])"#);
    assert!(e.contains("precisa de `at`"), "{e}");
    let e = bad(
        r#"interarrival: Const(1.0), table: (mode: Sequence, repeat: Cycle(5.0), rows: [(count: 1)])"#,
    );
    assert!(e.contains("Cycle só vale no modo Times"), "{e}");
    let e = bad(
        r#"interarrival: Const(0.0), table: (mode: Sequence, repeat: Loop, rows: [(count: 0)])"#,
    );
    assert!(e.contains("média > 0"), "{e}");
    let e = bad(
        r#"interarrival: Const(1.0), schedule: (cycle: 10.0, periods: [(start: 0.0, end: 5.0)]),
           table: (rows: [(at: 1.0)])"#,
    );
    assert!(e.contains("não os dois"), "{e}");
}

#[test]
fn check_lines_describe_the_table() {
    let m = model(&table_model(
        r#"interarrival: Const(1.0), table: (repeat: Cycle(1440.0), rows: [(at: 480.0, count: 12), (at: 600.0, count: 5)])"#,
        2000.0,
        false,
    ));
    assert_eq!(
        m.arrival_lines(),
        ["src: horário com 2 linha(s), 17 item(ns) a cada ciclo de 1440"]
    );
}

#[test]
fn table_round_trips_through_ron() {
    let m = model(&table_model(
        r#"interarrival: Const(1.0), table: (mode: Sequence, repeat: Loop, rows: [(count: 2, labels: {"a": 1.0})])"#,
        10.0,
        false,
    ));
    let text = ron::ser::to_string(&m).unwrap();
    assert_eq!(Model::parse(&text).unwrap(), m);
}

#[test]
fn experiment_keys_change_table_rows() {
    let mut m = model(&table_model(
        r#"interarrival: Const(1.0), table: (rows: [(at: 1.0, count: 2), (at: 3.0)])"#,
        10.0,
        false,
    ));
    utsim_model::experiment::patch(&mut m, "src.table.rows[1].count", "5").unwrap();
    utsim_model::experiment::patch(&mut m, "src.table.rows[2].labels.k", "4.0").unwrap();
    utsim_model::experiment::patch(&mut m, "src.table.rows[2].at", "2.5").unwrap();
    let utsim_model::ObjectKind::Source { table: Some(t), .. } = &m.objects[0].kind else {
        unreachable!()
    };
    assert_eq!(t.rows[0].count, 5);
    assert_eq!(t.rows[1].labels["k"], 4.0);
    assert_eq!(t.rows[1].at, Some(2.5));
    let e = utsim_model::experiment::patch(&mut m, "src.table.rows[3].count", "1")
        .unwrap_err()
        .to_string();
    assert!(e.contains("linha 3 não existe"), "{e}");
}

// ---------------------------------------------------------------- C2 batch per arrival

#[test]
fn batch_creates_several_items_per_arrival() {
    let text = table_model(
        "interarrival: Const(10.0), first_at_zero: true, batch: Const(3.0)",
        35.0,
        false,
    );
    let (trace, _) = traced(&text);
    assert_eq!(
        created_at(&trace),
        [
            0.0, 0.0, 0.0, 10.0, 10.0, 10.0, 20.0, 20.0, 20.0, 30.0, 30.0, 30.0
        ]
    );
    let r = run(&text);
    let src = r.block("src").unwrap();
    assert_eq!(src.custom["arrivals.count"], 4.0);
    assert_eq!(src.custom["entered.count"], 12.0);
}

#[test]
fn one_by_one_spaces_the_items_and_waits_for_the_last() {
    // Items at 0, 1, 2; the next arrival starts its gap when the last left (2 + 10).
    let text = table_model(
        "interarrival: Const(10.0), first_at_zero: true, batch: Const(3.0), release: OneByOne(Const(1.0))",
        35.0,
        false,
    );
    let (trace, _) = traced(&text);
    assert_eq!(
        created_at(&trace),
        [0.0, 1.0, 2.0, 12.0, 13.0, 14.0, 24.0, 25.0, 26.0]
    );
    // The same spacing applies to the rows of a table.
    let text = table_model(
        "interarrival: Const(1.0), release: OneByOne(Const(0.5)), table: (rows: [(at: 4.0, count: 3)])",
        35.0,
        false,
    );
    let (trace, _) = traced(&text);
    assert_eq!(created_at(&trace), [4.0, 4.5, 5.0]);
}

#[test]
fn batch_with_drop_loses_what_does_not_fit() {
    // Queue of 1 before a machine that never finishes in the run: the first arrival
    // places two items (machine and queue) and loses one; the next three lose all three.
    let text = r#"Model(
        format: 2,
        name: "lote_perdido",
        run: (stop_time: 35.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(10.0), first_at_zero: true,
                batch: Const(3.0), on_blocked: Drop)),
            (name: "q", kind: Queue(capacity: 1)),
            (name: "m", kind: Processor(process: Const(1000.0))),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "q"), ("q", "m"), ("m", "out")],
    )"#;
    let r = run(text);
    let src = r.block("src").unwrap();
    assert_eq!(src.custom["arrivals.count"], 4.0);
    assert_eq!(src.custom["lost.count"], 10.0);
    assert_eq!(src.custom["entered.count"], 2.0);
}

#[test]
fn batch_is_validated() {
    let e = Model::parse(&table_model(
        "interarrival: Const(1.0), batch: Const(2.0), table: (rows: [(at: 1.0)])",
        10.0,
        false,
    ))
    .unwrap()
    .validate()
    .unwrap_err()
    .to_string();
    assert!(e.contains("tire `batch`"), "{e}");
    let e = Model::parse(&table_model(
        "interarrival: Const(1.0), batch: Normal(mean: 1.0, sd: -1.0)",
        10.0,
        false,
    ))
    .unwrap()
    .validate()
    .unwrap_err()
    .to_string();
    assert!(e.contains("src.batch"), "{e}");
}

// ---------------------------------------------------------------- C3 item types

/// Two types and a source with `source` fields routing by `label` to sinks s1 and s2.
fn typed_model(source: &str, label: &str) -> String {
    format!(
        r##"Model(
            format: 2,
            name: "tipos",
            run: (stop_time: 1000.0, seed: 7),
            item_types: [
                (name: "A", color: "#3b82f6", asset: "caixa_pequena", labels: {{"peso": 8.0, "rota": 2.0}}),
                (name: "B", color: "#f59e0b", labels: {{"rota": 1.0}}),
            ],
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 2.0), {source}), output: ByLabel("{label}")),
                (name: "s1", kind: Sink),
                (name: "s2", kind: Sink),
            ],
            connections: [("src", "s1"), ("src", "s2")],
        )"##
    )
}

#[test]
fn reserved_type_label_routes_by_type() {
    // Type A = 1 goes to port 1 (s1), B = 2 to s2; the mix draws about 1:3.
    let r = run(&typed_model(
        r#"item_type_mix: [("A", 1.0), ("B", 3.0)]"#,
        "type",
    ));
    let (s1, s2) = (r.block("s1").unwrap(), r.block("s2").unwrap());
    assert!(s1.input > 0 && s2.input > 0);
    let share = s2.input as f64 / (s1.input + s2.input) as f64;
    assert!((share - 0.75).abs() < 0.05, "{share}");
    // Per-type report at the sinks: each sink got one type only, and the counts add up.
    assert_eq!(s1.extra["time_in_system.A"].count, s1.input);
    assert!(!s1.extra.contains_key("time_in_system.B"));
    assert_eq!(s2.extra["time_in_system.B"].count, s2.input);
    assert_eq!(s1.extra["time_in_system"].count, s1.input);
}

#[test]
fn type_labels_are_defaults_the_source_can_override() {
    // A's `rota` is 2: every A goes to s2.
    let r = run(&typed_model(r#"item_type: "A""#, "rota"));
    assert_eq!(r.block("s1").unwrap().input, 0);
    assert!(r.block("s2").unwrap().input > 0);
    // The source's own `rota` wins over the type's.
    let r = run(&typed_model(
        r#"item_type: "A", labels: {"rota": Const(1.0)}"#,
        "rota",
    ));
    assert_eq!(r.block("s2").unwrap().input, 0);
    assert!(r.block("s1").unwrap().input > 0);
}

#[test]
fn table_rows_set_the_type_and_the_same_seed_repeats() {
    let src = r#"table: (mode: Sequence, repeat: Loop, rows: [
        (count: 2, item_type: "B"), (count: 1, item_type: "A"),
    ]), first_at_zero: true"#;
    let text = typed_model(src, "type");
    let (trace, names) = traced(&text);
    let order = deliveries(&trace, &names, &["s1", "s2"]);
    assert_eq!(order[..6], ["s2", "s2", "s1", "s2", "s2", "s1"]);
    // The trace says which type each new item got.
    let types: Vec<u32> = trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::ItemType { item_type, .. } => Some(item_type),
            _ => None,
        })
        .take(3)
        .collect();
    assert_eq!(types, [2, 2, 1]);
    // Determinism: same seed, same report.
    let a = run(&typed_model(
        r#"item_type_mix: [("A", 1.0), ("B", 1.0)]"#,
        "type",
    ));
    let b = run(&typed_model(
        r#"item_type_mix: [("A", 1.0), ("B", 1.0)]"#,
        "type",
    ));
    assert_eq!(
        serde_json::to_string(&a.blocks).unwrap(),
        serde_json::to_string(&b.blocks).unwrap()
    );
}

#[test]
fn item_types_are_validated() {
    let err = |src: &str| {
        Model::parse(&typed_model(src, "type"))
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string()
    };
    let e = err(r#"item_type: "C""#);
    assert!(e.contains("o tipo 'C' não existe (tipos: A, B)"), "{e}");
    let e = err(r#"item_type_mix: [("A", 0.0)]"#);
    assert!(e.contains("ao menos um peso"), "{e}");
    let e = err(r#"item_type: "A", item_type_mix: [("B", 1.0)]"#);
    assert!(e.contains("não os dois"), "{e}");
    let e = err(r#"item_type: "A", labels: {"type": Const(1.0)}"#);
    assert!(e.contains("reservado"), "{e}");
    let e = err(r#"table: (rows: [(at: 1.0, item_type: "Z")])"#);
    assert!(e.contains("table.rows[1].item_type"), "{e}");
    let bad = typed_model(r#"item_type: "A""#, "type").replace("#3b82f6", "azul");
    let e = Model::parse(&bad)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    assert!(e.contains("#rrggbb"), "{e}");
}

#[test]
fn check_lists_the_item_types() {
    let m = model(&typed_model(r#"item_type: "A""#, "type"));
    assert_eq!(
        m.arrival_lines(),
        ["tipos de item (rótulo 'type'): A = 1 (#3b82f6, caixa_pequena); B = 2 (#f59e0b)"]
    );
}
