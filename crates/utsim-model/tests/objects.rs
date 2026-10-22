//! Deterministic checks of Combiner, Separator and Queue batching: exact item counts.

use utsim_core::RunReport;
use utsim_model::Model;

fn run(text: &str) -> RunReport {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m.run_replications().unwrap().remove(0)
}

fn input(r: &RunReport, name: &str) -> u64 {
    r.block(name).unwrap().input
}

/// Pallets (port 0) and boxes (port 1) into a combiner of the given mode, then `tail`.
fn assembly(mode: &str, tail_objects: &str, tail_connections: &str) -> String {
    format!(
        r#"Model(
            name: "montagem",
            run: (stop_time: 10000.0),
            objects: [
                (name: "paletes", kind: Source(interarrival: Const(4.0), max_arrivals: Some(10),
                                               labels: {{ "p": Const(1.0) }})),
                (name: "caixas", kind: Source(interarrival: Const(1.0), max_arrivals: Some(40),
                                              labels: {{ "p": Const(2.0) }})),
                (name: "comb", kind: Combiner(components: [4], mode: {mode}, process: Const(1.0))),
                {tail_objects}
            ],
            connections: [("paletes", "comb"), ("caixas", "comb"), {tail_connections}],
        )"#
    )
}

#[test]
fn combiner_pack_emits_only_containers() {
    let r = run(&assembly(
        "Pack",
        r#"(name: "out", kind: Sink)"#,
        r#"("comb", "out")"#,
    ));
    assert_eq!(input(&r, "out"), 10);
    assert_eq!(r.block("comb").unwrap().output, 10);
    assert_eq!(r.block("comb").unwrap().content_now, 0);
    assert_eq!(input(&r, "comb"), 50);
}

#[test]
fn separator_unpack_restores_the_boxes() {
    let r = run(&assembly(
        "Pack",
        r#"(name: "sep", kind: Separator(mode: Unpack, process: Const(0.5)), output: ByLabel("p")),
           (name: "pal_out", kind: Sink),
           (name: "box_out", kind: Sink)"#,
        r#"("comb", "sep"), ("sep", "pal_out"), ("sep", "box_out")"#,
    ));
    assert_eq!(input(&r, "pal_out"), 10);
    assert_eq!(input(&r, "box_out"), 40);
    assert_eq!(r.block("sep").unwrap().content_now, 0);
}

#[test]
fn combiner_join_destroys_components() {
    let r = run(&assembly(
        "Join",
        r#"(name: "out", kind: Sink)"#,
        r#"("comb", "out")"#,
    ));
    assert_eq!(input(&r, "out"), 10);
    assert_eq!(r.block("comb").unwrap().output, 10);
    assert_eq!(r.block("comb").unwrap().content_now, 0);
}

#[test]
fn combiner_batch_releases_container_and_components() {
    let r = run(&assembly(
        "Batch",
        r#"(name: "out", kind: Sink)"#,
        r#"("comb", "out")"#,
    ));
    assert_eq!(input(&r, "out"), 10 * (1 + 4));
}

#[test]
fn combiner_waits_for_every_component_port() {
    // Two component ports (2 screws + 1 cap); caps are scarce, so they set the pace.
    let r = run(r#"Model(
            name: "duas_portas",
            run: (stop_time: 100.0),
            objects: [
                (name: "base", kind: Source(interarrival: Const(0.1))),
                (name: "parafusos", kind: Source(interarrival: Const(0.1))),
                (name: "tampas", kind: Source(interarrival: Const(10.0))),
                (name: "comb", kind: Combiner(components: [2, 1], mode: Join, process: Const(0.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("base", "comb"), ("parafusos", "comb"), ("tampas", "comb"), ("comb", "out")],
        )"#);
    // Caps arrive at t=10,20,...,100 (the last one right at stop_time still counts).
    assert_eq!(input(&r, "out"), 10);
}

#[test]
fn separator_split_triples_the_flow() {
    let r = run(r#"Model(
            name: "split",
            run: (stop_time: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(2.0), max_arrivals: Some(7))),
                (name: "sep", kind: Separator(mode: Split(3), process: Const(1.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "sep"), ("sep", "out")],
        )"#);
    assert_eq!(input(&r, "out"), 21);
}

fn batch_model(batch: &str, n: u32, stop: f64) -> String {
    format!(
        r#"Model(
            name: "lote",
            run: (stop_time: {stop}),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0), max_arrivals: Some({n}))),
                (name: "q", kind: Queue(batch: Some({batch}))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "out")],
        )"#
    )
}

#[test]
fn queue_batch_waits_for_full_size() {
    let r = run(&batch_model("(size: 5)", 12, 100.0));
    assert_eq!(input(&r, "out"), 10);
    assert_eq!(r.block("q").unwrap().content_now, 2);
}

#[test]
fn queue_batch_max_wait_releases_partial_batches() {
    // Items at t=1..12. Batches {1,2,3} (timer 1+2.5), {4,5,6}, {7,8,9}, {10,11,12}.
    let full = run(&batch_model("(size: 5, max_wait: 2.5)", 12, 100.0));
    assert_eq!(input(&full, "out"), 12);
    // At t=12.2 the last batch is still waiting (its timer fires at 12.5).
    let partial = run(&batch_model("(size: 5, max_wait: 2.5)", 12, 12.2));
    assert_eq!(input(&partial, "out"), 9);
}

#[test]
fn queue_batch_flush_closes_the_input() {
    let model = |flush: bool| {
        format!(
            r#"Model(
                name: "flush",
                run: (stop_time: 1000.0),
                objects: [
                    (name: "src", kind: Source(interarrival: Const(1.0), max_arrivals: Some(6))),
                    (name: "q", kind: Queue(batch: Some((size: 2, flush: {flush})))),
                    (name: "p", kind: Processor(process: Const(10.0))),
                    (name: "out", kind: Sink),
                ],
                connections: [("src", "q"), ("q", "p"), ("p", "out")],
            )"#
        )
    };
    let closed = run(&model(true));
    let open = run(&model(false));
    assert_eq!(input(&closed, "out"), 6);
    assert_eq!(input(&open, "out"), 6);
    // Flush: never more than the one batch inside. No flush: keeps collecting meanwhile.
    assert_eq!(closed.block("q").unwrap().content_max, 2.0);
    assert_eq!(open.block("q").unwrap().content_max, 5.0);
}

#[test]
fn invalid_new_objects_are_rejected() {
    let bad = |objects: &str, conns: &str| {
        let text = format!(
            r#"Model(name: "x", run: (stop_time: 10.0), objects: [
                (name: "s", kind: Source(interarrival: Const(1.0))),
                (name: "s2", kind: Source(interarrival: Const(1.0))),
                {objects}
                (name: "out", kind: Sink),
            ], connections: [{conns}])"#
        );
        Model::parse(&text).unwrap().validate().is_err()
    };
    let comb = r#"(name: "c", kind: Combiner(components: [1], process: Const(1.0))),"#;
    // Missing the component input.
    assert!(bad(comb, r#"("s", "c"), ("c", "out")"#));
    // Correct wiring is accepted.
    assert!(!bad(comb, r#"("s", "c"), ("s2", "c"), ("c", "out")"#));
    let empty = r#"(name: "c", kind: Combiner(components: [], process: Const(1.0))),"#;
    assert!(bad(empty, r#"("s", "c"), ("c", "out")"#));
    let split0 = r#"(name: "c", kind: Separator(mode: Split(0), process: Const(1.0))),"#;
    assert!(bad(split0, r#"("s", "c"), ("c", "out")"#));
    let batch0 = r#"(name: "c", kind: Queue(batch: Some((size: 0)))),"#;
    assert!(bad(batch0, r#"("s", "c"), ("c", "out")"#));
}
