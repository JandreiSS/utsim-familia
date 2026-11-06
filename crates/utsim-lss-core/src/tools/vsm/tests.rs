use super::*;
use crate::programa::testkit::*;
use crate::tools::{Deliverable, Tool, item_ref};

fn map(t: &mut T) -> Id {
    let p = t.program;
    let del = t.id();
    t.commit(Deliverable::create(p, None, Tool::Vsm, del, "VSM atual"));
    del
}

fn row(t: &mut T, del: Id, name: &str, cycle: f64, uptime: f64, inv: Option<f64>, va: bool) -> Id {
    let p = t.program;
    let id = t.id();
    let mut extra = vec![
        (fields::NAME, Value::from(name)),
        (fields::CYCLE, Value::from(cycle)),
        (fields::UPTIME, Value::from(uptime)),
        (fields::VALUE_ADDED, Value::from(va)),
    ];
    if let Some(i) = inv {
        extra.push((fields::INVENTORY, Value::from(i)));
    }
    let e = new_item(&t.s, p, del, PROCESS_ITEM, id, None, extra);
    t.commit(e);
    id
}

#[test]
fn ladder_by_hand() {
    let mut t = T::new();
    let del = map(&mut t);
    // 480 min a day for 240 units: takt 2 min.
    t.commit(header_edits(
        del,
        Some(240.0),
        Some(480.0),
        Some("min"),
        Some(false),
    ));
    row(&mut t, del, "Corte", 1.5, 100.0, None, true);
    row(&mut t, del, "Solda", 1.8, 80.0, Some(120.0), true);
    row(&mut t, del, "Inspeção", 0.5, 100.0, Some(60.0), false);
    let h = header(&t.s, del);
    assert!(!h.future);
    assert_eq!(h.unit, "min");
    let procs = processes(&t.s, del);
    assert_eq!(procs.len(), 3);
    let l = ladder(&procs, &h);
    assert_eq!(l.takt, Some(2.0));
    // Waits: 0, 120 × 2 = 240, 60 × 2 = 120; process 1,5 + 1,8 + 0,5 = 3,8.
    assert_eq!(l.wait, Some(360.0));
    assert!((l.process - 3.8).abs() < 1e-12);
    assert!((l.lead_time.unwrap() - 363.8).abs() < 1e-9);
    assert!((l.value_added - 3.3).abs() < 1e-12);
    assert!((l.pce.unwrap() - 3.3 / 363.8).abs() < 1e-12);
    // Solda: 1,8 / 0,8 = 2,25 > takt.
    assert_eq!(l.over_takt, vec!["Solda".to_string()]);
    assert_eq!(l.slowest.as_deref(), Some("Solda"));
    assert!((l.inventory_days.unwrap() - 180.0 / 240.0).abs() < 1e-12);
    // Without demand the waits are unknown.
    let none = ladder(&procs, &Header::default());
    assert_eq!((none.takt, none.lead_time), (None, None));
}

#[test]
fn rows_from_the_model_table_and_the_future_copy() {
    let csv = "processo,ciclo,setup,disponibilidade,operadores,estoque_anterior,agrega_valor\n\
               corte,1.5,0,1,1,,true\n\"solda, robô\",2,0.5,0.9,0,3.456,true\ninspecao,0.4,0,1,1,1,false\n";
    let procs = from_diagram_csv(csv);
    assert_eq!(procs.len(), 3);
    assert_eq!(procs[1].name, "solda, robô");
    assert_eq!(procs[1].inventory, Some(3.456));
    assert!(!procs[2].value_added);
    let mut t = T::new();
    let p = t.program;
    let del = map(&mut t);
    let mut n = 90_000;
    let e = model_rows_edits(&t.s, p, del, &procs, || {
        n += 1;
        Id::v7(n, [3; 10])
    });
    t.commit(e);
    let rows = processes(&t.s, del);
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["corte", "solda, robô", "inspecao"]
    );
    assert_eq!(rows[1].uptime, Some(0.9));
    assert_eq!(rows[1].inventory, Some(3.46));
    assert!(rows.iter().all(|r| r.from_model));
    // The future state starts as a copy and then changes on its own.
    t.commit(header_edits(
        del,
        Some(100.0),
        Some(400.0),
        Some("min"),
        None,
    ));
    let fut = t.id();
    t.commit(Deliverable::create(p, None, Tool::Vsm, fut, "VSM futuro"));
    let mut m = 95_000;
    let e = copy_edits(&t.s, p, del, fut, || {
        m += 1;
        Id::v7(m, [4; 10])
    });
    t.commit(e);
    let h = header(&t.s, fut);
    assert!(h.future && h.from == Some(del) && h.demand == Some(100.0));
    let copy = processes(&t.s, fut);
    assert_eq!(copy.len(), 3);
    assert_eq!(copy[1].name, "solda, robô");
    t.set(
        crate::op::Kind::ToolItem,
        copy[1].id,
        fields::INVENTORY,
        0.0,
    );
    assert_eq!(processes(&t.s, del)[1].inventory, Some(3.46));
    let before = ladder(&processes(&t.s, del), &header(&t.s, del));
    let after = ladder(&processes(&t.s, fut), &h);
    assert!(after.lead_time.unwrap() < before.lead_time.unwrap());
    let _ = item_ref(copy[0].id);
}
