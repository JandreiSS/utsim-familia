//! Fluid networks: every expected value below is computed by hand in the comment above
//! the test. Metrics appear as `fluidos.<no>.<metrica>` in the flat metrics view.

use std::collections::BTreeMap;

use utsim_core::RunReport;
use utsim_model::Model;

fn model(stop: f64, warmup: f64, objects: &str, connections: &str, fluids: &str) -> Model {
    let text = format!(
        r#"Model(
            name: "fluidos",
            run: (stop_time: {stop}, warmup: {warmup}),
            objects: [{objects}],
            connections: [{connections}],
            fluids: Some((
                name: "fluidos",
                {fluids}
            )),
        )"#
    );
    let m = Model::parse(&text).expect("model parses");
    if let Err(e) = m.validate() {
        panic!("model is invalid: {e}");
    }
    m
}

fn run(m: &Model) -> RunReport {
    m.run_replications().unwrap().remove(0)
}

/// A purely fluid model (no discrete objects needed except a placeholder).
fn only_fluids(stop: f64, warmup: f64, fluids: &str) -> BTreeMap<String, f64> {
    let m = model(
        stop,
        warmup,
        r#"(name: "fonte0", kind: Source(interarrival: Const(1.0), max_arrivals: Some(0))),
           (name: "fim0", kind: Sink)"#,
        r#"("fonte0", "fim0")"#,
        fluids,
    );
    run(&m).metrics()
}

fn near(m: &BTreeMap<String, f64>, key: &str, want: f64) {
    let got = *m
        .get(key)
        .unwrap_or_else(|| panic!("sem '{key}'; existem {:?}", m.keys()));
    assert!(
        (got - want).abs() <= 1e-9 * want.abs().max(1.0),
        "{key}: obtido {got}, esperado {want}"
    );
}

const SOURCE_TANK_PIPE_SINK: &str = r#"
    nodes: [
        (name: "fonte", kind: Source(rate: 2.0)),
        (name: "tanque", kind: Tank(capacity: 10.0)),
        (name: "tubo", kind: Pipe(max_rate: 1.0)),
        (name: "ralo", kind: Sink()),
    ],
    links: [("fonte", "tanque"), ("tanque", "tubo"), ("tubo", "ralo")],
"#;

// Source 2/h into an initially empty tank (cap 10) that drains through a 1/h pipe.
// The empty tank passes through up to the pipe, so the net rate is 2 - 1 = 1/h and the
// tank is full at t = 10. After that the inflow is throttled to 1/h.
// Stop at 30: final level 10, max 10.
// Mean level = (integral 0..10 of t dt + 10 * 20) / 30 = (50 + 200) / 30.
// Volumes: source emits 2*10 + 1*20 = 40 (= tank received); pipe and sink receive 30.
#[test]
fn tank_fills_then_throttles_inflow() {
    let m = only_fluids(30.0, 0.0, SOURCE_TANK_PIPE_SINK);
    near(&m, "fluidos.tanque.nivel_final", 10.0);
    near(&m, "fluidos.tanque.nivel_max", 10.0);
    near(&m, "fluidos.tanque.nivel_medio", 250.0 / 30.0);
    near(&m, "fluidos.fonte.volume", 40.0);
    near(&m, "fluidos.tanque.volume", 40.0);
    near(&m, "fluidos.tubo.volume", 30.0);
    near(&m, "fluidos.ralo.volume", 30.0);
}

// Same network stopped at t = 4: the tank is still filling at 1/h, level 4.
#[test]
fn tank_level_is_exact_between_events() {
    let m = only_fluids(4.0, 0.0, SOURCE_TANK_PIPE_SINK);
    near(&m, "fluidos.tanque.nivel_final", 4.0);
    near(&m, "fluidos.tanque.nivel_medio", 2.0);
    near(&m, "fluidos.fonte.volume", 8.0);
}

// With a warm-up of 10 the statistics restart at the instant the tank becomes full:
// level constant at 10 (mean, max, final) and, from then on, every flow is 1/h for 20.
#[test]
fn statistics_restart_after_warmup() {
    let m = only_fluids(30.0, 10.0, SOURCE_TANK_PIPE_SINK);
    near(&m, "fluidos.tanque.nivel_medio", 10.0);
    near(&m, "fluidos.tanque.nivel_max", 10.0);
    near(&m, "fluidos.tanque.nivel_final", 10.0);
    near(&m, "fluidos.fonte.volume", 20.0);
    near(&m, "fluidos.tanque.volume", 20.0);
}

// Tank with 6 units, no inflow, 1.5/h pipe to a sink: empty at t = 4.
// Stop at 10: final 0, max 6 (the initial level), mean = (24 - 12) / 10 = 1.2
// (integral 0..4 of 6 - 1.5 t dt = 24 - 12), volume received by the sink = 6.
#[test]
fn tank_drains_to_empty() {
    let m = only_fluids(
        10.0,
        0.0,
        r#"nodes: [
            (name: "tanque", kind: Tank(capacity: 10.0, initial: 6.0)),
            (name: "tubo", kind: Pipe(max_rate: 1.5)),
            (name: "ralo", kind: Sink()),
        ],
        links: [("tanque", "tubo"), ("tubo", "ralo")],"#,
    );
    near(&m, "fluidos.tanque.nivel_final", 0.0);
    near(&m, "fluidos.tanque.nivel_max", 6.0);
    near(&m, "fluidos.tanque.nivel_medio", 1.2);
    near(&m, "fluidos.ralo.volume", 6.0);
}

// Source 2/h into two parallel unlimited sinks: 1/h each, 10 over 10 time units.
// If one of them is capped at 0.5/h, the refused 0.5 goes to the other: 0.5 and 1.5/h,
// i.e. 5 and 15 over 10 time units.
#[test]
fn parallel_sinks_share_and_redistribute() {
    let links = r#"links: [("fonte", "a"), ("fonte", "b")],"#;
    let nodes = |cap: &str| {
        format!(
            r#"nodes: [
                (name: "fonte", kind: Source(rate: 2.0)),
                (name: "a", kind: Sink({cap})),
                (name: "b", kind: Sink()),
            ],
            {links}"#
        )
    };
    let m = only_fluids(10.0, 0.0, &nodes(""));
    near(&m, "fluidos.a.volume", 10.0);
    near(&m, "fluidos.b.volume", 10.0);
    let m = only_fluids(10.0, 0.0, &nodes("max_rate: Some(0.5)"));
    near(&m, "fluidos.a.volume", 5.0);
    near(&m, "fluidos.b.volume", 15.0);
}

// Two sources (2/h each) into a sink limited to 3/h: each gets 1.5/h; over 10 time
// units the sink receives 30 and each source emits 15.
#[test]
fn contention_for_a_shared_sink() {
    let m = only_fluids(
        10.0,
        0.0,
        r#"nodes: [
            (name: "f1", kind: Source(rate: 2.0)),
            (name: "f2", kind: Source(rate: 2.0)),
            (name: "ralo", kind: Sink(max_rate: Some(3.0))),
        ],
        links: [("f1", "ralo"), ("f2", "ralo")],"#,
    );
    near(&m, "fluidos.ralo.volume", 30.0);
    near(&m, "fluidos.f1.volume", 15.0);
    near(&m, "fluidos.f2.volume", 15.0);
}

// Trucks every 5 time units unload 3 units each into a tank drained at 0.5/h.
// Arrivals at t = 5, 10, 15, 20. Level right after the n-th arrival: 3, 3.5, 4, 4.5
// (each cycle drains 2.5 and adds 3). Stop at 22: final 4.5 - 2*0.5 = 3.5, max 4.5.
// Area: [0,5] 0; then 5L - 6.25 for L = 3, 3.5, 4 (8.75, 11.25, 13.75) and, over the
// last 2 units from 4.5, 9 - 1 = 8. Mean = 41.75 / 22.
// Volumes: 12 delivered (4 trucks); the pipe moves 0.5 * 17 = 8.5.
#[test]
fn items_become_fluid_sawtooth() {
    let m = model(
        22.0,
        0.0,
        r#"(name: "caminhoes", kind: Source(interarrival: Const(5.0))),
           (name: "descarga", kind: Sink)"#,
        r#"("caminhoes", "descarga")"#,
        r#"nodes: [
            (name: "entrada", kind: FromItems(object: "descarga", volume: 3.0)),
            (name: "tanque", kind: Tank(capacity: 100.0)),
            (name: "bomba", kind: Pipe(max_rate: 0.5)),
            (name: "ralo", kind: Sink()),
        ],
        links: [("entrada", "tanque"), ("tanque", "bomba"), ("bomba", "ralo")],"#,
    );
    let r = run(&m);
    assert_eq!(r.block("descarga").unwrap().input, 4);
    let m = r.metrics();
    near(&m, "fluidos.tanque.nivel_final", 3.5);
    near(&m, "fluidos.tanque.nivel_max", 4.5);
    near(&m, "fluidos.tanque.nivel_medio", 41.75 / 22.0);
    near(&m, "fluidos.entrada.volume", 12.0);
    near(&m, "fluidos.tanque.volume", 12.0);
    near(&m, "fluidos.bomba.volume", 8.5);
}

const BOTTLING: &str = r#"
    nodes: [
        (name: "fonte", kind: Source(rate: 2.0)),
        (name: "envase", kind: ToItems(object: "fila", volume: 4.0, max_rate: Some(RATE))),
    ],
    links: [("fonte", "envase")],
"#;

fn bottling(rate: &str, stop: f64) -> RunReport {
    let m = model(
        stop,
        0.0,
        r#"(name: "fila", kind: Queue()),
           (name: "saida", kind: Sink)"#,
        r#"("fila", "saida")"#,
        &BOTTLING.replace("RATE", rate),
    );
    run(&m)
}

// Converter at 2/h (the source rate), 4 units per item: one item every 2 time units,
// at t = 2, 4, ..., 20 -> 10 items by t = 21.5, all reaching the sink.
// Limited to 1/h the source is throttled and one item appears every 4: t = 4, ..., 20
// -> 5 items; the source emits 21.5 and the converter receives 21.5.
#[test]
fn fluid_becomes_items() {
    let r = bottling("2.0", 21.5);
    assert_eq!(r.block("fila").unwrap().input, 10);
    assert_eq!(r.block("saida").unwrap().input, 10);
    near(&r.metrics(), "fluidos.envase.volume", 43.0);

    let r = bottling("1.0", 21.5);
    assert_eq!(r.block("fila").unwrap().input, 5);
    assert_eq!(r.block("saida").unwrap().input, 5);
    let m = r.metrics();
    near(&m, "fluidos.fonte.volume", 21.5);
    near(&m, "fluidos.envase.volume", 21.5);
}

// A tank that receives 6 units per item (cap 10, nothing drains it): the first item
// brings the level to 6, the second would bring it to 12.
#[test]
fn overflow_is_a_clear_error() {
    let m = model(
        100.0,
        0.0,
        r#"(name: "caminhoes", kind: Source(interarrival: Const(5.0))),
           (name: "descarga", kind: Sink)"#,
        r#"("caminhoes", "descarga")"#,
        r#"nodes: [
            (name: "entrada", kind: FromItems(object: "descarga", volume: 6.0)),
            (name: "tanque", kind: Tank(capacity: 10.0)),
        ],
        links: [("entrada", "tanque")],"#,
    );
    let e = m.run_replications().unwrap_err().to_string();
    assert!(e.contains("tanque 'tanque' transbordou"), "{e}");
}

// A long run with contention, a throttled source and truck-sized impulses. Network:
//   fonte(3) -> tubo(2.5) -> A(cap 5, max_out 2) -> X(1.5/h)
//   caminhoes -> 1.3 per item every 7 -> C(cap 20, max_out 0.3) -> X
// Whatever the dynamics, input (source + trucks) = X received + stored in A and C.
#[test]
fn long_run_conserves_mass() {
    let stop = 20000.0;
    let m = model(
        stop,
        0.0,
        r#"(name: "caminhoes", kind: Source(interarrival: Const(7.0))),
           (name: "descarga", kind: Sink)"#,
        r#"("caminhoes", "descarga")"#,
        r#"nodes: [
            (name: "fonte", kind: Source(rate: 3.0)),
            (name: "tubo", kind: Pipe(max_rate: 2.5)),
            (name: "a", kind: Tank(capacity: 5.0, max_out: Some(2.0))),
            (name: "entrada", kind: FromItems(object: "descarga", volume: 1.3)),
            (name: "c", kind: Tank(capacity: 20.0, max_out: Some(0.3))),
            (name: "x", kind: Sink(max_rate: Some(1.5))),
        ],
        links: [("fonte", "tubo"), ("tubo", "a"), ("a", "x"),
                ("entrada", "c"), ("c", "x")],"#,
    );
    let m = run(&m).metrics();
    let get = |k: &str| m[&format!("fluidos.{k}")];
    let input = get("fonte.volume") + get("entrada.volume");
    let output = get("x.volume");
    let stored = get("a.nivel_final") + get("c.nivel_final");
    assert!(
        (input - output - stored).abs() <= 1e-9 * input,
        "entrada {input}, saída {output}, estoque {stored}"
    );
    // X is saturated nearly all the time, so it received almost 1.5 * stop.
    assert!(
        output > 1.45 * stop && output <= 1.5 * stop + 1e-6,
        "{output}"
    );
    assert!(get("a.nivel_max") <= 5.0 + 1e-9 && get("c.nivel_max") <= 20.0 + 1e-9);
}

// A converter whose object is full waits. Tank starts at 20 and feeds a ToItems of 1
// unit per item at up to 2/h (an item every 0.5) into a Queue of capacity 2, served by
// a processor taking 2 time units per item (0.5 items/h).
// t=0.5, 1.0, 1.5: items 1-3 accepted (1 in the processor, 2 queued: queue full).
// t=2.0: item 4 refused -> the unit goes back to the converter, which blocks (rate 0)
// until the processor takes the next queued item at t=2.5 (queue exit); the retry
// succeeds at once and the converter runs 0.5 more (1 unit) until the next refusal at
// t=3.0. The pattern repeats every 2 time units: accepted at 4.5, 6.5, 8.5.
// Stop at 10: 7 items entered the queue (0.5, 1, 1.5, 2.5, 4.5, 6.5, 8.5) and 4 reached
// the sink (processor finishes at 2.5, 4.5, 6.5, 8.5). The tank lost 2*2 (t<2) + 1 per
// 0.5-run at 2.5-3, 4.5-5, 6.5-7, 8.5-9 = 8 -> level 12; 7 units became items and
// 1 waits inside the blocked converter (8 received).
#[test]
fn converter_waits_when_object_is_full() {
    let m = model(
        10.0,
        0.0,
        r#"(name: "fila", kind: Queue(capacity: Some(2))),
           (name: "rotuladora", kind: Processor(capacity: 1, process: Const(2.0))),
           (name: "saida", kind: Sink)"#,
        r#"("fila", "rotuladora"), ("rotuladora", "saida")"#,
        r#"nodes: [
            (name: "reserva", kind: Tank(capacity: 100.0, initial: 20.0)),
            (name: "envase", kind: ToItems(object: "fila", volume: 1.0, max_rate: Some(2.0))),
        ],
        links: [("reserva", "envase")],"#,
    );
    let r = run(&m);
    assert_eq!(r.block("fila").unwrap().input, 7);
    assert_eq!(r.block("saida").unwrap().input, 4);
    let m = r.metrics();
    near(&m, "fluidos.reserva.nivel_final", 12.0);
    near(&m, "fluidos.envase.volume", 8.0);
}

/// A pump near the largest f64 between two tanks. The item that enters `descarga` at
/// t = 1 drops 6 L into `a`, which the pump empties into `b` in 6 / 1e308 time units:
/// shorter than the clock can tell apart from t = 1. The step is taken at once (it once
/// fired at the same instant until the limit of events per instant stopped the run).
#[test]
fn a_pump_too_fast_for_the_clock_moves_the_volume_at_once() {
    let m = model(
        10.0,
        0.0,
        r#"(name: "caminhao", kind: Source(interarrival: Const(1.0), max_arrivals: Some(1))),
           (name: "descarga", kind: Sink)"#,
        r#"("caminhao", "descarga")"#,
        r#"nodes: [
            (name: "entrada", kind: FromItems(object: "descarga", volume: 6.0)),
            (name: "a", kind: Tank(capacity: 100.0)),
            (name: "bomba", kind: Pipe(max_rate: 1e308)),
            (name: "b", kind: Tank(capacity: 30.0)),
        ],
        links: [("entrada", "a"), ("a", "bomba"), ("bomba", "b")],"#,
    );
    let r = run(&m).metrics();
    near(&r, "fluidos.a.nivel_final", 0.0);
    near(&r, "fluidos.b.nivel_final", 6.0);
    near(&r, "fluidos.b.nivel_max", 6.0);
    // From t = 1 to 10 the 6 L sit in b: mean level 6 × 9 / 10.
    near(&r, "fluidos.b.nivel_medio", 5.4);
    near(&r, "fluidos.bomba.volume", 6.0);
}
