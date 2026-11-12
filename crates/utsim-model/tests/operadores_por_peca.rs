//! Operators per piece (`OperatorUse.count`), the crew gain (`team_speedup`) and the
//! warning for a team larger than what its processes can use at once.
//!
//! The case that started it: a dispatcher with 2 operators on a processor of capacity 1.
//! Only one ever works, the other stands by, and the queue grows.

use utsim_core::RunReport;
use utsim_model::Model;

fn model(machine: &str, team: &str, extra: &str) -> String {
    format!(
        r#"Model(
            name: "dupla",
            run: (stop_time: 100.0),
            objects: [
                (name: "s", kind: Source(interarrival: Const(10.0), first_at_zero: true), position: (0.0, 0.0, 0.0)),
                (name: "m", kind: {machine}, position: (10.0, 0.0, 0.0)),
                (name: "p1", kind: Transporter(speed: 1.0), position: (10.0, 0.0, 0.0)),
                (name: "p2", kind: Transporter(speed: 1.0), position: (10.0, 0.0, 0.0)),
                (name: "equipe", kind: Dispatcher(team: [{team}])),
                (name: "out", kind: Sink, position: (20.0, 0.0, 0.0)),
                {extra}
            ],
            connections: [("s", "m"), ("m", "out"){conn}],
        )"#,
        conn = if extra.contains("\"m2\"") {
            r#", ("s", "m2"), ("m2", "out")"#
        } else {
            ""
        },
    )
}

fn run(text: &str) -> RunReport {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m.run_replications().unwrap().remove(0)
}

fn state(r: &RunReport, block: &str, state: &str) -> f64 {
    r.block(block)
        .unwrap()
        .states
        .get(state)
        .copied()
        .unwrap_or(0.0)
}

fn near(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: obtido {got}, esperado {want}"
    );
}

fn idle_warnings(text: &str) -> Vec<String> {
    let m = Model::parse(text).unwrap();
    m.validate().unwrap();
    m.warnings()
        .into_iter()
        .filter(|w| w.contains("ao mesmo tempo"))
        .collect()
}

#[test]
fn two_operators_on_capacity_one_are_warned() {
    let one = r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "equipe")))"#;
    let w = idle_warnings(&model(one, r#""p1", "p2""#, ""));
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("'equipe' tem 2 operadores"), "{}", w[0]);
    assert!(w[0].contains("'m' usa no máximo 1"), "{}", w[0]);
    assert!(w[0].contains("1 fica parado"), "{}", w[0]);
    assert!(w[0].contains("capacidade de 'm' para 2"), "{}", w[0]);
    assert!(w[0].contains("use 2 operadores por peça"), "{}", w[0]);

    // Either fix silences it.
    let two_per_piece =
        r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "equipe", count: 2)))"#;
    assert!(idle_warnings(&model(two_per_piece, r#""p1", "p2""#, "")).is_empty());
    let capacity_two = r#"Processor(capacity: 2, process: Const(5.0), operators: Some(OperatorUse(team: "equipe")))"#;
    assert!(idle_warnings(&model(capacity_two, r#""p1", "p2""#, "")).is_empty());
    // One operator: nothing to say.
    assert!(idle_warnings(&model(one, r#""p1""#, "")).is_empty());
}

#[test]
fn a_team_with_other_work_is_not_warned() {
    // The same team also repairs another machine: the second operator has other work.
    let one = r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "equipe")))"#;
    let extra = r#"(name: "m2", kind: Processor(process: Const(1.0)), position: (30.0, 0.0, 0.0),
                    downtime: Some((mtbf: Exp(mean: 100.0), mttr: Const(5.0), repair_crew: Some("equipe")))),"#;
    assert!(idle_warnings(&model(one, r#""p1", "p2""#, extra)).is_empty());
}

#[test]
fn a_team_that_takes_turns_is_not_warned() {
    // RoundRobin: the two share the work on purpose (rest, ergonomics), one at a time.
    let one = r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "equipe")))"#;
    let text = model(one, r#""p1", "p2""#, "").replace(
        r#"Dispatcher(team: ["p1", "p2"])"#,
        r#"Dispatcher(team: ["p1", "p2"], rule: RoundRobin)"#,
    );
    assert!(text.contains("RoundRobin"));
    assert!(idle_warnings(&text).is_empty());
}

#[test]
fn a_team_shared_by_two_processes_is_counted_once() {
    // Two processes of capacity 1 share a team of 3: at most 2 work at once.
    let one = r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "equipe")))"#;
    let extra = r#"(name: "p3", kind: Transporter(speed: 1.0), position: (10.0, 0.0, 0.0)),
                   (name: "m2", kind: Processor(process: Const(5.0), operators: Some(OperatorUse(team: "p2"))), position: (30.0, 0.0, 0.0)),"#;
    let w = idle_warnings(&model(one, r#""p1", "p2", "p3""#, extra));
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("'equipe' tem 3 operadores, mas 'm', 'm2' usam no máximo 2"),
        "{}",
        w[0]
    );
}

#[test]
fn two_per_piece_keeps_both_operators_until_the_end() {
    // One piece every 10 from t = 0, process 5, both operators already at the machine.
    // Each piece takes both for 0..5, 10..15, ...: each operator works half the time.
    let m =
        r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "equipe", count: 2)))"#;
    let r = run(&model(m, r#""p1", "p2""#, ""));
    near("m processing", state(&r, "m", "processing"), 0.5);
    near("p1 utilize", state(&r, "p1", "utilize"), 0.5);
    near("p2 utilize", state(&r, "p2", "utilize"), 0.5);
    near("staytime", r.block("m").unwrap().staytime.mean, 5.0);
    assert_eq!(r.block("out").unwrap().input, 10);

    // With one per piece the same work needs only one at a time: the two add up to 0.5.
    let m = r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "equipe")))"#;
    let r = run(&model(m, r#""p1", "p2""#, ""));
    near(
        "p1 + p2 utilize",
        state(&r, "p1", "utilize") + state(&r, "p2", "utilize"),
        0.5,
    );
}

#[test]
fn team_speedup_divides_the_process_time() {
    // Process 8 with two people twice as fast: 4 per piece.
    let m = r#"Processor(process: Const(8.0),
        operators: Some(OperatorUse(team: "equipe", count: 2, team_speedup: Some(2.0))))"#;
    let r = run(&model(m, r#""p1", "p2""#, ""));
    near("staytime", r.block("m").unwrap().staytime.mean, 4.0);
    near("m processing", state(&r, "m", "processing"), 0.4);
    near("p1 utilize", state(&r, "p1", "utilize"), 0.4);
    near("p2 utilize", state(&r, "p2", "utilize"), 0.4);

    // Absent or 1: no gain.
    let m = r#"Processor(process: Const(8.0),
        operators: Some(OperatorUse(team: "equipe", count: 2)))"#;
    near(
        "sem ganho",
        run(&model(m, r#""p1", "p2""#, ""))
            .block("m")
            .unwrap()
            .staytime
            .mean,
        8.0,
    );
}

#[test]
fn team_speedup_must_be_positive() {
    for bad in ["0.0", "-1.0"] {
        let m = format!(
            r#"Processor(process: Const(8.0),
            operators: Some(OperatorUse(team: "equipe", count: 2, team_speedup: Some({bad}))))"#
        );
        let e = Model::parse(&model(&m, r#""p1", "p2""#, ""))
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string();
        assert!(e.contains("team_speedup"), "{e}");
    }
}

#[test]
fn pairs_from_a_shared_pool_never_deadlock() {
    // Three operators in one pool, two processes (one of capacity 2) that each need two
    // per piece, random arrivals and times. Pieces only ever wait for operators: every
    // piece that came in leaves, except the few still inside at the end. The operators walk
    // fast so the walk between the machines does not overload the pool.
    let text = r#"Model(
        name: "pool",
        run: (stop_time: 20000.0, seed: 7, replications: 3),
        objects: [
            (name: "s1", kind: Source(interarrival: Exp(mean: 10.0)), position: (0.0, 0.0, 0.0)),
            (name: "s2", kind: Source(interarrival: Exp(mean: 12.0)), position: (0.0, 10.0, 0.0)),
            (name: "q1", kind: Queue(), position: (2.0, 0.0, 0.0)),
            (name: "q2", kind: Queue(), position: (2.0, 10.0, 0.0)),
            (name: "a", kind: Processor(capacity: 2, process: Exp(mean: 4.0),
                operators: Some(OperatorUse(team: "pool", count: 2))), position: (5.0, 0.0, 0.0)),
            (name: "b", kind: Processor(process: Exp(mean: 3.0),
                operators: Some(OperatorUse(team: "pool", count: 2, team_speedup: Some(1.5)))), position: (5.0, 10.0, 0.0)),
            (name: "o1", kind: Transporter(speed: 100.0), position: (5.0, 5.0, 0.0)),
            (name: "o2", kind: Transporter(speed: 100.0), position: (5.0, 5.0, 0.0)),
            (name: "o3", kind: Transporter(speed: 100.0), position: (5.0, 5.0, 0.0)),
            (name: "pool", kind: Dispatcher(team: ["o1", "o2", "o3"])),
            (name: "out", kind: Sink, position: (10.0, 5.0, 0.0)),
        ],
        connections: [("s1", "q1"), ("s2", "q2"), ("q1", "a"), ("q2", "b"), ("a", "out"), ("b", "out")],
    )"#;
    let m = Model::parse(text).unwrap();
    m.validate().unwrap();
    for r in m.run_replications().unwrap() {
        let made = r.block("s1").unwrap().output + r.block("s2").unwrap().output;
        let done = r.block("out").unwrap().input;
        assert!(made > 3000, "{made}");
        assert!(made - done < 30, "presas: {} de {made}", made - done);
        // Every operator worked: the pool is shared, not taken by one process.
        for o in ["o1", "o2", "o3"] {
            assert!(
                state(&r, o, "utilize") > 0.1,
                "{o}: {}",
                state(&r, o, "utilize")
            );
        }
    }
}
