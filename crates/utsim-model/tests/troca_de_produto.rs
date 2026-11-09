//! Product changeover (`changeover` of a processor, the line Manufatura discreta): the setup
//! happens only when the product label changes, with the time of each change from the
//! matrix or, without a row, from `setup`. Hand-computed timings: one order every 10 from
//! t = 0, process 2, the run lasts 100, so a state that lasts `d` shows as `d / 100`.

use utsim_core::RunReport;
use utsim_model::{Model, ObjectKind};

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

/// Orders of products 1, 1, 2, 1 at t = 0, 10, 20, 30 into the machine `kind`.
fn line(kind: &str) -> String {
    format!(
        r#"Model(
            name: "troca",
            run: (stop_time: 100.0),
            objects: [
                (name: "ordens", kind: Source(
                    interarrival: Const(10.0),
                    first_at_zero: true,
                    table: (mode: Sequence, rows: [
                        (count: 1, labels: {{"produto": 1.0}}),
                        (count: 1, labels: {{"produto": 1.0}}),
                        (count: 1, labels: {{"produto": 2.0}}),
                        (count: 1, labels: {{"produto": 1.0}}),
                    ]),
                )),
                (name: "maquina", kind: {kind}),
                (name: "expedicao", kind: Sink),
            ],
            connections: [("ordens", "maquina"), ("maquina", "expedicao")],
        )"#
    )
}

#[test]
fn the_setup_happens_only_when_the_product_changes() {
    // Order 1 (product 1) is the first: the machine starts ready, 0..2. Order 2, same
    // product: 10..12. Order 3, 1 -> 2 from the matrix (3): setup 20..23, process 23..25.
    // Order 4, 2 -> 1 has no row, so `setup` (5): 30..35, process 35..37.
    let r = run(&line(
        r#"Processor(setup: Some(Const(5.0)), process: Const(2.0),
            changeover: Some((label: "produto", matrix: [(from: 1.0, to: 2.0, time: Const(3.0))])))"#,
    ));
    let m = r.block("maquina").unwrap();
    near("setup", state(&r, "maquina", "setup"), 0.08);
    near("processing", state(&r, "maquina", "processing"), 0.08);
    near("staytime", m.staytime.mean, (2.0 + 2.0 + 5.0 + 7.0) / 4.0);
    near("changeovers", m.custom["changeovers"], 2.0);
    assert_eq!(r.block("expedicao").unwrap().input, 4);
}

#[test]
fn without_changeover_the_setup_is_every_order() {
    // The same line without `changeover`: setup 5 and process 2 on every order, and no
    // changeover count in the report (the reports of other models stay as they were).
    let r = run(&line(
        r#"Processor(setup: Some(Const(5.0)), process: Const(2.0))"#,
    ));
    let m = r.block("maquina").unwrap();
    near("setup", state(&r, "maquina", "setup"), 0.20);
    near("staytime", m.staytime.mean, 7.0);
    assert!(!m.custom.contains_key("changeovers"));
}

#[test]
fn orders_without_the_label_never_change() {
    // An order without the product label does not count as a change, and the next one
    // compares with "no product": the machine stays ready.
    let text = line(
        r#"Processor(setup: Some(Const(5.0)), process: Const(2.0),
            changeover: Some((label: "familia")))"#,
    );
    let r = run(&text);
    near("setup", state(&r, "maquina", "setup"), 0.0);
    near(
        "changeovers",
        r.block("maquina").unwrap().custom["changeovers"],
        0.0,
    );
}

#[test]
fn invalid_changeovers_are_rejected() {
    let bad = [
        (
            r#"Processor(process: Const(2.0), changeover: Some((label: "produto")))"#,
            "changeover precisa de setup",
        ),
        (
            r#"Processor(setup: Some(Const(5.0)), process: Const(2.0), changeover: Some((label: " ")))"#,
            "nome do rótulo",
        ),
        (
            r#"Processor(setup: Some(Const(5.0)), process: Const(2.0),
                changeover: Some((label: "produto", matrix: [(from: 1.0, to: 1.0, time: Const(3.0))])))"#,
            "from e to diferentes",
        ),
        (
            r#"Processor(setup: Some(Const(5.0)), process: Const(2.0),
                changeover: Some((label: "produto", matrix: [
                    (from: 1.0, to: 2.0, time: Const(3.0)),
                    (from: 1.0, to: 2.0, time: Const(4.0)),
                ])))"#,
            "repetida",
        ),
        (
            r#"Processor(setup: Some(Const(5.0)), process: Const(2.0),
                changeover: Some((label: "produto", matrix: [(from: 1.0, to: 2.0, time: Uniform(min: 5.0, max: 1.0))])))"#,
            "changeover.matrix[0].time",
        ),
    ];
    for (kind, want) in bad {
        let m = Model::parse(&line(kind)).expect("model parses");
        let e = m.validate().unwrap_err().to_string();
        assert!(e.contains(want), "{want}: {e}");
    }
}

#[test]
fn the_changeover_survives_a_round_trip() {
    let m = Model::parse(&line(
        r#"Processor(setup: Some(Const(5.0)), process: Const(2.0),
            changeover: Some((label: "produto", matrix: [(from: 1.0, to: 2.0, time: Const(3.0))])))"#,
    ))
    .unwrap();
    let text = ron::ser::to_string_pretty(&m, ron::ser::PrettyConfig::default()).unwrap();
    let back = Model::parse(&text).unwrap();
    let ObjectKind::Processor { changeover, .. } = &back.objects[1].kind else {
        unreachable!()
    };
    let c = changeover.as_ref().unwrap();
    assert_eq!(c.label, "produto");
    assert_eq!(c.matrix.len(), 1);
    assert_eq!(c.matrix[0].to, 2.0);
}
