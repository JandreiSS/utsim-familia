//! AGV traffic control: edge capacities, FIFO queues, deadlock detection.
//!
//! Layouts are tiny so every expected value is computed by hand. Each vehicle `agv_i`
//! has its own queue `q_i` (where its single pallet is created, at the vehicle's own
//! position, so there is no empty travel and loading takes no time) and its own sink
//! `out_i`. Speed is 1, so a 10-unit edge takes exactly 10 time units.

use utsim_core::RunReport;
use utsim_model::Model;

/// Two nodes 10 apart.
const AB: &str =
    r#"nodes: [(name: "A", position: (0.0, 0.0, 0.0)), (name: "B", position: (10.0, 0.0, 0.0))]"#;

struct Veh {
    /// Where the vehicle and its pallet start.
    from: &'static str,
    /// Where the pallet goes.
    to: &'static str,
    /// The pallet is created at this time.
    at: f64,
    agv: bool,
}

fn veh(from: &'static str, to: &'static str, at: f64) -> Veh {
    Veh {
        from,
        to,
        at,
        agv: true,
    }
}

fn model_text(network: &str, vehicles: &[Veh], stop: f64) -> String {
    let mut objects = String::new();
    for (i, v) in vehicles.iter().enumerate() {
        let i = i + 1;
        let first = v.at == 0.0;
        objects += &format!(
            r#"
            (name: "src_{i}", kind: Source(interarrival: Const({gap}), first_at_zero: {first}, max_arrivals: 1)),
            (name: "q_{i}", kind: Queue(), transport: "agv_{i}", position: {from}),
            (name: "out_{i}", kind: Sink, position: {to}),
            (name: "agv_{i}", kind: Transporter(speed: 1.0, agv: {agv}), position: {from}),"#,
            gap = if first { 1.0 } else { v.at },
            from = v.from,
            to = v.to,
            agv = v.agv,
        );
    }
    let mut connections = String::new();
    for i in 1..=vehicles.len() {
        connections += &format!(r#"("src_{i}", "q_{i}"), ("q_{i}", "out_{i}"),"#);
    }
    format!(
        r#"Model(
            name: "agv",
            run: (stop_time: {stop}),
            objects: [{objects}],
            connections: [{connections}],
            network: Some(({network})),
        )"#
    )
}

fn build(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn run(text: &str) -> RunReport {
    build(text).run_replications().unwrap().remove(0)
}

fn close(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-9,
        "{what}: obtido {got:.9}, esperado {want:.9}"
    );
}

/// Time in system of the pallet delivered to `out_i` (one pallet each, so min = max).
fn tis(r: &RunReport, i: usize) -> f64 {
    let t = &r.block(&format!("out_{i}")).unwrap().extra["time_in_system"];
    assert_eq!(t.min, t.max, "out_{i} should have exactly one pallet");
    t.min
}

fn state(r: &RunReport, i: usize, name: &str) -> f64 {
    // A state that never happened is absent from the report.
    let states = &r.block(&format!("agv_{i}")).unwrap().states;
    states.get(name).copied().unwrap_or(0.0)
}

/// A <-> B in one two-way edge with the given capacity.
fn corridor(capacity: &str) -> String {
    format!(r#"{AB}, edges: [(from: "A", to: "B", two_way: true, capacity: {capacity})]"#)
}

#[test]
fn single_lane_makes_the_second_vehicle_wait_exactly_until_the_first_leaves() {
    // Both pallets are created at A at t = 0 and go to B (10 units, speed 1).
    // agv_1 asks first and gets the edge: A -> B during [0, 10].
    // agv_2 is queued, blocked during [0, 10], then crosses during [10, 20].
    let r = run(&model_text(
        &corridor("Some(1)"),
        &[
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
        ],
        100.0,
    ));
    close("tempo do 1", tis(&r, 1), 10.0);
    close("tempo do 2", tis(&r, 2), 20.0);
    close("bloqueado do 1", state(&r, 1, "blocked"), 0.0);
    close("bloqueado do 2", state(&r, 2, "blocked"), 10.0 / 100.0);
    close(
        "carregado do 2",
        state(&r, 2, "travel_loaded"),
        10.0 / 100.0,
    );
}

#[test]
fn capacity_two_lets_both_vehicles_pass_together() {
    let r = run(&model_text(
        &corridor("Some(2)"),
        &[
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
        ],
        100.0,
    ));
    close("tempo do 1", tis(&r, 1), 10.0);
    close("tempo do 2", tis(&r, 2), 10.0);
    close("bloqueado do 2", state(&r, 2, "blocked"), 0.0);
}

#[test]
fn opposite_directions_share_the_capacity_of_a_two_way_edge() {
    // agv_1: A -> B, agv_2: B -> A, edge A<->B capacity 1 (shared by both directions).
    // agv_1 crosses during [0, 10]; agv_2 waits during [0, 10] and returns during
    // [10, 20].
    let r = run(&model_text(
        &corridor("Some(1)"),
        &[
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
            veh("(10.0, 0.0, 0.0)", "(0.0, 0.0, 0.0)", 0.0),
        ],
        100.0,
    ));
    close("tempo do 1", tis(&r, 1), 10.0);
    close("tempo do 2", tis(&r, 2), 20.0);
    close("bloqueado do 2", state(&r, 2, "blocked"), 10.0 / 100.0);

    // Two separate one-way edges (one per direction) are independent: nobody waits.
    let net = format!(
        r#"{AB}, edges: [(from: "A", to: "B", two_way: false, capacity: Some(1)), (from: "B", to: "A", two_way: false, capacity: Some(1))]"#
    );
    let r = run(&model_text(
        &net,
        &[
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
            veh("(10.0, 0.0, 0.0)", "(0.0, 0.0, 0.0)", 0.0),
        ],
        100.0,
    ));
    close("tempo do 1", tis(&r, 1), 10.0);
    close("tempo do 2", tis(&r, 2), 10.0);
}

#[test]
fn waiting_vehicles_are_served_in_arrival_order_not_in_model_order() {
    // agv_1 asks at t = 0 and crosses during [0, 10]. agv_3 asks at t = 1 (before
    // agv_2, which asks at t = 2) although it is listed after it. FIFO: agv_3 crosses
    // during [10, 20] and agv_2 during [20, 30].
    // Pallet times in system: 10; agv_2: 30 - 2 = 28; agv_3: 20 - 1 = 19.
    let r = run(&model_text(
        &corridor("Some(1)"),
        &[
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 2.0),
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 1.0),
        ],
        100.0,
    ));
    close("tempo do 1", tis(&r, 1), 10.0);
    close("tempo do 2", tis(&r, 2), 28.0);
    close("tempo do 3", tis(&r, 3), 19.0);
    // agv_3 waits [1, 10] = 9; agv_2 waits [2, 20] = 18.
    close("bloqueado do 3", state(&r, 3, "blocked"), 9.0 / 100.0);
    close("bloqueado do 2", state(&r, 2, "blocked"), 18.0 / 100.0);
}

#[test]
fn ordinary_transporters_ignore_edge_capacities() {
    // Same layout as the single-lane test, but the vehicles are not AGVs: they pass
    // through each other and both arrive at t = 10.
    let mut v = [
        veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
        veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
    ];
    for x in &mut v {
        x.agv = false;
    }
    let r = run(&model_text(&corridor("Some(1)"), &v, 100.0));
    close("tempo do 1", tis(&r, 1), 10.0);
    close("tempo do 2", tis(&r, 2), 10.0);
    close("bloqueado do 2", state(&r, 2, "blocked"), 0.0);
}

#[test]
fn points_off_the_network_connect_through_free_spurs() {
    // Pallet picked up at (0,-3) (nearest node A, 3 away) and delivered to (10,4)
    // (nearest node B, 4 away): 3 + 10 + 4 = 17 at speed 1. The spurs are not
    // controlled, only the A-B edge is. A second vehicle on the same job leaves
    // the edge to the first one: it starts the edge at 3, but the first holds it
    // during [3, 13]. Second AGV's pallet: waits [3, 13], crosses [13, 23], spur 4:
    // arrives at 27.
    let r = run(&model_text(
        &corridor("Some(1)"),
        &[
            veh("(0.0, -3.0, 0.0)", "(10.0, 4.0, 0.0)", 0.0),
            veh("(0.0, -3.0, 0.0)", "(10.0, 4.0, 0.0)", 0.0),
        ],
        100.0,
    ));
    close("tempo do 1", tis(&r, 1), 17.0);
    close("tempo do 2", tis(&r, 2), 27.0);
    close("bloqueado do 2", state(&r, 2, "blocked"), 10.0 / 100.0);
}

#[test]
fn without_waiting_an_agv_takes_as_long_as_an_ordinary_transporter() {
    // Accelerating profile over a two-edge route (A-B-C, 20 long): the whole-route
    // profile is used, so AGV and ordinary transporter agree: a = d = 1, v = 2:
    // 20/2 + 2 = 12.
    let net = r#"nodes: [(name: "A", position: (0.0, 0.0, 0.0)), (name: "B", position: (10.0, 0.0, 0.0)), (name: "C", position: (20.0, 0.0, 0.0))],
        edges: [(from: "A", to: "B", capacity: Some(1)), (from: "B", to: "C", capacity: Some(1))]"#;
    let times: Vec<f64> = [true, false]
        .iter()
        .map(|&agv| {
            let mut v = veh("(0.0, 0.0, 0.0)", "(20.0, 0.0, 0.0)", 0.0);
            v.agv = agv;
            let text = model_text(net, &[v], 100.0)
                .replace(
                    &format!("Transporter(speed: 1.0, agv: {agv})"),
                    &format!("Transporter(speed: 2.0, acceleration: Some(1.0), deceleration: Some(1.0), agv: {agv})"),
                );
            tis(&run(&text), 1)
        })
        .collect();
    close("agv", times[0], 12.0);
    close("comum", times[1], 12.0);
}

/// One-way ring A -> B -> C -> A, each edge with capacity 1. A = (0,0), B = (6,0),
/// C = (6,8): A->B is 6 long, B->C is 8 and C->A is 10 (a 6-8-10 triangle).
const RING: &str = r#"nodes: [(name: "A", position: (0.0, 0.0, 0.0)), (name: "B", position: (6.0, 0.0, 0.0)), (name: "C", position: (6.0, 8.0, 0.0))],
    edges: [(from: "A", to: "B", two_way: false, capacity: Some(1)),
            (from: "B", to: "C", two_way: false, capacity: Some(1)),
            (from: "C", to: "A", two_way: false, capacity: Some(1))]"#;

#[test]
fn a_ring_of_vehicles_each_wanting_the_next_edge_is_a_deadlock() {
    // agv_1 A -> C (edges A->B, B->C), agv_2 B -> A (B->C, C->A), agv_3 C -> B
    // (C->A, A->B). At t = 0 each takes its first edge (A->B, B->C, C->A).
    // t = 6: agv_1 wants B->C, held by agv_2: waits (agv_2 is still moving: no
    // deadlock yet). t = 8: agv_2 wants C->A, held by agv_3: waits, still holding
    // B->C. t = 10: agv_3 wants A->B, held by agv_1: the cycle closes.
    let text = model_text(
        RING,
        &[
            veh("(0.0, 0.0, 0.0)", "(6.0, 8.0, 0.0)", 0.0),
            veh("(6.0, 0.0, 0.0)", "(0.0, 0.0, 0.0)", 0.0),
            veh("(6.0, 8.0, 0.0)", "(6.0, 0.0, 0.0)", 0.0),
        ],
        100.0,
    );
    let err = build(&text).run_replications().unwrap_err().to_string();
    assert!(err.contains("t=10"), "{err}");
    assert!(err.contains("impasse"), "{err}");
    for part in [
        "agv_1 espera a aresta B->C (ocupada por agv_2)",
        "agv_2 espera a aresta C->A (ocupada por agv_3)",
        "agv_3 espera a aresta A->B (ocupada por agv_1)",
    ] {
        assert!(err.contains(part), "faltou '{part}' em: {err}");
    }
}

#[test]
fn a_ring_with_a_free_edge_is_not_a_deadlock() {
    // Only two vehicles on the ring: agv_1 A -> C, agv_2 B -> A. agv_1 takes A->B,
    // agv_2 takes B->C (8 long). t = 6: agv_1 waits for B->C (its holder, agv_2, is
    // moving: no deadlock). t = 8: agv_2 reaches C and asks for C->A (free): it gets
    // it and frees B->C in the same instant, which agv_1 receives. agv_1 crosses
    // B->C during [8, 16]; agv_2 crosses C->A (10) during [8, 18].
    let r = run(&model_text(
        RING,
        &[
            veh("(0.0, 0.0, 0.0)", "(6.0, 8.0, 0.0)", 0.0),
            veh("(6.0, 0.0, 0.0)", "(0.0, 0.0, 0.0)", 0.0),
        ],
        100.0,
    ));
    close("tempo do 1", tis(&r, 1), 16.0);
    close("tempo do 2", tis(&r, 2), 18.0);
    close("bloqueado do 1", state(&r, 1, "blocked"), 2.0 / 100.0);
}

#[test]
fn traffic_control_is_deterministic_and_replications_are_independent() {
    let text = model_text(
        &corridor("Some(1)"),
        &[
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
            veh("(0.0, 0.0, 0.0)", "(10.0, 0.0, 0.0)", 0.0),
            veh("(10.0, 0.0, 0.0)", "(0.0, 0.0, 0.0)", 1.0),
        ],
        100.0,
    )
    .replace(
        "run: (stop_time: 100)",
        "run: (stop_time: 100, replications: 3)",
    );
    let m = build(&text);
    let a = m.run_replications().unwrap();
    let b = m.run_replications().unwrap();
    assert_eq!(a.len(), 3);
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.metrics(), y.metrics());
        assert_eq!(x.events, y.events);
    }
    // A leftover from one replication (a held edge) must not leak into the next:
    // every replication gives the same times.
    for r in &a {
        close("tempo do 1", tis(r, 1), 10.0);
        close("tempo do 2", tis(r, 2), 20.0);
    }
}
