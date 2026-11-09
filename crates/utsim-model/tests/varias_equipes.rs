//! A processor that needs people of several teams at the same time (`also`, the line Saúde
//! e hospital: a doctor and a nurse in the same room). Hand-computed timings, as in
//! `operators.rs`: speed 1 and straight lines, so time = distance; one item at t = 0; the
//! run lasts 100, so a state that lasts `d` shows as the fraction `d / 100`.

use utsim_core::RunReport;
use utsim_model::Model;

fn run(text: &str) -> RunReport {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m.run_replications().unwrap().remove(0)
}

fn near(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: obtido {got}, esperado {want}"
    );
}

fn state(r: &RunReport, block: &str, state: &str) -> f64 {
    r.block(block)
        .unwrap()
        .states
        .get(state)
        .copied()
        .unwrap_or(0.0)
}

/// One room at (10, 0); the doctor starts at (0, 0), 10 away, and the nurse at (10, 20),
/// 20 away. `sala` is the kind text of the room.
fn room(sala: &str) -> String {
    format!(
        r#"Model(
            name: "sala",
            run: (stop_time: 100.0),
            objects: [
                (name: "chegada", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: Some(1))),
                (name: "sala", kind: {sala}, position: (10.0, 0.0, 0.0)),
                (name: "med", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0)),
                (name: "enf", kind: Transporter(speed: 1.0), position: (10.0, 20.0, 0.0)),
                (name: "alta", kind: Sink),
            ],
            connections: [("chegada", "sala"), ("sala", "alta")],
        )"#
    )
}

#[test]
fn the_stage_starts_when_every_team_is_there() {
    // Both asked at t = 0. The doctor arrives at 10 and the nurse at 20: the process (5)
    // runs 20..25, then both are released.
    let r = run(&room(
        r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "med")),
            also: [OperatorUse(team: "enf")])"#,
    ));
    near(
        "sala staytime",
        r.block("sala").unwrap().staytime.mean,
        25.0,
    );
    near(
        "sala waiting_operator",
        state(&r, "sala", "waiting_operator"),
        0.20,
    );
    near("sala processing", state(&r, "sala", "processing"), 0.05);
    near("med travel_empty", state(&r, "med", "travel_empty"), 0.10);
    near("enf travel_empty", state(&r, "enf", "travel_empty"), 0.20);
    assert_eq!(r.block("alta").unwrap().input, 1);
}

#[test]
fn different_teams_for_setup_and_process() {
    // The doctor only for the setup (3), the nurse only for the process (7). The doctor is
    // asked at 0 and arrives at 10: setup 10..13. Then the doctor is released and the nurse
    // asked; she arrives at 13 + 20 = 33: process 33..40.
    let r = run(&room(
        r#"Processor(setup: Some(Const(3.0)), process: Const(7.0),
            operators: Some(OperatorUse(team: "med", process: false)),
            also: [OperatorUse(team: "enf", setup: false)])"#,
    ));
    near(
        "sala staytime",
        r.block("sala").unwrap().staytime.mean,
        40.0,
    );
    near("sala setup", state(&r, "sala", "setup"), 0.03);
    near("sala processing", state(&r, "sala", "processing"), 0.07);
    near(
        "sala waiting_operator",
        state(&r, "sala", "waiting_operator"),
        0.30,
    );
    near("med utilize", state(&r, "med", "utilize"), 0.03);
    near("enf utilize", state(&r, "enf", "utilize"), 0.07);
}

#[test]
fn without_also_nothing_changes() {
    // The same room with the doctor alone: 10 to arrive, 5 of process.
    let r = run(&room(
        r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "med")))"#,
    ));
    near(
        "sala staytime",
        r.block("sala").unwrap().staytime.mean,
        15.0,
    );
    near("enf idle", state(&r, "enf", "idle"), 1.0);
}

#[test]
fn also_needs_operators_and_other_teams() {
    for (sala, why) in [
        (
            r#"Processor(process: Const(5.0), also: [OperatorUse(team: "enf")])"#,
            "sem operators",
        ),
        (
            r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "med")),
                also: [OperatorUse(team: "med")])"#,
            "a mesma equipe",
        ),
        (
            r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "med")),
                also: [OperatorUse(team: "enf", count: 0)])"#,
            "count 0",
        ),
        (
            r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "med")),
                also: [OperatorUse(team: "alta")])"#,
            "equipe que não é recurso móvel",
        ),
    ] {
        let m = Model::parse(&room(sala)).unwrap();
        assert!(m.validate().is_err(), "{why}");
    }
}

#[test]
fn the_round_trip_keeps_also() {
    let m = Model::parse(&room(
        r#"Processor(process: Const(5.0), operators: Some(OperatorUse(team: "med")),
            also: [OperatorUse(team: "enf", requires: "sutura")])"#,
    ))
    .unwrap();
    let text = ron::ser::to_string_pretty(&m, Default::default()).unwrap();
    assert!(text.contains("also"), "{text}");
    assert_eq!(Model::parse(&text).unwrap(), m);
    // Without `also` the field is not written: old files stay the same.
    let m = Model::parse(&room(r#"Processor(process: Const(5.0))"#)).unwrap();
    assert!(
        !ron::ser::to_string_pretty(&m, Default::default())
            .unwrap()
            .contains("also")
    );
}
