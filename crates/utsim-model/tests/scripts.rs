//! Scripts in Rhai (phase 6): `on_entry`, `OutputRule::Script` and the limits.

use utsim_core::RunReport;
use utsim_model::Model;

fn model(seed: u64, objects: &str, connections: &str) -> Model {
    let text = format!(
        r#"Model(
            name: "scripts",
            run: (stop_time: 1000.0, seed: {seed}),
            objects: [{objects}],
            connections: [{connections}],
        )"#
    );
    Model::parse(&text).expect("model parses")
}

fn run(m: &Model) -> RunReport {
    m.validate().expect("model is valid");
    m.run_replications().unwrap().remove(0)
}

fn input(r: &RunReport, name: &str) -> u64 {
    r.block(name).unwrap().input
}

/// Two sources (12 items with v=1, 8 items with v=5) into a queue with the given
/// output rule; two sinks receive the routed items.
fn routed(extra: &str, output: &str) -> Model {
    model(
        1,
        &format!(
            r#"(name: "a", kind: Source(interarrival: Const(1.0), max_arrivals: Some(12),
                                        labels: {{ "v": Const(1.0) }})),
               (name: "b", kind: Source(interarrival: Const(1.0), max_arrivals: Some(8),
                                        labels: {{ "v": Const(5.0) }})),
               (name: "q", kind: Queue(), output: {output} {extra}),
               (name: "s1", kind: Sink),
               (name: "s2", kind: Sink)"#
        ),
        r#"("a", "q"), ("b", "q"), ("q", "s1"), ("q", "s2")"#,
    )
}

#[test]
fn script_routing_by_label_has_exact_counts() {
    let r = run(&routed("", r#"Script("if item.v > 3 { 2 } else { 1 }")"#));
    assert_eq!(input(&r, "s1"), 12);
    assert_eq!(input(&r, "s2"), 8);
}

#[test]
fn on_entry_sets_labels_used_by_bylabel() {
    let r = run(&routed(
        r#", on_entry: Some("item.rota = if item.v > 3 { 2 } else { 1 };")"#,
        r#"ByLabel("rota")"#,
    ));
    assert_eq!(input(&r, "s1"), 12);
    assert_eq!(input(&r, "s2"), 8);
}

#[test]
fn on_entry_sees_time_and_block_name() {
    let m = model(
        1,
        r#"(name: "a", kind: Source(interarrival: Const(1.0), max_arrivals: Some(20))),
           (name: "q", kind: Queue(), output: ByLabel("rota"),
            on_entry: Some("item.rota = if block == \"q\" && now > 10.5 { 2 } else { 1 };")),
           (name: "cedo", kind: Sink),
           (name: "tarde", kind: Sink)"#,
        r#"("a", "q"), ("q", "cedo"), ("q", "tarde")"#,
    );
    let r = run(&m);
    assert_eq!(input(&r, "cedo") + input(&r, "tarde"), 20);
    assert_eq!(input(&r, "cedo"), 10);
    assert_eq!(input(&r, "tarde"), 10);
}

fn random_split(seed: u64) -> (u64, u64) {
    let m = model(
        seed,
        r#"(name: "a", kind: Source(interarrival: Const(1.0), max_arrivals: Some(400))),
           (name: "q", kind: Queue(), output: Script("if item.p < 0.3 { 1 } else { 2 }"),
            on_entry: Some("item.p = random();")),
           (name: "s1", kind: Sink),
           (name: "s2", kind: Sink)"#,
        r#"("a", "q"), ("q", "s1"), ("q", "s2")"#,
    );
    let r = run(&m);
    (input(&r, "s1"), input(&r, "s2"))
}

#[test]
fn random_functions_are_reproducible() {
    let (a1, a2) = random_split(7);
    assert_eq!(a1 + a2, 400);
    assert_eq!(random_split(7), (a1, a2), "same seed, same result");
    assert_ne!(random_split(8), (a1, a2), "another seed changes the draws");
    let share = a1 as f64 / 400.0;
    assert!((share - 0.3).abs() < 0.08, "share = {share}");
}

#[test]
fn invalid_port_is_a_modelling_error() {
    let m = routed("", r#"Script("5")"#);
    m.validate().unwrap();
    let err = m.run_replications().unwrap_err().to_string();
    assert!(err.contains("porta 5 inválida"), "{err}");
    assert!(err.contains("2 saída"), "{err}");
}

#[test]
fn runaway_script_hits_the_operation_limit() {
    let m = routed(
        r#", on_entry: Some("let i = 0; while true { i += 1; }")"#,
        "FirstAvailable",
    );
    m.validate().unwrap();
    let err = m.run_replications().unwrap_err().to_string();
    assert!(err.contains("limite"), "{err}");
}

#[test]
fn runtime_error_names_the_missing_label() {
    let m = routed(
        r#", on_entry: Some("item.x = item.nao_existe;")"#,
        "FirstAvailable",
    );
    let err = m.run_replications().unwrap_err().to_string();
    assert!(err.contains("nao_existe"), "{err}");
}

#[test]
fn compile_errors_report_line_column_and_source() {
    let m = routed(
        r#", on_entry: Some("let a = 1;\nlet b = ;")"#,
        "FirstAvailable",
    );
    let err = m.validate().unwrap_err().to_string();
    assert!(err.contains("linha 2, coluna"), "{err}");
    assert!(err.contains("let b = ;"), "{err}");
    let m = routed("", r#"Script("if (")"#);
    let err = m.validate().unwrap_err().to_string();
    assert!(err.contains("regra de saída Script"), "{err}");
    assert!(err.contains("linha 1"), "{err}");
}
