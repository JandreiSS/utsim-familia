//! Esteiras desenhadas por nós: a geometria do trajeto entra no resultado.
//!
//! Uma peça só, fonte → esteira → saída, sem transporte: o tempo no sistema é o tempo de
//! passagem na esteira, comprimento 3D do trajeto ÷ velocidade.

use utsim_core::RunReport;
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn tis(r: &RunReport) -> f64 {
    r.block("out").unwrap().extra["time_in_system"].max
}

fn run(m: &Model) -> f64 {
    tis(&m.run_replications().unwrap().remove(0))
}

/// Esteira com o trajeto `nodes` (texto RON da lista de nós) e o resto dado.
fn belt(nodes: &str, extra: &str, speed: f64, items: u32) -> Model {
    model(&format!(
        r#"Model(
            format: 2,
            name: "trajeto",
            run: (stop_time: 10000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(0.001), first_at_zero: true, max_arrivals: {items}),
                 position: (-2.0, 0.0, 0.0)),
                (name: "esteira", kind: Conveyor(length: 5.0, speed: {speed}, item_length: 1.0,
                    route: Some((nodes: [{nodes}]{extra}))),
                 position: (0.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (20.0, 0.0, 0.0)),
            ],
            connections: [("src", "esteira"), ("esteira", "out")],
        )"#
    ))
}

#[test]
fn the_transit_time_is_the_3d_length_over_the_speed() {
    let flat = belt("(at: (0.0, 0.0, 0.0)), (at: (8.0, 0.0, 0.0))", "", 1.0, 1);
    assert!((run(&flat) - 8.0).abs() < 1e-6, "{}", run(&flat));
    // Raising the last node 6 m: the belt is √(8² + 6²) = 10 m long.
    let up = belt("(at: (0.0, 0.0, 0.0)), (at: (8.0, 0.0, 6.0))", "", 1.0, 1);
    assert!((run(&up) - 10.0).abs() < 1e-6, "{}", run(&up));
}

#[test]
fn a_node_height_changes_the_result() {
    let low = belt(
        "(at: (0.0, 0.0, 1.0)), (at: (4.0, 0.0, 1.0)), (at: (8.0, 0.0, 1.0))",
        "",
        0.5,
        1,
    );
    let high = belt(
        "(at: (0.0, 0.0, 1.0)), (at: (4.0, 0.0, 2.0)), (at: (8.0, 0.0, 1.0))",
        "",
        0.5,
        1,
    );
    let (a, b) = (run(&low), run(&high));
    assert!((a - 16.0).abs() < 1e-6, "{a}");
    // Two legs of √17 m at 0,5 m/s.
    assert!((b - 4.0 * 17f64.sqrt()).abs() < 1e-6, "{b}");
}

#[test]
fn a_curve_changes_the_result() {
    let sharp = "(at: (0.0, 0.0, 0.0)), (at: (5.0, 0.0, 0.0)), (at: (5.0, 5.0, 0.0))";
    let round =
        "(at: (0.0, 0.0, 0.0)), (at: (5.0, 0.0, 0.0), radius: Some(2.0)), (at: (5.0, 5.0, 0.0))";
    let a = run(&belt(sharp, "", 1.0, 1));
    let b = run(&belt(round, "", 1.0, 1));
    assert!((a - 10.0).abs() < 1e-6, "{a}");
    let exact = 6.0 + std::f64::consts::PI;
    assert!((b - exact).abs() < exact * 2e-4, "{b}");
}

#[test]
fn the_belt_holds_what_fits_on_the_drawn_length() {
    // Ten items at once: one enters every item_length ÷ speed = 1 s and each rides the
    // 3 m route for 3 s, so at most 3 are on it, whatever `length` (5) says.
    let m = belt("(at: (0.0, 0.0, 0.0)), (at: (3.0, 0.0, 0.0))", "", 1.0, 10);
    let r = m.run_replications().unwrap().remove(0);
    assert_eq!(r.block("out").unwrap().input, 10);
    assert_eq!(r.block("esteira").unwrap().content_max, 3.0);
}

#[test]
fn a_gravity_belt_is_slower_when_it_is_flatter() {
    let steep = belt(
        "(at: (0.0, 0.0, 1.0)), (at: (10.0, 0.0, 0.0))",
        ", drive: Gravity",
        0.5,
        1,
    );
    let gentle = belt(
        "(at: (0.0, 0.0, 0.3)), (at: (10.0, 0.0, 0.0))",
        ", drive: Gravity",
        0.5,
        1,
    );
    let len = (100.0f64 + 0.09).sqrt();
    // 10 % grade: the brake speed. 3 % grade: (0,03 − 0,015) ÷ 0,035 of it.
    assert!((run(&steep) - 101f64.sqrt() / 0.5).abs() < 1e-6);
    let v = 0.5 * (0.03 - 0.015) / (0.05 - 0.015);
    assert!((run(&gentle) - len / v).abs() < 1e-6, "{}", run(&gentle));
}

#[test]
fn a_steep_belt_is_warned() {
    // 30 % ≈ 16,7°: above the 15° of a simple belt, below the 25° of a channeled one.
    let simple = belt("(at: (0.0, 0.0, 0.0)), (at: (10.0, 0.0, 3.0))", "", 1.0, 1);
    let channeled = belt(
        "(at: (0.0, 0.0, 0.0)), (at: (10.0, 0.0, 3.0))",
        ", style: Channeled",
        1.0,
        1,
    );
    assert!(simple.warnings().iter().any(|w| w.contains("inclinação")));
    assert!(
        !channeled
            .warnings()
            .iter()
            .any(|w| w.contains("inclinação"))
    );
}

#[test]
fn the_ends_are_the_first_and_last_nodes() {
    let m = belt("(at: (0.0, 0.0, 0.5)), (at: (4.0, 3.0, 1.5))", "", 1.0, 1);
    let ends = m.belt_ends();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].1, [0.0, 0.0, 0.5]);
    assert_eq!(ends[0].2, [4.0, 3.0, 1.5]);
}

#[test]
fn a_route_with_one_node_is_rejected() {
    let text = r#"Model(
        format: 2, name: "x", run: (stop_time: 10.0),
        objects: [
            (name: "esteira", kind: Conveyor(length: 5.0, speed: 1.0, item_length: 1.0,
                route: Some((nodes: [(at: (0.0, 0.0, 0.0))]))), position: (0.0, 0.0, 0.0)),
        ],
        connections: [],
    )"#;
    let m = Model::parse(text).unwrap();
    assert!(m.validate().is_err());
}

/// Fonte → esteira desenhada → operador leva até `out`, em movimento livre.
fn carried(last: &str) -> f64 {
    let m = model(&format!(
        r#"Model(
            format: 2,
            name: "ponta_desenhada",
            run: (stop_time: 1000.0, movement: Free),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (-5.0, 5.0, 0.0)),
                (name: "esteira", kind: Conveyor(length: 5.0, speed: 1.0, item_length: 1.0,
                    route: Some((nodes: [(at: (0.0, 0.0, 0.0)), (at: (10.0, 0.0, 0.0)), (at: {last})]))),
                 transport: "op", position: (0.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (30.0, 20.0, 0.0), access: (0.0, -2.0)),
                (name: "op", kind: Transporter(speed: 1.0, clearance: 0.5), position: (30.0, 18.0, 0.0)),
            ],
            connections: [("src", "esteira"), ("esteira", "out")],
        )"#
    ));
    run(&m)
}

#[test]
fn moving_the_last_node_moves_where_the_operator_picks_up() {
    // Ending nearer to `out` shortens the walk, even though the belt grows by the same.
    let a = carried("(20.0, 0.0, 0.0)");
    let b = carried("(20.0, 10.0, 0.0)");
    assert!((a - b).abs() > 1.0, "{a} {b}");
}

/// Operador leva a peça da fila em (0, 0) à saída em (20, 0) pelo caminho no piso (os
/// pontos de acesso ficam sobre o caminho, em y = −3). `extra` muda o caminho.
fn walk(points: &str, extra: &str) -> f64 {
    let m = model(&format!(
        r#"Model(
            format: 2,
            name: "caminho",
            run: (stop_time: 1000.0, movement: Aisles),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (0.0, 5.0, 0.0)),
                (name: "q", kind: Queue(), transport: "op", position: (0.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "out", kind: Sink, position: (20.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "op", kind: Transporter(speed: 1.0), position: (0.0, -3.0, 0.0),
                 visual: (asset: "operador")),
            ],
            connections: [("src", "q"), ("q", "out")],
            aisles: [
                (name: "passeio", kind: Pedestrian, points: [{points}], width: 1.2{extra}),
            ],
        )"#
    ));
    run(&m)
}

#[test]
fn a_ramp_lengthens_the_walk() {
    let flat = walk("(0.0, -3.0), (10.0, -3.0), (20.0, -3.0)", "");
    // Up 1 m at the middle point: two legs of √101 m instead of 20 m.
    let ramp = walk(
        "(0.0, -3.0), (10.0, -3.0), (20.0, -3.0)",
        ", heights: [0.0, 1.0, 0.0]",
    );
    assert!((flat - 20.0).abs() < 1e-6, "{flat}");
    assert!((ramp - 2.0 * 101f64.sqrt()).abs() < 1e-6, "{ramp}");
}

#[test]
fn a_curved_path_is_walked_along_its_curve() {
    // A detour through (10, −13): a sharp corner vs one rounded with a 4 m radius.
    let pts = "(0.0, -3.0), (10.0, -13.0), (20.0, -3.0)";
    let sharp = walk(pts, "");
    let round = walk(pts, ", radii: [0.0, 4.0, 0.0]");
    assert!((sharp - 2.0 * 200f64.sqrt()).abs() < 1e-6, "{sharp}");
    assert!(round < sharp - 0.5, "{round} {sharp}");
}
