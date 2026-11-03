//! Corredores de circulação e modo `Aisles` (plano, seção 6): rede gerada, classes,
//! cruzamento com parada obrigatória e espera determinística, larguras e avisos.
//!
//! Velocidade 1 m/s sem aceleração: o tempo no sistema do único item de cada fluxo é o
//! comprimento da viagem carregada mais as paradas e esperas.

use utsim_core::RunReport;
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn tis(r: &RunReport, sink: &str) -> f64 {
    r.block(sink).unwrap().extra["time_in_system"].max
}

/// Fila em (0, 0) e saída em (20, 0) com pontos de acesso em (0, -3) e (20, -3); um
/// corredor de empilhadeiras reto entre eles (20 m) e um de pedestres mais afastado, em
/// y = -6 (3 m de ligação em cada ponta: 26 m).
fn two_aisles(asset: &str) -> String {
    format!(
        r#"Model(
            format: 2,
            name: "classes",
            run: (stop_time: 1000.0, movement: Aisles),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (0.0, 5.0, 0.0)),
                (name: "q", kind: Queue(), transport: "r", position: (0.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "out", kind: Sink, position: (20.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "r", kind: Transporter(speed: 1.0), position: (0.0, -3.0, 0.0),
                 visual: (asset: "{asset}")),
            ],
            connections: [("src", "q"), ("q", "out")],
            aisles: [
                (name: "via_emp", kind: Forklift, points: [(-2.0, -3.0), (22.0, -3.0)], width: 3.6),
                (name: "passeio", kind: Pedestrian, points: [(-2.0, -6.0), (22.0, -6.0)], width: 1.2),
            ],
        )"#
    )
}

#[test]
fn people_keep_to_pedestrian_aisles_and_vehicles_to_theirs() {
    let person = model(&two_aisles("operador"));
    let r = person.run_replications().unwrap().remove(0);
    // 3 m down to the walkway, 20 m along it, 3 m up: never the shorter forklift aisle.
    assert!((tis(&r, "out") - 26.0).abs() < 1e-9, "{}", tis(&r, "out"));
    let vehicle = model(&two_aisles("empilhadeira"));
    let r = vehicle.run_replications().unwrap().remove(0);
    assert!((tis(&r, "out") - 20.0).abs() < 1e-9, "{}", tis(&r, "out"));
    // The routes themselves, by hand.
    let route = person
        .nav_set()
        .router("r")
        .unwrap()
        .route([0.0, -3.0, 0.0], [20.0, 0.0, 0.0]);
    assert_eq!(
        route,
        vec![
            [0.0, -3.0, 0.0],
            [0.0, -6.0, 0.0],
            [20.0, -6.0, 0.0],
            [20.0, -3.0, 0.0]
        ]
    );
    let lines = person.movement_lines();
    assert!(lines[0].contains("pelos corredores"), "{lines:?}");
    assert!(lines[0].contains("de pedestres ou misto"), "{lines:?}");
}

/// Corredor de empilhadeiras horizontal (y = 0, 3,6 m) e de pedestres vertical (x = 0,
/// 1,2 m) em cruz; a empilhadeira vai de (-10, 0) a (10, 0) e a pessoa de (0, -10) a
/// (0, 10), as duas saindo em t = 0.
fn crossing(with_person: bool, stop: &str) -> String {
    let person = if with_person {
        r#"(name: "src2", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
             position: (6.0, -12.0, 0.0)),
            (name: "q2", kind: Queue(), transport: "op", position: (3.0, -10.0, 0.0), access: (-3.0, 0.0)),
            (name: "out2", kind: Sink, position: (3.0, 10.0, 0.0), access: (-3.0, 0.0)),
            (name: "op", kind: Transporter(speed: 1.0), position: (0.0, -10.0, 0.0),
             visual: (asset: "operador")),"#
    } else {
        ""
    };
    let links = if with_person {
        r#", ("src2", "q2"), ("q2", "out2")"#
    } else {
        ""
    };
    format!(
        r#"Model(
            format: 2,
            name: "cruzamento",
            run: (stop_time: 1000.0, movement: Aisles),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (-12.0, 6.0, 0.0)),
                (name: "q", kind: Queue(), transport: "emp", position: (-10.0, 3.0, 0.0), access: (0.0, -3.0)),
                (name: "out", kind: Sink, position: (10.0, 3.0, 0.0), access: (0.0, -3.0)),
                (name: "emp", kind: Transporter(speed: 1.0), position: (-10.0, 0.0, 0.0),
                 visual: (asset: "empilhadeira")),
                {person}
            ],
            connections: [("src", "q"), ("q", "out"){links}],
            aisles: [
                (name: "corredor_A", kind: Forklift, points: [(-10.0, 0.0), (10.0, 0.0)], width: 3.6),
                (name: "passeio_1", kind: Pedestrian, points: [(0.0, -10.0), (0.0, 10.0)], width: 1.2),
            ],
            {stop}
        )"#
    )
}

#[test]
fn the_vehicle_stops_two_seconds_before_the_crossing() {
    let m = model(&crossing(false, ""));
    let lines = m.aisle_lines();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("cruzamento corredor_A x passeio_1")),
        "{lines:?}"
    );
    let r = m.run_replications().unwrap().remove(0);
    // Zone of 3.6 m around (0, 0): 8.2 m, 2 s stop, 3.6 m, 8.2 m.
    assert!((tis(&r, "out") - 22.0).abs() < 1e-9, "{}", tis(&r, "out"));
    // The stop is configurable.
    let m = model(&crossing(false, "floor_marking: (crossing_stop: 0.5),"));
    let r = m.run_replications().unwrap().remove(0);
    assert!((tis(&r, "out") - 20.5).abs() < 1e-9, "{}", tis(&r, "out"));
}

#[test]
fn the_vehicle_yields_to_the_pedestrian_crossing() {
    let m = model(&crossing(true, ""));
    let reps = m.run_replications().unwrap();
    let r = &reps[0];
    // The person walks straight through: 20 m, inside the zone from 8.2 s to 11.8 s.
    assert!((tis(r, "out2") - 20.0).abs() < 1e-9, "{}", tis(r, "out2"));
    // The forklift reaches the zone at 8.2 s, stops until 10.2 s, waits for the person
    // to leave at 11.8 s, crosses (3.6 s) and drives the last 8.2 m: 23.6 s.
    assert!((tis(r, "out") - 23.6).abs() < 1e-9, "{}", tis(r, "out"));
    // Deterministic: same trace on every run and replication.
    let again = m.run_replications().unwrap();
    assert_eq!(tis(&again[0], "out"), tis(r, "out"));
}

#[test]
fn widths_below_the_profile_minimum_are_reported() {
    let text = crossing(false, "")
        .replace("width: 3.6", "width: 3.0")
        .replace("width: 1.2", "width: 1.0");
    let w = model(&text).warnings();
    assert!(
        w.iter().any(|s| s.contains("'corredor_A'")
            && s.contains("3,60 m")
            && s.contains("2L + 1,20 m")
            && s.contains("L = 1,20 m de 'emp'")
            && s.contains("não norma")),
        "{w:?}"
    );
    assert!(
        w.iter()
            .any(|s| s.contains("'passeio_1'") && s.contains("1,20 m") && s.contains("secondary")),
        "{w:?}"
    );
    // At the minimum: no width warning.
    let w = model(&crossing(false, "")).warnings();
    assert!(!w.iter().any(|s| s.contains("o mínimo do perfil")), "{w:?}");
}

#[test]
fn a_person_without_pedestrian_aisles_is_reported() {
    let text = two_aisles("operador").replace(
        r#"(name: "passeio", kind: Pedestrian, points: [(-2.0, -6.0), (22.0, -6.0)], width: 1.2),"#,
        "",
    );
    let w = model(&text).warnings();
    assert!(
        w.iter()
            .any(|s| s.contains("'r' (pessoa) não tem corredor de pedestres ou misto")),
        "{w:?}"
    );
}

#[test]
fn aisles_over_obstacles_and_unreachable_access_points_are_reported() {
    let text = two_aisles("empilhadeira").replace(
        "aisles: [",
        r#"obstacles: [(name: "pilar", polygon: [(9.0, -4.0), (11.0, -4.0), (11.0, -2.0), (9.0, -2.0)])],
            aisles: ["#,
    );
    let m = model(&text);
    let w = m.warnings();
    assert!(
        w.iter()
            .any(|s| s.contains("o eixo do corredor 'via_emp' cruza 'pilar'")),
        "{w:?}"
    );
    assert!(
        w.iter()
            .any(|s| s.contains("'r' não alcança o ponto de acesso de 'out' pelos corredores")),
        "{w:?}"
    );
    let svg = m.aisles_svg();
    assert!(svg.starts_with("<svg") && svg.contains("via_emp") && svg.contains("pilar"));
}

#[test]
fn aisles_mode_needs_aisles_and_old_models_do_not_change() {
    let text = r#"Model(format: 2, name: "x", run: (stop_time: 10.0, movement: Aisles),
        objects: [(name: "p", kind: Transporter(speed: 1.0))])"#;
    let e = Model::parse(text)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    assert!(e.contains("Aisles exige `aisles`"), "{e}");
    // A model without aisles writes no aisle fields (the file stays as it was).
    let m = model(&two_aisles("operador").replace(
        r#"aisles: [
                (name: "via_emp", kind: Forklift, points: [(-2.0, -3.0), (22.0, -3.0)], width: 3.6),
                (name: "passeio", kind: Pedestrian, points: [(-2.0, -6.0), (22.0, -6.0)], width: 1.2),
            ],"#,
        "",
    )
    .replace("movement: Aisles", "movement: Free"));
    let ron = ron::to_string(&m).unwrap();
    assert!(
        !ron.contains("aisles") && !ron.contains("floor_marking"),
        "{ron}"
    );
    assert!(m.floor_marks().is_empty());
}

#[test]
fn floor_marks_follow_the_profile() {
    let m = model(&crossing(false, "floor_marking: (profile: Branco),"));
    let marks = m.floor_marks();
    assert!(
        marks
            .iter()
            .filter(|k| k.pattern == utsim_model::Pattern::Continuous)
            .all(|k| k.color == "#FFFFFF")
    );
    assert!(
        marks
            .iter()
            .any(|k| k.pattern == utsim_model::Pattern::Zebra)
    );
}
