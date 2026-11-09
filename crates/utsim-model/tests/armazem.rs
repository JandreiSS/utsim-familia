//! Warehouse pieces of the core (`docs/plano_linhas_especializadas.md`, 8.2): an order split
//! into its lines (`SplitBy`), the order put together again (`gather`), wave release
//! (`waves`) and ABC slotting with access times in a rack, on cases known by hand.

use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).unwrap();
    m.validate().unwrap();
    m
}

/// One order of 3 lines at t = 0: split in 1 minute, each line picked in 2 (one picker),
/// then consolidated.
const ORDER: &str = r#"Model(name: "pedido", run: (stop_time: 100.0), objects: [
    (name: "pedidos", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"linhas": Const(3.0)})),
    (name: "linhas", kind: Separator(mode: SplitBy(count: "linhas", group: "pedido"), process: Const(1.0))),
    (name: "separacao", kind: Processor(process: Const(2.0))),
    (name: "consolidacao", kind: Queue(gather: Some((group: "pedido", count: "linhas")))),
    (name: "expedicao", kind: Sink),
], connections: [("pedidos", "linhas"), ("linhas", "separacao"), ("separacao", "consolidacao"),
                 ("consolidacao", "expedicao")])"#;

#[test]
fn an_order_is_split_into_its_lines_and_put_together_again() {
    let m = model(ORDER);
    let (report, items) = m.run_item_log(0).unwrap();
    // Three lines picked one after the other: 1-3, 3-5, 5-7; the order ships at 7.
    assert_eq!(report.block("separacao").unwrap().input, 3);
    assert_eq!(report.block("expedicao").unwrap().input, 1);
    let shipped = items.iter().find(|r| r.block == Some(4)).unwrap();
    assert_eq!(shipped.ended_at, Some(7.0));
    // Every line has the order number and its index.
    let mut lines: Vec<(f64, f64)> = items
        .iter()
        .map(|r| {
            let n = |k: &str| r.labels.get(k).and_then(|v| v.as_num()).unwrap();
            (n("pedido"), n("linha"))
        })
        .collect();
    lines.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert_eq!(lines, vec![(1.0, 1.0), (1.0, 2.0), (1.0, 3.0)]);
    // The other lines left the model inside the order.
    assert!(items.iter().all(|r| r.ended_at == Some(7.0)));
}

#[test]
fn lines_of_different_orders_wait_for_their_own_order() {
    // Two orders, of 2 and 1 lines; the single line of order 2 does not complete order 1.
    let text = ORDER
        .replace(
            r#"max_arrivals: 1,
        labels: {"linhas": Const(3.0)}"#,
            r#"max_arrivals: 2,
        labels: {"linhas": Discrete([(2.0, 1.0)])}"#,
        )
        .replace(
            "Const(1000.0), first_at_zero: true",
            "Const(0.5), first_at_zero: true",
        );
    let m = model(&text);
    let (report, _) = m.run_item_log(0).unwrap();
    assert_eq!(report.block("separacao").unwrap().input, 4);
    assert_eq!(report.block("expedicao").unwrap().input, 2);
}

#[test]
fn waves_release_everything_waiting_at_their_times() {
    // Arrivals at 1..7; a wave at 4.5 of each 10: 1-4 leave at 4.5, 5-7 at 14.5.
    let m = model(
        r#"Model(name: "ondas", run: (stop_time: 30.0), objects: [
    (name: "pedidos", kind: Source(interarrival: Const(1.0), max_arrivals: 7)),
    (name: "onda", kind: Queue(waves: Some((cycle: 10.0, times: [4.5])))),
    (name: "saida", kind: Sink),
], connections: [("pedidos", "onda"), ("onda", "saida")])"#,
    );
    let (report, items) = m.run_item_log(0).unwrap();
    let mut ends: Vec<f64> = items.iter().filter_map(|r| r.ended_at).collect();
    ends.sort_by(f64::total_cmp);
    assert_eq!(ends, vec![4.5, 4.5, 4.5, 4.5, 14.5, 14.5, 14.5]);
    assert_eq!(report.block("onda").unwrap().custom["waves.count"], 2.0);
}

#[test]
fn abc_slotting_puts_class_a_where_access_is_fastest() {
    // One level of 4 bays, 1 minute per bay from the front; A takes 1/4 of the positions,
    // B the next 1/4, C the rest. Three class C pallets at t = 0, 1, 2; dwell 10.
    let m = model(
        r#"Model(name: "abc", run: (stop_time: 100.0), objects: [
    (name: "paletes", kind: Source(interarrival: Const(1.0), first_at_zero: true, max_arrivals: 3,
        labels: {"classe": Discrete([(3.0, 1.0)])})),
    (name: "porta_paletes", kind: Rack(bays: 4, levels: 1, dwell: Some(Const(10.0)),
        placement: Abc(a: 0.25, b: 0.25), access: Some((per_bay: 1.0, per_level: 0.0)))),
    (name: "saida", kind: Sink),
], connections: [("paletes", "porta_paletes"), ("porta_paletes", "saida")])"#,
    );
    let (report, _) = m.run_item_log(0).unwrap();
    let rack = report.block("porta_paletes").unwrap();
    // All class C: bays 2 and 3 first (access 2 and 3), then B's bay 1 when C is full.
    assert_eq!(rack.extra["access_time.C"].count, 3);
    assert!((rack.extra["access_time"].mean - (2.0 + 3.0 + 1.0) / 3.0).abs() < 1e-9);

    let m = model(
        r#"Model(name: "abc", run: (stop_time: 100.0), objects: [
    (name: "a", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"classe": Const(1.0)})),
    (name: "b", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"classe": Const(2.0)})),
    (name: "c", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"classe": Const(3.0)})),
    (name: "a2", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"classe": Const(1.0)})),
    (name: "porta_paletes", kind: Rack(bays: 4, levels: 1, dwell: Some(Const(10.0)),
        placement: Abc(a: 0.25, b: 0.25), access: Some((per_bay: 1.0, per_level: 0.0)))),
    (name: "saida", kind: Sink),
], connections: [("a", "porta_paletes"), ("b", "porta_paletes"), ("c", "porta_paletes"),
                 ("a2", "porta_paletes"), ("porta_paletes", "saida")])"#,
    );
    let (report, items) = m.run_item_log(0).unwrap();
    let rack = report.block("porta_paletes").unwrap();
    // A in bay 0 (10), B in bay 1 (11), C in bay 2 (12); the second A finds its zone
    // full and takes the next worse zone that has room: C's bay 3 (13).
    assert_eq!(rack.extra["access_time.A"].count, 2);
    assert!((rack.extra["access_time.A"].mean - 1.5).abs() < 1e-9);
    assert!((rack.extra["access_time.B"].mean - 1.0).abs() < 1e-9);
    assert!((rack.extra["access_time.C"].mean - 2.0).abs() < 1e-9);
    let mut ends: Vec<f64> = items.iter().filter_map(|r| r.ended_at).collect();
    ends.sort_by(f64::total_cmp);
    assert_eq!(ends, vec![10.0, 11.0, 12.0, 13.0]);
}

#[test]
fn without_the_new_fields_nothing_changes() {
    let m = model(
        r#"Model(name: "simples", run: (stop_time: 50.0), objects: [
    (name: "pedidos", kind: Source(interarrival: Const(1.0))),
    (name: "fila", kind: Queue()),
    (name: "porta_paletes", kind: Rack(bays: 4, levels: 2, dwell: Some(Const(3.0)))),
    (name: "saida", kind: Sink),
], connections: [("pedidos", "fila"), ("fila", "porta_paletes"), ("porta_paletes", "saida")])"#,
    );
    let r = &m.run_replications().unwrap()[0];
    let rack = r.block("porta_paletes").unwrap();
    assert!(!rack.extra.contains_key("access_time"));
    assert!(!r.block("fila").unwrap().custom.contains_key("waves.count"));
}
