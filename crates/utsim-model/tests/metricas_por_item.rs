//! Optional per-item metrics (visits, stays, work and touch time): known answers on
//! small models, conservation of items, and the report unchanged by turning them on.

use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).unwrap();
    m.validate().unwrap();
    m
}

/// One deterministic step worked by a person standing at the station.
const ONE_STEP: &str = r#"Model(name: "um", run: (stop_time: 1000.0), objects: [
    (name: "chegada", kind: Source(interarrival: Const(10.0), first_at_zero: true)),
    (name: "posto", kind: Processor(process: Const(4.0),
        operators: Some(OperatorUse(team: "ana"))), position: (1.0, 0.0, 0.0)),
    (name: "fim", kind: Sink),
    (name: "ana", kind: Transporter(speed: 1.0), position: (1.0, 0.0, 0.0)),
], connections: [("chegada", "posto"), ("posto", "fim")])"#;

#[test]
fn one_step_lead_time_is_process_time() {
    let m = model(ONE_STEP);
    let (_, items) = m.run_item_log(0).unwrap();
    let done: Vec<_> = items.iter().filter(|r| r.ended_at.is_some()).collect();
    assert!(done.len() >= 90, "{}", done.len());
    for r in done {
        let lead = r.ended_at.unwrap() - r.created_at;
        assert!((lead - 4.0).abs() < 1e-9, "{lead}");
        assert!((r.total_work() - 4.0).abs() < 1e-9);
        assert!((r.total_touch() - 4.0).abs() < 1e-9);
        assert_eq!(r.visits[&1], 1);
        assert_eq!(r.loops(), 0);
        assert!((r.stay[&1] - 4.0).abs() < 1e-9);
        assert_eq!(r.block, Some(2));
    }
}

#[test]
fn work_without_people_is_not_touch_time() {
    let m = model(&ONE_STEP.replace(
        "operators: Some(OperatorUse(team: \"ana\"))",
        "operators: Some(OperatorUse(team: \"ana\", process: false))",
    ));
    let (_, items) = m.run_item_log(0).unwrap();
    let r = items.iter().find(|r| r.ended_at.is_some()).unwrap();
    assert!((r.total_work() - 4.0).abs() < 1e-9);
    assert_eq!(r.total_touch(), 0.0);
}

#[test]
fn rework_loops_follow_the_geometric_mean() {
    // A check sends 30 % of the cases back: mean loops = 1 / (1 - p) - 1.
    let m = model(
        r#"Model(name: "volta", run: (stop_time: 200000.0, seed: 3), objects: [
            (name: "chegada", kind: Source(interarrival: Exp(mean: 10.0))),
            (name: "caixa", kind: Queue()),
            (name: "posto", kind: Processor(process: Exp(mean: 2.0))),
            (name: "aprovacao", kind: Processor(process: Exp(mean: 2.0)),
                output: Weighted([0.7, 0.3])),
            (name: "fim", kind: Sink),
        ], connections: [("chegada", "caixa"), ("caixa", "posto"), ("posto", "aprovacao"),
                         ("aprovacao", "fim"), ("aprovacao", "caixa")])"#,
    );
    let (report, items) = m.run_item_log(0).unwrap();
    let done: Vec<_> = items.iter().filter(|r| r.ended_at.is_some()).collect();
    let mean = done.iter().map(|r| r.loops() as f64).sum::<f64>() / done.len() as f64;
    let want = 1.0 / 0.7 - 1.0;
    assert!((mean - want).abs() < 0.03, "{mean} vs {want}");
    for r in &done {
        assert_eq!(r.visits[&2], r.visits[&3]);
        assert_eq!(r.loops() + 1, r.visits[&3]);
    }
    // Conservation: created = finished + still inside.
    let created = report.blocks[0].output as usize;
    let open = items.iter().filter(|r| r.ended_at.is_none()).count();
    assert_eq!(items.len(), done.len() + open);
    let inside: usize = report.blocks.iter().map(|b| b.content_now).sum();
    assert_eq!(open, inside);
    // The report counts from t = 0 here (no warm-up).
    assert_eq!(created, items.len() - report.blocks[0].content_now);
}

#[test]
fn report_is_the_same_with_the_log() {
    for path in ["mm1.ron", "celula_com_operadores.ron", "pessoas_fadiga.ron"] {
        let m = Model::load(format!(
            "{}/../../examples/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let plain = m.drive(&mut m.build(0)).unwrap();
        let (logged, items) = m.run_item_log(0).unwrap();
        assert!(!items.is_empty());
        assert_eq!(plain.metrics(), logged.metrics(), "{path}");
        assert_eq!(plain.events, logged.events, "{path}");
    }
}
