//! A real waiting area on a queue (`waiting_area`): each item takes a place of the grid
//! (traced as `Slot`), the item on top leaves before the one under it, a freed place stays
//! empty until the next arrival, and the carrier goes to the front of the item it takes.

use utsim_core::TraceEvent;
use utsim_model::{Model, WaitingArea, WaitingKind};

/// Items every second into a queue, then a machine of 10 s: the queue fills while the
/// machine works. `area` is the queue's `waiting_area` (RON).
fn model(area: &str) -> Model {
    let text = format!(
        r#"Model(
            name: "espera real",
            run: (stop_time: 60.0, seed: 1, replications: 1),
            objects: [
                (name: "fonte", kind: Source(interarrival: Const(1.0), first_at_zero: true, max_arrivals: Some(5)),
                 position: (0.0, 0.0, 0.0)),
                (name: "fila", kind: Queue(), position: (4.0, 0.0, 0.0){area}),
                (name: "maquina", kind: Processor(process: Const(10.0)), position: (8.0, 0.0, 0.0)),
                (name: "saida", kind: Sink, position: (12.0, 0.0, 0.0)),
            ],
            connections: [("fonte", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#
    );
    let m = Model::parse(&text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

/// The order the items left the queue for the machine (by their creation order, 1-based).
fn order_out(m: &Model) -> Vec<usize> {
    let r = m.replay(0).unwrap();
    let fila = r.blocks.iter().position(|b| b.name == "fila").unwrap();
    let mut created = Vec::new();
    let mut out = Vec::new();
    for rec in &r.records {
        match &rec.event {
            TraceEvent::Created { item, .. } => created.push(*item),
            TraceEvent::Moved { item, from, .. } if *from == fila => {
                out.push(created.iter().position(|c| c == item).unwrap() + 1);
            }
            _ => {}
        }
    }
    out
}

#[test]
fn without_a_waiting_area_the_queue_is_fifo_as_before() {
    assert_eq!(order_out(&model("")), vec![1, 2, 3, 4, 5]);
}

#[test]
fn the_item_on_top_leaves_before_the_one_under_it() {
    // One column, two layers (pitch larger than half the table): 2 under, 3 on top.
    let m = model(", waiting_area: Some((kind: Table, pitch: Some(0.6)))");
    let places = m
        .objects
        .iter()
        .find(|o| o.name == "fila")
        .and_then(|o| o.waiting_area)
        .unwrap()
        .places(None);
    assert_eq!(places.len(), 2, "{places:?}");
    // 1 goes straight to the machine; 2 and 3 wait stacked; 4 and 5 wait beside the
    // table. At 10 s the machine takes 3 (on top), then the one beside the table that
    // took its place (4), and so on: 2 waits under the others until the end.
    let out = order_out(&m);
    assert_eq!(out[0], 1);
    assert_eq!(out[1], 3, "{out:?}");
    assert_eq!(*out.last().unwrap(), 2, "{out:?}");
}

#[test]
fn every_item_on_the_area_is_traced_on_its_place_and_a_freed_place_is_reused() {
    let m = model(", waiting_area: Some((kind: Table))");
    let r = m.replay(0).unwrap();
    let fila = r.blocks.iter().position(|b| b.name == "fila").unwrap();
    let slots: Vec<usize> = r
        .records
        .iter()
        .filter_map(|rec| match &rec.event {
            TraceEvent::Slot { block, slot, .. } if *block == fila => Some(*slot),
            _ => None,
        })
        .collect();
    // Five items, each on the first free place: 0 for the first (it leaves at once),
    // then 0 again, 1, 2, 3.
    assert_eq!(slots, vec![0, 0, 1, 2, 3]);
    // FIFO among free items: a single layer here, so the order is kept.
    assert_eq!(order_out(&m), vec![1, 2, 3, 4, 5]);
}

#[test]
fn a_waiting_area_is_checked() {
    let bad = |area: &str| {
        let text = format!(
            r#"Model(name: "x", run: (stop_time: 1.0), objects: [
                (name: "f", kind: Source(interarrival: Const(1.0)), position: (0.0, 0.0, 0.0)),
                (name: "q", kind: Queue({area}), position: (2.0, 0.0, 0.0), waiting_area: Some((kind: Pallet, layers: Some(0)))),
                (name: "s", kind: Sink, position: (4.0, 0.0, 0.0)),
            ], connections: [("f", "q"), ("q", "s")])"#
        );
        Model::parse(&text)
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string()
    };
    assert!(bad("").contains("layers"), "{}", bad(""));
    assert!(bad("batch: Some((size: 2))").contains("lote"));
    // The kinds read back by name.
    for k in WaitingKind::ALL {
        assert_eq!(WaitingKind::by_name(k.name()), Some(k));
    }
    let w = WaitingArea {
        kind: WaitingKind::Shelf,
        ..Default::default()
    };
    assert_eq!(w.element(), "estante_espera");
}
