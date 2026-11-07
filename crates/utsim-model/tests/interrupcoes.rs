//! Preemption of a member working at a station (`Preempt::Full`, E4 of
//! `docs/plano_chegadas_e_equipes_3d.md`): timelines worked out by hand, the touch time
//! kept, determinism by seed and conservation on random teams.

use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).unwrap();
    m.validate().unwrap();
    m
}

/// One person at the desks of two stations: a 10-minute job at t = 0 and an urgent
/// 3-minute one (priority 10) at t = 4.
fn interruption(preempt: &str) -> String {
    format!(
        r#"Model(name: "interrupcao", run: (stop_time: 100.0), objects: [
    (name: "caso", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1)),
    (name: "telefone", kind: Source(interarrival: Const(4.0), max_arrivals: 1)),
    (name: "posto", kind: Processor(process: Const(10.0),
        operators: Some(OperatorUse(team: "equipe"))), position: (1.0, 0.0, 0.0)),
    (name: "atender", kind: Processor(process: Const(3.0),
        operators: Some(OperatorUse(team: "equipe", priority: Some(10)))), position: (1.0, 0.0, 0.0)),
    (name: "fim_caso", kind: Sink),
    (name: "fim_ligacao", kind: Sink),
    (name: "equipe", kind: Dispatcher(team: ["ana"]{preempt})),
    (name: "ana", kind: Transporter(speed: 1.0), position: (1.0, 0.0, 0.0)),
], connections: [("caso", "posto"), ("telefone", "atender"), ("posto", "fim_caso"),
                 ("atender", "fim_ligacao")])"#
    )
}

/// When the item that reached sink `b` (block index) left the model.
fn end(items: &[utsim_core::ItemRecord], b: usize) -> f64 {
    items
        .iter()
        .find(|r| r.block == Some(b))
        .and_then(|r| r.ended_at)
        .expect("finished")
}

#[test]
fn an_urgent_job_interrupts_the_work_and_the_rest_waits_with_its_time_left() {
    // Full: the case is worked 0-4, the call 4-7, the case again 7-13 (6 minutes left).
    let m = model(&interruption(", preempt: Full"));
    let (report, items) = m.run_item_log(0).unwrap();
    assert!(
        (end(&items, 5) - 7.0).abs() < 1e-9,
        "ligação {}",
        end(&items, 5)
    );
    assert!(
        (end(&items, 4) - 13.0).abs() < 1e-9,
        "caso {}",
        end(&items, 4)
    );
    // The work drawn at the start is still the touch time.
    let caso = items.iter().find(|r| r.block == Some(4)).unwrap();
    assert!((caso.total_touch() - 10.0).abs() < 1e-9);
    assert_eq!(
        report.block("equipe").unwrap().custom["preemptions.count"],
        1.0
    );
    // The station waited for its operator while she took the call.
    let posto = report.block("posto").unwrap();
    assert!(
        (posto.states["processing"] - 10.0 / 100.0).abs() < 1e-9,
        "{:?}",
        posto.states
    );
    assert!(
        (posto.states["waiting_operator"] - 3.0 / 100.0).abs() < 1e-9,
        "{:?}",
        posto.states
    );
    // Without Full the person finishes the case first: 0-10, then the call 10-13.
    for p in ["", ", preempt: Empty"] {
        let (report, items) = model(&interruption(p)).run_item_log(0).unwrap();
        assert!((end(&items, 4) - 10.0).abs() < 1e-9, "{p}");
        assert!((end(&items, 5) - 13.0).abs() < 1e-9, "{p}");
        let preemptions = report
            .block("equipe")
            .unwrap()
            .custom
            .get("preemptions.count");
        assert!(preemptions.is_none_or(|v| *v == 0.0), "{p}");
    }
}

/// The same with a multitasking person holding two 10-minute cases (slices of 2, 1 lost
/// per turn) when the call comes.
fn multitasking(preempt: &str) -> String {
    interruption(preempt)
        .replace(
            r#"(name: "caso", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1)),"#,
            r#"(name: "caso", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1)),
    (name: "caso_b", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1)),
    (name: "posto_b", kind: Processor(process: Const(10.0),
        operators: Some(OperatorUse(team: "equipe"))), position: (1.0, 0.0, 0.0)),
    (name: "fim_b", kind: Sink),"#,
        )
        .replace(
            r#"kind: Transporter(speed: 1.0)"#,
            r#"kind: Transporter(speed: 1.0, multitask: Some((max_open: 2, slice: Const(2.0), switch: Const(1.0))))"#,
        )
        .replace(
            r#"("posto", "fim_caso"),"#,
            r#"("posto", "fim_caso"), ("caso_b", "posto_b"), ("posto_b", "fim_b"),"#,
        )
}

#[test]
fn an_interruption_takes_every_open_case_back_and_is_done_alone() {
    // a 0-2, turn, b 3-4 when the call comes at 4: a has 8 left, b 9. Both go back to the
    // front of the queue, b (the one in hand) first and then a ahead of it. The call alone
    // 4-7; then a 7-9, b 10-12, a 13-15, b 16-18, a 19-21, b 22-24, a 25-27 (done), one
    // more turn and b 28-31.
    let m = model(&multitasking(", preempt: Full"));
    let (report, items) = m.run_item_log(0).unwrap();
    let names: Vec<&str> = m.objects.iter().map(|o| o.name.as_str()).collect();
    let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
    assert!((end(&items, at("fim_ligacao")) - 7.0).abs() < 1e-9);
    assert!(
        (end(&items, at("fim_caso")) - 27.0).abs() < 1e-9,
        "{}",
        end(&items, at("fim_caso"))
    );
    assert!(
        (end(&items, at("fim_b")) - 31.0).abs() < 1e-9,
        "{}",
        end(&items, at("fim_b"))
    );
    for r in items.iter().filter(|r| r.ended_at.is_some()) {
        assert!((r.total_touch() - r.total_work()).abs() < 1e-9);
    }
    let ana = report.block("ana").unwrap();
    let total: f64 = ana.states.values().sum();
    assert!((total - 1.0).abs() < 1e-9, "{:?}", ana.states);
}

/// Two people, ordinary cases and urgent ones with random arrivals and work.
fn random_team(preempt: &str, seed: u64) -> String {
    format!(
        r#"Model(name: "equipe", run: (stop_time: 20000.0, seed: {seed}), objects: [
    (name: "casos", kind: Source(interarrival: Exp(mean: 8.0))),
    (name: "urgentes", kind: Source(interarrival: Exp(mean: 40.0))),
    (name: "posto", kind: Processor(capacity: 6, process: Exp(mean: 6.0),
        operators: Some(OperatorUse(team: "equipe"))), position: (2.0, 0.0, 0.0)),
    (name: "urgencia", kind: Processor(capacity: 2, process: Exp(mean: 4.0),
        operators: Some(OperatorUse(team: "equipe", priority: Some(10)))), position: (4.0, 0.0, 0.0)),
    (name: "fim", kind: Sink),
    (name: "fim_urgente", kind: Sink),
    (name: "equipe", kind: Dispatcher(team: ["ana", "rui"]{preempt}, metrics: true)),
    (name: "ana", kind: Transporter(speed: 60.0, multitask: Some((max_open: 2, slice: Exp(mean: 3.0), switch: Const(0.5)))), position: (3.0, 1.0, 0.0)),
    (name: "rui", kind: Transporter(speed: 60.0), position: (3.0, -1.0, 0.0)),
], connections: [("casos", "posto"), ("urgentes", "urgencia"), ("posto", "fim"),
                 ("urgencia", "fim_urgente")])"#
    )
}

#[test]
fn random_teams_conserve_cases_and_time_and_repeat_by_seed() {
    for seed in [1, 2, 3] {
        let m = model(&random_team(", preempt: Full", seed));
        let (report, items) = m.run_item_log(0).unwrap();
        let done: Vec<_> = items.iter().filter(|r| r.ended_at.is_some()).collect();
        assert!(done.len() > 2000, "{}", done.len());
        let inside: usize = report.blocks.iter().map(|b| b.content_now).sum();
        assert_eq!(items.len(), done.len() + inside);
        for r in &done {
            assert!((r.total_touch() - r.total_work()).abs() < 1e-9);
        }
        for name in ["ana", "rui"] {
            let total: f64 = report.block(name).unwrap().states.values().sum();
            assert!((total - 1.0).abs() < 1e-9, "{name}");
        }
        assert!(report.block("equipe").unwrap().custom["preemptions.count"] > 10.0);
        // Same seed, same run.
        let again = serde_json::to_string(&m.run_replications().unwrap()).unwrap();
        let first = serde_json::to_string(&m.run_replications().unwrap()).unwrap();
        assert_eq!(again, first);
    }
    // The urgent cases wait less with the interruption.
    let wait = |p: &str| {
        let r = &model(&random_team(p, 1)).run_replications().unwrap()[0];
        r.block("fim_urgente").unwrap().extra["time_in_system"].mean
    };
    assert!(wait(", preempt: Full") < wait(""));
}
