//! Indicators of Manufatura discreta on small cases with answers known by hand.

use super::*;

fn run(text: &str) -> Manufatura {
    let m = Model::parse(text).unwrap();
    Manufatura::rodar(&m, None, Meta::default()).unwrap()
}

fn near(what: &str, got: f64, want: f64) {
    assert!((got - want).abs() < 1e-9, "{what}: {got}, esperado {want}");
}

/// Orders of products 1, 1, 2, 1 at t = 0, 10, 20, 30, each with 5 minutes of due date;
/// one machine with process 2 that changes over only when the product changes (1 -> 2 takes
/// 3 from the matrix, 2 -> 1 the default 5).
const TROCA: &str = r#"Model(name: "troca", run: (stop_time: 100.0), objects: [
    (name: "liberacao", kind: Source(interarrival: Const(10.0), first_at_zero: true,
        labels: {"prazo": Const(5.0)},
        table: (mode: Sequence, rows: [
            (count: 1, labels: {"produto": 1.0}),
            (count: 1, labels: {"produto": 1.0}),
            (count: 1, labels: {"produto": 2.0}),
            (count: 1, labels: {"produto": 1.0}),
        ]))),
    (name: "maquina", kind: Processor(setup: Some(Const(5.0)), process: Const(2.0),
        changeover: Some((label: "produto", matrix: [(from: 1.0, to: 2.0, time: Const(3.0))])))),
    (name: "expedicao", kind: Sink),
], connections: [("liberacao", "maquina"), ("maquina", "expedicao")])"#;

#[test]
fn lead_time_due_dates_and_oee_by_hand() {
    // Orders: 0..2, 10..12, 20..23 change + 23..25, 30..35 change + 35..37. Leads 2, 2, 5
    // and 7; due at creation + 5, so only the last is late, by 2.
    let k = run(TROCA);
    let r = &k.reps[0];
    assert_eq!(r.ordens, 4);
    near("atravessamento", r.atravessamento, 4.0);
    near("wip", r.wip, 16.0 / 100.0);
    near("no prazo", r.no_prazo.unwrap(), 0.75);
    near("atraso", r.atraso.unwrap(), 2.0);
    assert_eq!(r.por_produto[&1].ordens, 3);
    near("produto 1", r.por_produto[&1].atravessamento, 11.0 / 3.0);
    near("produto 1 prazo", r.por_produto[&1].no_prazo.unwrap(), 2.0 / 3.0);
    near("produto 2 prazo", r.por_produto[&2].no_prazo.unwrap(), 1.0);
    // OEE: 8 of 100 minutes in changeover, so availability 0.92; 4 parts of 2 minutes in
    // 92 running minutes; no scrap.
    let m = &r.maquinas["maquina"];
    near("disponibilidade", m.disponibilidade, 0.92);
    near("desempenho", m.desempenho, 8.0 / 92.0);
    near("qualidade", m.qualidade, 1.0);
    near("oee", m.oee, 0.08);
    near("em troca", m.em_troca, 0.08);
    near("trocas por dia", m.trocas_dia.unwrap(), 2.0 * 1440.0 / 100.0);
}

#[test]
fn scrap_counts_against_quality() {
    // Two orders at 0 and 10, process 2; the second goes to the scrap sink.
    let k = run(r#"Model(name: "refugo", run: (stop_time: 100.0), objects: [
        (name: "liberacao", kind: Source(interarrival: Const(10.0), first_at_zero: true,
            table: (mode: Sequence, rows: [
                (count: 1, labels: {"destino": 1.0}),
                (count: 1, labels: {"destino": 2.0}),
            ]))),
        (name: "maquina", kind: Processor(process: Const(2.0)), output: ByLabel("destino")),
        (name: "expedicao", kind: Sink),
        (name: "refugo", kind: Sink, sink_role: Some(Scrap)),
    ], connections: [("liberacao", "maquina"), ("maquina", "expedicao"), ("maquina", "refugo")])"#);
    let r = &k.reps[0];
    assert_eq!((r.ordens, r.refugo), (1, 1));
    assert_eq!(r.no_prazo, None);
    let m = &r.maquinas["maquina"];
    near("qualidade", m.qualidade, 0.5);
    near("desempenho", m.desempenho, 4.0 / 100.0);
    near("oee", m.oee, 0.02);
    assert_eq!(m.trocas_dia, None);
}

#[test]
fn a_card_loop_holds_the_orders_and_cards_are_not_orders() {
    // One card. Orders at 0 and 1; the first takes the card at 0 and works 0..10, the card
    // comes back at 10 and the second works 10..20. Leads 10 and 19; the card loop is not a
    // machine and the card is not an order.
    let k = run(r#"Model(name: "conwip", run: (stop_time: 100.0), objects: [
        (name: "liberacao", kind: Source(interarrival: Const(1.0), first_at_zero: true, max_arrivals: Some(2))),
        (name: "conwip", kind: Queue(), preset: "manufatura/cartoes"),
        (name: "conwip_liberar", kind: Combiner(components: [1], mode: Pack, process: Const(0.0)),
            preset: "manufatura/cartoes"),
        (name: "conwip_cartoes", kind: Source(interarrival: Const(0.01), first_at_zero: true,
            max_arrivals: Some(1), labels: {"cartao": Const(1.0)}), preset: "manufatura/cartoes"),
        (name: "conwip_quadro", kind: Queue(), preset: "manufatura/cartoes"),
        (name: "conwip_devolver", kind: Separator(mode: Unpack, process: Const(0.0)),
            output: Script("if label(item, \"cartao\", 0.0) == 1.0 { 1 } else { 2 }"),
            preset: "manufatura/cartoes"),
        (name: "fila", kind: Queue()),
        (name: "maquina", kind: Processor(process: Const(10.0))),
        (name: "expedicao", kind: Sink),
    ], connections: [
        ("liberacao", "conwip"),
        ("conwip", "conwip_liberar"),
        ("conwip_cartoes", "conwip_quadro"),
        ("conwip_quadro", "conwip_liberar"),
        ("conwip_devolver", "conwip_quadro"),
        ("conwip_liberar", "fila"),
        ("fila", "maquina"),
        ("maquina", "conwip_devolver"),
        ("conwip_devolver", "expedicao"),
    ])"#);
    let r = &k.reps[0];
    assert_eq!(r.ordens, 2);
    near("atravessamento", r.atravessamento, 14.5);
    near("wip", r.wip, 29.0 / 100.0);
    assert_eq!(r.maquinas.keys().collect::<Vec<_>>(), vec!["maquina"]);
    near("desempenho", r.maquinas["maquina"].desempenho, 0.2);
}
