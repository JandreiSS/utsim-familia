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
    // Up 10 % at Tobler's 0,70 of the speed, down 10 % at full speed (`ramps.rs`).
    let up = utsim_objects::ramps::walk_factor(0.1);
    let want = 101f64.sqrt() / up + 101f64.sqrt();
    assert!((ramp - want).abs() < 1e-6, "{ramp} {want}");
}

/// Uma rampa de empilhadeira: o veículo reduz subindo e descendo.
#[test]
fn a_forklift_slows_down_on_a_ramp() {
    let text = |extra: &str| {
        format!(
            r#"Model(
            format: 2,
            name: "rampa_empilhadeira",
            run: (stop_time: 1000.0, movement: Aisles),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (0.0, 5.0, 0.0)),
                (name: "q", kind: Queue(), transport: "emp", position: (0.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "out", kind: Sink, position: (20.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "emp", kind: Transporter(speed: 1.0), position: (0.0, -3.0, 0.0),
                 visual: (asset: "empilhadeira")),
            ],
            connections: [("src", "q"), ("q", "out")],
            aisles: [
                (name: "via", kind: Forklift, points: [(0.0, -3.0), (10.0, -3.0), (20.0, -3.0)], width: 3.6{extra}),
            ],
        )"#
        )
    };
    let flat = run(&model(&text("")));
    let ramp = run(&model(&text(", heights: [0.0, 1.0, 0.0]")));
    let f = utsim_objects::ramps::vehicle_factor(0.1);
    assert!((flat - 20.0).abs() < 1e-6, "{flat}");
    assert!((ramp - 2.0 * 101f64.sqrt() / f).abs() < 1e-6, "{ramp}");
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

/// Operador leva a peça de `q` (0, −4) a `out` (0, 4), em movimento livre, com uma esteira
/// desenhada atravessando o caminho em y = 0, de x = −10 a 10, na altura `z`.
fn under(z: f64) -> f64 {
    let m = model(&format!(
        r#"Model(
            format: 2,
            name: "por_baixo",
            run: (stop_time: 1000.0, movement: Free),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (3.0, -6.0, 0.0)),
                (name: "q", kind: Queue(), transport: "op", position: (3.0, -4.0, 0.0), access: (-3.0, 0.0)),
                (name: "out", kind: Sink, position: (3.0, 4.0, 0.0), access: (-3.0, 0.0)),
                (name: "op", kind: Transporter(speed: 1.0, clearance: 0.5), position: (0.0, -4.0, 0.0),
                 visual: (asset: "operador")),
                (name: "alta", kind: Conveyor(length: 20.0, speed: 1.0, item_length: 1.0,
                    route: Some((nodes: [(at: (-10.0, 0.0, {z})), (at: (0.0, 0.0, {z})), (at: (10.0, 0.0, {z}))]))),
                 position: (0.0, 0.0, 0.0)),
                (name: "fim", kind: Sink, position: (12.0, 0.0, 0.0)),
            ],
            connections: [("src", "q"), ("q", "out"), ("alta", "fim")],
        )"#
    ));
    run(&m)
}

#[test]
fn people_walk_under_a_high_belt_and_around_a_low_one() {
    let low = under(0.8);
    let high = under(3.0);
    // Under the high belt the way is about straight (it only steps around the legs of the
    // middle node); the low one is gone around, past an end 10 m away.
    assert!(high < low - 5.0, "alta {high}, baixa {low}");
}

/// Empilhadeira leva a peça de `q` a `out` pelo corredor de veículos em y = 0; um caminho
/// de pedestres o atravessa em x = 0, no piso ou por uma passarela a 3 m.
fn forklift_across(heights: &str) -> f64 {
    let m = model(&format!(
        r#"Model(
            format: 2,
            name: "passarela",
            run: (stop_time: 1000.0, movement: Aisles),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (-10.0, 5.0, 0.0)),
                (name: "q", kind: Queue(), transport: "emp", position: (-10.0, 3.0, 0.0), access: (0.0, -3.0)),
                (name: "out", kind: Sink, position: (10.0, 3.0, 0.0), access: (0.0, -3.0)),
                (name: "emp", kind: Transporter(speed: 1.0), position: (-10.0, 0.0, 0.0),
                 visual: (asset: "empilhadeira")),
            ],
            connections: [("src", "q"), ("q", "out")],
            aisles: [
                (name: "corredor_A", kind: Forklift, points: [(-12.0, 0.0), (12.0, 0.0)], width: 3.6),
                (name: "passeio", kind: Pedestrian, points: [(0.0, -10.0), (0.0, -4.0), (0.0, 4.0), (0.0, 10.0)],
                 width: 1.2{heights}),
            ],
        )"#
    ));
    assert_eq!(m.aisle_net().crossings.is_empty(), !heights.is_empty());
    run(&m)
}

#[test]
fn a_walkway_takes_the_crossing_away() {
    let floor = forklift_across("");
    let walkway = forklift_across(", heights: [0.0, 3.0, 3.0, 0.0]");
    // The mandatory stop before the pedestrian crossing (2 s by default) is gone.
    assert!(
        (floor - walkway - 2.0).abs() < 1e-6,
        "piso {floor}, passarela {walkway}"
    );
}

#[test]
fn a_stair_is_slower_than_walking() {
    let stair = walk(
        "(0.0, -3.0), (10.0, -3.0), (20.0, -3.0)",
        ", heights: [0.0, 1.0, 0.0], stair: true",
    );
    let up = utsim_objects::ramps::stair_factor(0.1);
    let down = utsim_objects::ramps::stair_factor(-0.1);
    let want = 101f64.sqrt() / up + 101f64.sqrt() / down;
    assert!((stair - want).abs() < 1e-6, "{stair} {want}");
}

#[test]
fn a_stair_is_for_pedestrians_only() {
    let text = r#"Model(
        format: 2, name: "x", run: (stop_time: 10.0), objects: [], connections: [],
        aisles: [(name: "e", kind: Forklift, points: [(0.0, 0.0), (5.0, 0.0)], width: 3.6,
                  heights: [0.0, 2.0], stair: true)],
    )"#;
    assert!(Model::parse(text).unwrap().validate().is_err());
}

/// Duas esteiras desenhadas em série: a primeira entrega na segunda, com a saída dela a
/// `top` m e a entrada da segunda a `low` m.
fn two_belts(top: f64, low: f64, gap: f64) -> Model {
    model(&format!(
        r#"Model(
            format: 2,
            name: "queda",
            run: (stop_time: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (-2.0, 0.0, 0.0)),
                (name: "a", kind: Conveyor(length: 4.0, speed: 1.0, item_length: 0.5,
                    route: Some((nodes: [(at: (0.0, 0.0, {top})), (at: (4.0, 0.0, {top}))]))),
                 position: (0.0, 0.0, 0.0)),
                (name: "b", kind: Conveyor(length: 4.0, speed: 1.0, item_length: 0.5,
                    route: Some((nodes: [(at: ({x}, 0.0, {low})), (at: ({x2}, 0.0, {low}))]))),
                 position: (0.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (12.0, 0.0, 0.0)),
            ],
            connections: [("src", "a"), ("a", "b"), ("b", "out")],
        )"#,
        x = 4.0 + gap,
        x2 = 8.0 + gap
    ))
}

#[test]
fn an_item_falling_between_belts_takes_the_time_of_the_fall() {
    let level = run(&two_belts(1.0, 1.0, 0.0));
    let drop = run(&two_belts(1.0, 0.6, 0.0));
    assert!((level - 8.0).abs() < 1e-6, "{level}");
    let fall = utsim_model::trajeto::fall_time(0.4);
    assert!((fall - (0.8f64 / 9.81).sqrt()).abs() < 1e-12);
    assert!((drop - (8.0 + fall)).abs() < 1e-6, "{drop}");
}

#[test]
fn a_high_drop_a_gap_and_a_climb_are_warned() {
    let w = two_belts(1.5, 0.6, 0.0).warnings();
    assert!(w.iter().any(|m| m.contains("cai 0.90 m")), "{w:?}");
    let w = two_belts(1.0, 1.0, 0.8).warnings();
    assert!(w.iter().any(|m| m.contains("cai fora")), "{w:?}");
    let w = two_belts(0.6, 1.0, 0.0).warnings();
    assert!(w.iter().any(|m| m.contains("não sobe")), "{w:?}");
}
