//! Indicators of Processos contínuos e batelada on small cases with answers known by hand.

use super::*;

fn run(text: &str) -> Processos {
    let m = Model::parse(text).unwrap();
    Processos::rodar(&m, None, Meta::default()).unwrap()
}

fn near(what: &str, got: f64, want: f64) {
    assert!((got - want).abs() < 1e-9, "{what}: {got}, esperado {want}");
}

/// Batches of products 1, 1, 2, 1 at t = 0, 10, 20, 30, 100 L each with yield 0.9; one
/// reactor with recipe 1 -> 4 and 2 -> 6 that cleans (3) only when the product changes; every
/// batch that ends becomes 10 L in a product tank that nothing empties.
const LOTES: &str = r#"Model(name: "lotes", run: (stop_time: 100.0), objects: [
    (name: "programacao", kind: Source(interarrival: Const(10.0), first_at_zero: true,
        labels: {"volume": Const(100.0), "rendimento": Const(0.9)},
        table: (mode: Sequence, rows: [
            (count: 1, labels: {"produto": 1.0}),
            (count: 1, labels: {"produto": 1.0}),
            (count: 1, labels: {"produto": 2.0}),
            (count: 1, labels: {"produto": 1.0}),
        ]))),
    (name: "reator", kind: Processor(setup: Some(Const(3.0)), process: Const(1.0),
        changeover: Some((label: "produto")),
        recipe: Some((label: "produto", times: [(value: 1.0, time: Const(4.0)), (value: 2.0, time: Const(6.0))])))),
    (name: "produto", kind: Sink),
], connections: [("programacao", "reator"), ("reator", "produto")],
fluids: Some((nodes: [
    (name: "descarga", kind: FromItems(object: "produto", volume: 10.0)),
    (name: "tanque", kind: Tank(capacity: 100.0)),
], links: [("descarga", "tanque")])))"#;

#[test]
fn batches_volume_yield_and_cleaning_by_hand() {
    // Batches: 0..4, 10..14, 20..23 cleaning + 23..29, 30..33 cleaning + 33..37. Leads 4,
    // 4, 9 and 7.
    let k = run(LOTES);
    let r = &k.reps[0];
    let dias = 100.0 / 1440.0;
    assert_eq!(r.lotes, 4);
    near("lotes por dia", r.lotes_dia, 4.0 / dias);
    near("volume por dia", r.volume_dia.unwrap(), 4.0 * 90.0 / dias);
    near("rendimento", r.rendimento.unwrap(), 0.9);
    near("atravessamento", r.atravessamento, 6.0);
    assert_eq!(r.por_produto[&1].lotes, 3);
    near("volume produto 1", r.por_produto[&1].volume, 270.0);
    near("produto 1", r.por_produto[&1].atravessamento, 5.0);
    let m = &r.reatores["reator"];
    near("produzindo", m.produzindo, 0.18);
    near("em limpeza", m.em_limpeza, 0.06);
    near("sem lote", m.sem_lote, 0.76);
    near("limpezas por dia", m.limpezas_dia.unwrap(), 2.0 / dias);
    near("ciclo", m.ciclo, 6.0);
    // The tank gets 10 L at 4, 14, 29 and 37: mean level (96 + 86 + 71 + 63) / 10 L.
    near("tanque", r.tanques["tanque"], 31.6 / 100.0);
}

#[test]
fn batches_out_of_specification_do_not_count_as_product() {
    // Two batches at 0 and 10; the second goes to the scrap sink. No volume label.
    let k = run(r#"Model(name: "fora", run: (stop_time: 100.0), objects: [
        (name: "programacao", kind: Source(interarrival: Const(10.0), first_at_zero: true,
            table: (mode: Sequence, rows: [
                (count: 1, labels: {"destino": 1.0}),
                (count: 1, labels: {"destino": 2.0}),
            ]))),
        (name: "reator", kind: Processor(process: Const(2.0)), output: ByLabel("destino")),
        (name: "produto", kind: Sink),
        (name: "descarte", kind: Sink, sink_role: Some(Scrap)),
    ], connections: [("programacao", "reator"), ("reator", "produto"), ("reator", "descarte")])"#);
    let r = &k.reps[0];
    assert_eq!((r.lotes, r.fora), (1, 1));
    assert_eq!(r.volume_dia, None);
    assert_eq!(r.rendimento, None);
    assert_eq!(r.reatores["reator"].limpezas_dia, None);
    assert!(r.tanques.is_empty());
    let texto = relatorio::texto(&k);
    assert!(texto.contains("Lotes fora de especificação"), "{texto}");
    assert!(!texto.contains("Volume produzido"), "{texto}");
}
