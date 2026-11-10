//! Process time by recipe (`recipe` of a processor, the line Processos contínuos): an item
//! whose recipe label has a value of the table takes that time, any other the processor's
//! `process`. Hand-computed timings: one batch every 10 from t = 0, the run lasts 100, so a
//! state that lasts `d` shows as `d / 100`.

use utsim_core::RunReport;
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

/// Batches of products 1, 1, 2, 3 at t = 0, 10, 20, 30 into the reactor `kind`.
fn line(kind: &str) -> String {
    format!(
        r#"Model(
            name: "receita",
            run: (stop_time: 100.0),
            objects: [
                (name: "lotes", kind: Source(
                    interarrival: Const(10.0),
                    first_at_zero: true,
                    table: (mode: Sequence, rows: [
                        (count: 1, labels: {{"produto": 1.0}}),
                        (count: 1, labels: {{"produto": 1.0}}),
                        (count: 1, labels: {{"produto": 2.0}}),
                        (count: 1, labels: {{"produto": 3.0}}),
                    ]),
                )),
                (name: "reator", kind: {kind}),
                (name: "tanque", kind: Sink),
            ],
            connections: [("lotes", "reator"), ("reator", "tanque")],
        )"#
    )
}

#[test]
fn each_recipe_takes_its_own_time() {
    // Product 1 takes 4 (two batches), product 2 takes 6, product 3 has no recipe and
    // takes `process` (2): 4 + 4 + 6 + 2 = 16 of 100 processing.
    let r = run(&line(
        r#"Processor(process: Const(2.0), recipe: Some((label: "produto", times: [
            (value: 1.0, time: Const(4.0)), (value: 2.0, time: Const(6.0))])))"#,
    ));
    let m = r.block("reator").unwrap();
    near("processing", m.states["processing"], 0.16);
    near("staytime", m.staytime.mean, 4.0);
    assert_eq!(r.block("tanque").unwrap().input, 4);
}

#[test]
fn recipe_and_changeover_together() {
    // A cleaning (setup 3) only when the product changes: 1 -> 2 at 20 and 2 -> 3 at 30.
    // Busy: 4 + 4 + (3 + 6) + (3 + 2) = 22.
    let r = run(&line(
        r#"Processor(setup: Some(Const(3.0)), process: Const(2.0),
            changeover: Some((label: "produto")),
            receita: Some((label: "produto", times: [
                (value: 1.0, time: Const(4.0)), (value: 2.0, time: Const(6.0))])))"#,
    ));
    let m = r.block("reator").unwrap();
    near("setup", m.states["setup"], 0.06);
    near("processing", m.states["processing"], 0.16);
    near("changeovers", m.custom["changeovers"], 2.0);
}

#[test]
fn invalid_recipes_are_rejected() {
    let bad = [
        (
            r#"Processor(process: Const(2.0), recipe: Some((label: " ", times: [(value: 1.0, time: Const(4.0))])))"#,
            "rótulo da receita",
        ),
        (
            r#"Processor(process: Const(2.0), recipe: Some((label: "produto")))"#,
            "pelo menos uma receita",
        ),
        (
            r#"Processor(process: Const(2.0), recipe: Some((label: "produto", times: [
                (value: 1.0, time: Const(4.0)), (value: 1.0, time: Const(5.0))])))"#,
            "repetida",
        ),
        (
            r#"Processor(process: Const(2.0), recipe: Some((label: "produto", times: [
                (value: 1.0, time: Uniform(min: 5.0, max: 1.0))])))"#,
            "recipe.times[0].time",
        ),
    ];
    for (kind, want) in bad {
        let m = Model::parse(&line(kind)).expect("model parses");
        let e = m.validate().unwrap_err().to_string();
        assert!(e.contains(want), "{want}: {e}");
    }
}
