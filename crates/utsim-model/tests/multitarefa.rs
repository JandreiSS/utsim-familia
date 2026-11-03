//! Multitasking of a person (`docs/plano_linhas_especializadas.md`, section 2.2): a
//! timeline worked out by hand, `max_open: 1` equal to no multitasking, the touch time
//! kept, conservation of the person's time and the team's dispatcher offering cases to a
//! busy member.

use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).unwrap();
    m.validate().unwrap();
    m
}

/// Two cases at `t = 0`, one per station, both at the person's desk; 10 min of work each.
fn two_cases(multitask: &str) -> String {
    format!(
        r#"Model(name: "dois", run: (stop_time: 100.0), objects: [
    (name: "pedido_a", kind: Source(interarrival: Const(1000.0), first_at_zero: true)),
    (name: "pedido_b", kind: Source(interarrival: Const(1000.0), first_at_zero: true)),
    (name: "a", kind: Processor(process: Const(10.0),
        operators: Some(OperatorUse(team: "ana"))), position: (1.0, 0.0, 0.0)),
    (name: "b", kind: Processor(process: Const(10.0),
        operators: Some(OperatorUse(team: "ana"))), position: (1.0, 0.0, 0.0)),
    (name: "fim_a", kind: Sink),
    (name: "fim_b", kind: Sink),
    (name: "ana", kind: Transporter(speed: 1.0{multitask}), position: (1.0, 0.0, 0.0)),
], connections: [("pedido_a", "a"), ("pedido_b", "b"), ("a", "fim_a"), ("b", "fim_b")])"#
    )
}

const MT: &str = ", multitask: Some((max_open: 2, slice: Const(2.0), switch: Const(1.0)))";

/// When each case left, by the sink it reached (fim_a = block 4, fim_b = block 5).
fn ends(m: &Model) -> (f64, f64) {
    let (_, items) = m.run_item_log(0).unwrap();
    let end = |b: usize| {
        items
            .iter()
            .find(|r| r.block == Some(b))
            .and_then(|r| r.ended_at)
            .expect("case finished")
    };
    (end(4), end(5))
}

#[test]
fn slices_and_switches_by_hand() {
    // a: 0-2, b: 3-5, a: 6-8, ... (1 lost per turn). a gets its 10 min of work at 26;
    // b has 8 by then and, after one more turn, ends at 29.
    let m = model(&two_cases(MT));
    let (a, b) = ends(&m);
    assert!((a - 26.0).abs() < 1e-9, "a = {a}");
    assert!((b - 29.0).abs() < 1e-9, "b = {b}");
    // Without multitasking: one after the other.
    let (a, b) = ends(&model(&two_cases("")));
    assert!(
        (a - 10.0).abs() < 1e-9 && (b - 20.0).abs() < 1e-9,
        "{a} {b}"
    );
}

#[test]
fn touch_time_and_switch_metrics() {
    let m = model(&two_cases(MT));
    let (report, items) = m.run_item_log(0).unwrap();
    for r in items.iter().filter(|r| r.ended_at.is_some()) {
        assert!((r.total_touch() - 10.0).abs() < 1e-9, "{}", r.total_touch());
    }
    let ana = report.blocks.iter().find(|b| b.name == "ana").unwrap();
    // Turns at 2, 5, 8, ..., 23 (8) and after a ends at 26 (1).
    assert_eq!(ana.custom["multitask.switches"], 9.0);
    assert!((ana.custom["multitask.switching"] - 9.0 / 100.0).abs() < 1e-9);
    // Conservation: working + turning + idle = the whole window.
    let s = |k: &str| ana.states.get(k).copied().unwrap_or(0.0);
    assert!(
        (s("utilize") - 20.0 / 100.0).abs() < 1e-9,
        "{:?}",
        ana.states
    );
    assert!((s("setup") - 9.0 / 100.0).abs() < 1e-9, "{:?}", ana.states);
    assert!((s("utilize") + s("setup") + s("idle") - 1.0).abs() < 1e-9);
    // The station waits for its operator while she is on the other case.
    let a = report.blocks.iter().find(|b| b.name == "a").unwrap();
    assert!(
        (a.states["processing"] - 10.0 / 100.0).abs() < 1e-9,
        "{:?}",
        a.states
    );
    assert!((a.custom["servers.busy"] - 10.0 / 100.0).abs() < 1e-9);
}

#[test]
fn one_open_case_is_the_same_as_none() {
    let one = ", multitask: Some((max_open: 1, slice: Const(2.0), switch: Const(1.0)))";
    for (with, without) in [
        (two_cases(one), two_cases("")),
        (team_model(one), team_model("")),
    ] {
        let a = serde_json::to_string(&model(&with).run_replications().unwrap()).unwrap();
        let b = serde_json::to_string(&model(&without).run_replications().unwrap()).unwrap();
        assert_eq!(a, b);
    }
}

/// A team of two people, random arrivals and work, cases routed by the dispatcher.
fn team_model(multitask: &str) -> String {
    format!(
        r#"Model(name: "equipe", run: (stop_time: 20000.0, seed: 7, replications: 2), objects: [
    (name: "pedidos", kind: Source(interarrival: Exp(mean: 6.0))),
    (name: "caixa", kind: Queue()),
    (name: "analise", kind: Processor(capacity: 10, process: Exp(mean: 4.0),
        operators: Some(OperatorUse(team: "equipe"))), position: (2.0, 0.0, 0.0)),
    (name: "emissao", kind: Processor(capacity: 10, process: Exp(mean: 3.0),
        operators: Some(OperatorUse(team: "equipe"))), position: (4.0, 0.0, 0.0)),
    (name: "fim", kind: Sink),
    (name: "equipe", kind: Dispatcher(team: ["ana", "rui"])),
    (name: "ana", kind: Transporter(speed: 60.0{multitask}), position: (3.0, 0.0, 0.0)),
    (name: "rui", kind: Transporter(speed: 60.0{multitask}), position: (3.0, 1.0, 0.0)),
], connections: [("pedidos", "caixa"), ("caixa", "analise"), ("analise", "emissao"),
                 ("emissao", "fim")])"#
    )
}

#[test]
fn team_members_hold_several_cases_and_time_is_conserved() {
    let mt = ", multitask: Some((max_open: 3, slice: Exp(mean: 2.0), switch: Const(0.5)))";
    let m = model(&team_model(mt));
    let (report, items) = m.run_item_log(0).unwrap();
    let done: Vec<_> = items.iter().filter(|r| r.ended_at.is_some()).collect();
    assert!(done.len() > 2000, "{}", done.len());
    for name in ["ana", "rui"] {
        let p = report.blocks.iter().find(|b| b.name == name).unwrap();
        let total: f64 = p.states.values().sum();
        assert!((total - 1.0).abs() < 1e-9, "{name}: {:?}", p.states);
        let open = p.custom["multitask.open"];
        assert!(open > 1.0 && open <= 3.0, "{name}: {open}");
        assert!(p.custom["multitask.switches"] > 100.0);
    }
    // Touch time is the work drawn, whatever the interleaving: equal to the work.
    for r in &done {
        assert!((r.total_touch() - r.total_work()).abs() < 1e-9);
    }
    // Conservation of cases: created = finished + still inside.
    let inside: usize = report.blocks.iter().map(|b| b.content_now).sum();
    assert_eq!(items.len(), done.len() + inside);
    // Multitasking lengthens the lead time of the same arrivals and work.
    let lead = |m: &Model| {
        let (_, items) = m.run_item_log(0).unwrap();
        let done: Vec<_> = items
            .iter()
            .filter_map(|r| Some(r.ended_at? - r.created_at))
            .collect();
        done.iter().sum::<f64>() / done.len() as f64
    };
    assert!(lead(&m) > lead(&model(&team_model(""))));
}

#[test]
fn invalid_multitask_is_reported() {
    for (bad, want) in [
        ("max_open: 0, slice: Const(2.0)", "max_open"),
        ("max_open: 2, slice: Const(0.0)", "slice"),
    ] {
        let text = two_cases(&format!(", multitask: Some(({bad}))"));
        let err = Model::parse(&text)
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string();
        assert!(err.contains(want), "{err}");
    }
}
