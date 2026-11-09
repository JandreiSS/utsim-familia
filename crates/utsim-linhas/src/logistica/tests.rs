//! Indicators of Logística e armazém on small cases with answers known by hand.

use super::*;

fn run(text: &str, meta: Meta) -> Armazem {
    let m = Model::parse(text).unwrap();
    Armazem::rodar(&m, None, meta).unwrap()
}

/// Two trucks of 3 pallets at t = 0 and 1 into one dock; each pallet takes 2 minutes to
/// unload, then goes straight to storage.
const DOCA: &str = r#"Model(name: "doca", run: (stop_time: 100.0), objects: [
    (name: "caminhoes", kind: Source(interarrival: Const(1.0), first_at_zero: true, max_arrivals: 2,
        labels: {"paletes": Const(3.0)})),
    (name: "patio", kind: Queue()),
    (name: "doca", kind: Separator(mode: SplitBy(count: "paletes", group: "caminhao"), process: Const(1.0))),
    (name: "descarga", kind: Processor(process: Const(2.0))),
    (name: "estoque", kind: Sink),
], connections: [("caminhoes", "patio"), ("patio", "doca"), ("doca", "descarga"),
                 ("descarga", "estoque")])"#;

#[test]
fn dock_time_and_yard_wait_by_hand() {
    let k = run(DOCA, Meta { doca: 6.0 });
    let r = &k.reps[0];
    // Truck 1 docks at 0, split at 1; pallets leave the dock at 1, 3, 5 (one unloader):
    // dock time 5. Truck 2 waits in the yard until 5, split at 6, pallets out at 7, 9,
    // 11: wait 4, dock time 6.
    assert_eq!(r.caminhoes, 2);
    assert_eq!(r.paletes, 6);
    assert!((r.patio - (0.0 + 4.0) / 2.0).abs() < 1e-9, "{}", r.patio);
    assert!((r.doca - (5.0 + 6.0) / 2.0).abs() < 1e-9, "{}", r.doca);
    assert_eq!(r.doca_na_meta, 1.0);
    let k = run(DOCA, Meta { doca: 5.5 });
    assert_eq!(k.reps[0].doca_na_meta, 0.5);
    // The dock had a truck from 0 to 11 of the 100 minutes.
    assert!((k.reps[0].docas["doca"] - 0.11).abs() < 1e-9);
}

/// One order of 3 lines at t = 0 with a promise of 6 minutes.
const PEDIDO: &str = r#"Model(name: "pedido", run: (stop_time: 120.0), objects: [
    (name: "pedidos", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"linhas": Const(3.0), "sla": Const(6.0)})),
    (name: "linhas", kind: Separator(mode: SplitBy(count: "linhas", group: "pedido"), process: Const(1.0))),
    (name: "separacao", kind: Processor(process: Const(2.0))),
    (name: "consolidacao", kind: Queue(gather: Some((group: "pedido", count: "linhas")))),
    (name: "expedicao", kind: Sink),
], connections: [("pedidos", "linhas"), ("linhas", "separacao"), ("separacao", "consolidacao"),
                 ("consolidacao", "expedicao")])"#;

#[test]
fn lines_per_hour_and_orders_by_hand() {
    let k = run(PEDIDO, Meta::default());
    let r = &k.reps[0];
    // Three lines in 2 hours: 1,5 per hour; all in hour 0 of day 1.
    assert_eq!(r.linhas, 3);
    assert!((r.linhas_hora - 1.5).abs() < 1e-9);
    assert_eq!(k.linhas_por_hora, vec![(0, 3.0)]);
    // The order ships at 7: cycle 7, 3 lines, late for a promise of 6.
    assert_eq!(r.pedidos, 1);
    assert!((r.ciclo - 7.0).abs() < 1e-9);
    assert!((r.linhas_por_pedido - 3.0).abs() < 1e-9);
    assert_eq!(r.no_prazo, Some(0.0));
    assert!((r.pedidos_dia - 12.0).abs() < 1e-9);
    // No dock and no rack.
    assert_eq!(r.caminhoes, 0);
    assert!(r.porta_paletes.is_empty());
    let t = relatorio::texto(&k);
    assert!(t.contains("Linhas separadas por hora"));
    assert!(relatorio::html(&k).contains("<svg"));
}

#[test]
fn rack_occupancy_and_abc_access() {
    let k = run(
        r#"Model(name: "abc", run: (stop_time: 40.0), objects: [
    (name: "paletes", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"classe": Const(1.0)})),
    (name: "porta_paletes", kind: Rack(bays: 4, levels: 1, dwell: Some(Const(10.0)),
        placement: Abc(a: 0.25, b: 0.25), access: Some((per_bay: 1.0, per_level: 0.0)))),
    (name: "saida", kind: Sink),
], connections: [("paletes", "porta_paletes"), ("porta_paletes", "saida")])"#,
        Meta::default(),
    );
    let r = &k.reps[0];
    // One pallet for 10 of 40 minutes in 4 positions: 1/16.
    assert!((r.porta_paletes["porta_paletes"] - 10.0 / 40.0 / 4.0).abs() < 1e-9);
    assert_eq!(r.acesso["A"], 0.0);
    assert!(!r.acesso.contains_key("B"));
}
