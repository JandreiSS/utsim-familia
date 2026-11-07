//! Abandonment in a queue (`patience`, `docs/plano_linhas_especializadas.md`, 7.2): known
//! answers by hand, the item log of who gave up, and nothing changed without patience.

use utsim_model::Model;

/// One server of 10 minutes; customers at t = 0, 1 and 2; queue with `patience`.
fn three_customers(patience: &str) -> String {
    format!(
        r#"Model(name: "abandono", run: (stop_time: 100.0), objects: [
    (name: "clientes", kind: Source(interarrival: Const(1.0), first_at_zero: true, max_arrivals: 3)),
    (name: "fila", kind: Queue({patience})),
    (name: "guiche", kind: Processor(process: Const(10.0))),
    (name: "saida", kind: Sink),
], connections: [("clientes", "fila"), ("fila", "guiche"), ("guiche", "saida")])"#
    )
}

fn model(text: &str) -> Model {
    let m = Model::parse(text).unwrap();
    m.validate().unwrap();
    m
}

#[test]
fn customers_give_up_when_their_patience_ends() {
    // Patience 5: the first is served 0-10; the others give up at 6 and 7.
    let m = model(&three_customers("patience: Some(Const(5.0))"));
    let (report, items) = m.run_item_log(0).unwrap();
    let fila = report.block("fila").unwrap();
    assert_eq!(fila.custom["abandoned.count"], 2.0);
    let wait = &fila.extra["abandon_wait"];
    assert_eq!(wait.count, 2);
    assert!((wait.mean - 5.0).abs() < 1e-9);
    assert_eq!(report.block("saida").unwrap().input, 1);
    let mut gave_up: Vec<f64> = items
        .iter()
        .filter(|r| r.labels.contains_key("abandonou"))
        .map(|r| r.ended_at.unwrap())
        .collect();
    gave_up.sort_by(f64::total_cmp);
    assert_eq!(gave_up, vec![6.0, 7.0]);
    for r in items.iter().filter(|r| r.labels.contains_key("abandonou")) {
        assert_eq!(r.block, Some(1), "ends in the queue");
    }

    // Patience 15: the second is served 10-20; the third waits from 2 and gives up at 17.
    let m = model(&three_customers("patience: Some(Const(15.0))"));
    let (report, items) = m.run_item_log(0).unwrap();
    assert_eq!(report.block("fila").unwrap().custom["abandoned.count"], 1.0);
    assert_eq!(report.block("saida").unwrap().input, 2);
    let r = items
        .iter()
        .find(|r| r.labels.contains_key("abandonou"))
        .unwrap();
    assert_eq!(r.ended_at, Some(17.0));
}

#[test]
fn without_patience_nobody_gives_up_and_the_report_is_the_same() {
    let plain = model(&three_customers(""));
    let report = &plain.run_replications().unwrap()[0];
    let fila = report.block("fila").unwrap();
    assert!(!fila.custom.contains_key("abandoned.count"));
    assert!(!fila.extra.contains_key("abandon_wait"));
    assert_eq!(report.block("saida").unwrap().input, 3);
    // A patience longer than any wait changes nothing but the new count.
    let long = model(&three_customers("patience: Some(Const(1000.0))"));
    let r2 = &long.run_replications().unwrap()[0];
    assert_eq!(r2.block("fila").unwrap().custom["abandoned.count"], 0.0);
    assert_eq!(
        r2.block("saida").unwrap().staytime.mean,
        report.block("saida").unwrap().staytime.mean
    );
}

#[test]
fn random_patience_conserves_customers_and_repeats_by_seed() {
    let text = r#"Model(name: "agencia", run: (stop_time: 5000.0, seed: 9), objects: [
    (name: "clientes", kind: Source(interarrival: Exp(mean: 2.0))),
    (name: "fila", kind: Queue(patience: Some(Exp(mean: 6.0)))),
    (name: "guiche", kind: Processor(capacity: 2, process: Exp(mean: 4.5))),
    (name: "saida", kind: Sink),
], connections: [("clientes", "fila"), ("fila", "guiche"), ("guiche", "saida")])"#;
    let m = model(text);
    let (report, items) = m.run_item_log(0).unwrap();
    let created = report.block("clientes").unwrap().output as usize;
    let served = report.block("saida").unwrap().input as usize;
    let abandoned = report.block("fila").unwrap().custom["abandoned.count"] as usize;
    let inside: usize = report.blocks.iter().map(|b| b.content_now).sum();
    assert_eq!(created, served + abandoned + inside);
    assert!(abandoned > 50, "{abandoned}");
    assert_eq!(items.len(), created);
    let a = serde_json::to_string(&m.run_replications().unwrap()).unwrap();
    let b = serde_json::to_string(&m.run_replications().unwrap()).unwrap();
    assert_eq!(a, b);
}
