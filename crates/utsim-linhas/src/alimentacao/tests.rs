//! Indicators of Alimentação e food service on small cases with answers known by hand.

use super::*;

fn run(text: &str, meta: Meta) -> Alimentacao {
    let m = Model::parse(text).unwrap();
    Alimentacao::rodar(&m, None, meta).unwrap()
}

fn near(what: &str, got: f64, want: f64) {
    assert!((got - want).abs() < 1e-9, "{what}: {got}, esperado {want}");
}

/// Delivery orders at 0, 10 and 20 with 2, 1 and 2 parts. The comanda sends part 1 to the
/// grill (4 min) and part 2 to the drinks (1 min); the assembly waits for both.
const COMANDA: &str = r#"Model(name: "comanda", run: (stop_time: 60.0), objects: [
    (name: "pedidos", kind: Source(interarrival: Const(10.0), first_at_zero: true,
        labels: {"canal": Const(3.0)},
        table: (mode: Sequence, rows: [
            (count: 1, labels: {"itens": 2.0}),
            (count: 1, labels: {"itens": 1.0}),
            (count: 1, labels: {"itens": 2.0}),
        ]))),
    (name: "comanda", kind: Separator(mode: SplitBy(count: "itens", group: "comanda"), process: Const(0.0)),
        output: ByLabel("linha")),
    (name: "chapa", kind: Processor(process: Const(4.0))),
    (name: "bebidas", kind: Processor(process: Const(1.0))),
    (name: "montagem", kind: Queue(gather: Some((group: "comanda", count: "itens")))),
    (name: "saida", kind: Sink),
], connections: [("pedidos", "comanda"), ("comanda", "chapa"), ("comanda", "bebidas"),
    ("chapa", "montagem"), ("bebidas", "montagem"), ("montagem", "saida")])"#;

#[test]
fn split_orders_count_once_with_their_whole_time() {
    // Every order takes the 4 minutes of the grill: 0..4, 10..14, 20..24.
    let k = run(COMANDA, Meta { prazo: 4.0 });
    let r = &k.reps[0];
    let dias = 60.0 / 1440.0;
    assert_eq!(r.pedidos, 3);
    near("pedidos por dia", r.pedidos_dia, 3.0 / dias);
    near("tempo", r.tempo, 4.0);
    near("p90", r.tempo_p90, 4.0);
    near("no prazo", r.no_prazo, 1.0);
    assert_eq!((r.desistencias, r.descartes), (0, 0));
    assert_eq!(r.por_canal.keys().copied().collect::<Vec<_>>(), vec![3]);
    assert_eq!(r.por_canal[&3].pedidos, 3);
    assert_eq!(r.por_hora, vec![(0, 3.0 / dias)]);
    assert_eq!(r.pico(), Some((0, 3.0 / dias)));
    // Grill: one place, 12 of 60 minutes; drinks: 2 of 60.
    near("chapa", r.estacoes["chapa"].ocupacao, 0.2);
    assert_eq!(r.estacoes["chapa"].feitas, 3);
    near("bebidas", r.estacoes["bebidas"].ocupacao, 2.0 / 60.0);
    let texto = relatorio::texto(&k);
    assert!(texto.contains("Pedidos no prazo (até 4 min)"), "{texto}");
    assert!(!texto.contains("Desistências"), "{texto}");
}

#[test]
fn giving_up_and_cold_food_are_told_apart() {
    // Two customers at 0 and 1; the cashier takes 10 minutes and the second one gives up
    // after 3 minutes in the queue.
    let k = run(
        r#"Model(name: "fila", run: (stop_time: 50.0), objects: [
            (name: "clientes", kind: Source(interarrival: Const(1.0), first_at_zero: true,
                table: (mode: Sequence, rows: [(count: 2, labels: {})]))),
            (name: "fila", kind: Queue(patience: Some(Const(3.0)))),
            (name: "caixa", kind: Processor(process: Const(10.0))),
            (name: "saida", kind: Sink),
        ], connections: [("clientes", "fila"), ("fila", "caixa"), ("caixa", "saida")])"#,
        Meta::default(),
    );
    let r = &k.reps[0];
    assert_eq!((r.pedidos, r.desistencias, r.descartes), (1, 1, 0));
    near("desistência", r.desistencia, 0.5);
    near("tempo", r.tempo, 10.0);

    // Two orders at 0 and 1 ready at the pickup counter; one courier (10 minutes). The
    // second order gets cold after 2 minutes and is thrown away.
    let k = run(
        r#"Model(name: "retirada", run: (stop_time: 50.0), objects: [
            (name: "pedidos", kind: Source(interarrival: Const(1.0), first_at_zero: true,
                table: (mode: Sequence, rows: [(count: 2, labels: {})]))),
            (name: "retirada", kind: Queue(patience: Some(Const(2.0))), preset: "alimentacao/retirada"),
            (name: "entrega", kind: Processor(process: Const(10.0))),
            (name: "saida", kind: Sink),
        ], connections: [("pedidos", "retirada"), ("retirada", "entrega"), ("entrega", "saida")])"#,
        Meta::default(),
    );
    let r = &k.reps[0];
    assert_eq!((r.pedidos, r.desistencias, r.descartes), (1, 0, 1));
    near("desistência", r.desistencia, 0.0);
    assert_eq!(r.por_canal[&1].descartes, 1);
    let texto = relatorio::texto(&k);
    assert!(texto.contains("descartados"), "{texto}");
}

#[test]
fn tables_occupancy_and_turnover_by_hand() {
    // Two groups at 0 and 10 at a room with two tables, 30 minutes each: tables taken 1 in
    // 0..10, 2 in 10..30, 1 in 30..40 and 0 in 40..60, a mean of 1 out of 2.
    let k = run(
        r#"Model(name: "mesas", run: (stop_time: 60.0), objects: [
            (name: "grupos", kind: Source(interarrival: Const(10.0), first_at_zero: true,
                table: (mode: Sequence, rows: [(count: 2, labels: {})]))),
            (name: "salao", kind: Processor(capacity: 2, process: Const(30.0)), preset: "alimentacao/mesas"),
            (name: "saida", kind: Sink),
        ], connections: [("grupos", "salao"), ("salao", "saida")])"#,
        Meta::default(),
    );
    let r = &k.reps[0];
    let s = &r.mesas["salao"];
    assert_eq!(s.mesas, 2);
    near("ocupação", s.ocupacao, 0.5);
    near("giro", s.giro_dia, 2.0 / 2.0 / (60.0 / 1440.0));
    near("refeição", s.refeicao, 30.0);
    assert!(!r.estacoes.contains_key("salao"));
    let texto = relatorio::texto(&k);
    assert!(texto.contains("Salões"), "{texto}");
}
