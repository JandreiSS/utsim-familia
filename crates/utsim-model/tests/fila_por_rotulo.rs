//! Queue order by label (`order: Desc("p")` / `Asc("p")`): items leave sorted by the
//! label, ties in arrival order, items without the label last.

use std::collections::BTreeMap;

use utsim_core::TraceEvent;
use utsim_model::Model;

/// Six items arrive 0.1 apart; the first goes straight to the slow processor, the other
/// five wait in the queue with `p = k mod 3` (k = 1..5: p = 1, 2, 0, 1, 2). Item 4 has no
/// label in the `unlabelled` variant.
fn model(order: &str, unlabelled: bool) -> Model {
    let script = if unlabelled {
        "let k = (now * 10.0).round(); if k != 4.0 { item.p = k % 3.0; }"
    } else {
        "let k = (now * 10.0).round(); item.p = k % 3.0;"
    };
    let text = format!(
        r#"Model(name: "fila", run: (stop_time: 1000.0), objects: [
            (name: "chegada", kind: Source(interarrival: Const(0.1), first_at_zero: true, max_arrivals: Some(6))),
            (name: "fila", kind: Queue({order}), on_entry: Some("{script}")),
            (name: "posto", kind: Processor(process: Const(10.0))),
            (name: "fim", kind: Sink),
        ], connections: [("chegada", "fila"), ("fila", "posto"), ("posto", "fim")])"#
    );
    Model::parse(&text).unwrap()
}

/// Arrival number (k) of the items in the order they left the queue.
fn exit_order(m: &Model) -> Vec<u32> {
    m.validate().unwrap();
    let replay = m.replay(0).unwrap();
    let mut born: BTreeMap<u64, u32> = BTreeMap::new();
    let mut out = Vec::new();
    for r in &replay.records {
        match &r.event {
            TraceEvent::Created { item, .. } => {
                born.insert(*item, (r.t * 10.0).round() as u32);
            }
            TraceEvent::Moved { item, from: 1, .. } => out.push(born[item]),
            _ => {}
        }
    }
    out
}

#[test]
fn descending_label_first_ties_in_arrival_order() {
    assert_eq!(
        exit_order(&model("order: Desc(\"p\")", false)),
        [0, 2, 5, 1, 4, 3]
    );
}

#[test]
fn ascending_label_first() {
    assert_eq!(
        exit_order(&model("order: Asc(\"p\")", false)),
        [0, 3, 1, 4, 2, 5]
    );
}

#[test]
fn without_order_the_queue_is_fifo() {
    assert_eq!(exit_order(&model("", false)), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn items_without_the_label_go_last() {
    for order in ["order: Desc(\"p\")", "order: Asc(\"p\")"] {
        let got = exit_order(&model(order, true));
        assert_eq!(got.last(), Some(&4), "{order}: {got:?}");
    }
}

#[test]
fn order_and_lifo_together_are_refused() {
    let m = model("discipline: Lifo, order: Desc(\"p\")", false);
    let err = m.validate().unwrap_err().to_string();
    assert!(err.contains("não os dois"), "{err}");
    let m = model("order: Desc(\" \")", false);
    assert!(m.validate().is_err());
}
