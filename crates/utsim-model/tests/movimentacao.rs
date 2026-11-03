//! Modos de movimentação (formato 2): caminho livre com distância conhecida, desvio de
//! obstáculo, pontos de acesso, modo rede, determinismo, rede gerada e avisos.
//!
//! Planta base: fila `q` em (0, 0) e saída `out` em (10, 0), com pontos de acesso dados à
//! mão em `(0, -3)` e `(10, -3)` (`access: (0.0, -3.0)`), servidas pela empilhadeira `frota`
//! (velocidade 1, sem aceleração) que começa em `(0, -3)`: o tempo no sistema do único item
//! é o comprimento da viagem carregada.

use utsim_core::RunReport;
use utsim_core::task::path_length;
use utsim_model::{Model, Movement};

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn plant(movement: &str, obstacles: &str, network: &str) -> String {
    format!(
        r#"Model(
            format: 2,
            name: "movimentacao",
            run: (stop_time: 1000.0, {movement}),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (0.0, 5.0, 0.0)),
                (name: "q", kind: Queue(), transport: "frota", position: (0.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "out", kind: Sink, position: (10.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "frota", kind: Transporter(speed: 1.0, clearance: 0.5), position: (0.0, -3.0, 0.0)),
            ],
            connections: [("src", "q"), ("q", "out")],
            obstacles: [{obstacles}],
            {network}
        )"#
    )
}

const BLOCK: &str =
    r#"(name: "pilar", polygon: [(4.0, -4.0), (6.0, -4.0), (6.0, -2.0), (4.0, -2.0)])"#;

fn tis(r: &RunReport) -> f64 {
    r.block("out").unwrap().extra["time_in_system"].max
}

/// Exact shortest length between `(-d, 0)` and `(d, 0)` around a square of half side `s`
/// centred at the origin, kept `r` away from it (round corners).
fn around_square(d: f64, s: f64, r: f64) -> f64 {
    let c = [-s, s];
    let l = ((d - s).powi(2) + s * s).sqrt();
    let tangent = (l * l - r * r).sqrt();
    let to_a = (0.0 - c[1]).atan2(-d - c[0]);
    let alpha = (r / l).acos();
    let t_dir = (to_a - alpha).rem_euclid(2.0 * std::f64::consts::PI);
    let arc = (t_dir - std::f64::consts::FRAC_PI_2) * r;
    2.0 * (tangent + arc) + 2.0 * s
}

#[test]
fn free_path_in_an_empty_plant_is_the_straight_line_between_access_points() {
    let m = model(&plant("movement: Free", "", ""));
    let r = m.run_replications().unwrap().remove(0);
    // From (0,-3) to the access point of `out` at (10,-3): 10 m at 1 m/s.
    assert!((tis(&r) - 10.0).abs() < 1e-9, "{}", tis(&r));
    // Legacy (format 1 behaviour): from (0,-3) to the centre of q (3 m), then 10 m to
    // the centre of out: 13.
    let legacy = model(&plant("movement: Legacy", "", ""));
    let r = legacy.run_replications().unwrap().remove(0);
    assert!((tis(&r) - (3.0 + 10.0)).abs() < 1e-9, "{}", tis(&r));
}

#[test]
fn free_path_goes_around_an_obstacle() {
    let m = model(&plant("movement: Free", BLOCK, ""));
    let exact = around_square(5.0, 1.0, 0.5);
    let route = m
        .nav_set()
        .router("frota")
        .unwrap()
        .route([0.0, -3.0, 0.0], [10.0, 0.0, 0.0]);
    let len = path_length(&route);
    assert!(
        len >= exact - 1e-9 && len <= exact + 0.25,
        "{len} vs {exact}"
    );
    assert_eq!(*route.last().unwrap(), [10.0, -3.0, 0.0]);
    // The run takes exactly the route length (speed 1, no ramps).
    let r = m.run_replications().unwrap().remove(0);
    assert!((tis(&r) - len).abs() < 1e-9, "{} vs {len}", tis(&r));
    // A clearance of 0 hugs the square more tightly: shorter, still around it.
    let tight =
        model(&plant("movement: Free", BLOCK, "").replace("clearance: 0.5", "clearance: 0.0"));
    let t = path_length(
        &tight
            .nav_set()
            .router("frota")
            .unwrap()
            .route([0.0, -3.0, 0.0], [10.0, 0.0, 0.0]),
    );
    assert!(t < len && t > 10.0, "{t}");
}

#[test]
fn free_runs_are_deterministic_and_parallel_equals_sequential() {
    let text = plant("movement: Free, replications: 4", BLOCK, "");
    let m = model(&text);
    let json = |r: &RunReport| serde_json::to_string(r).unwrap();
    let a: Vec<String> = m.run_replications().unwrap().iter().map(json).collect();
    let b: Vec<String> = m.run_replications().unwrap().iter().map(json).collect();
    assert_eq!(a, b);
    // One replication built alone (fresh routers, empty cache) matches the batch.
    for (k, rep) in a.iter().enumerate() {
        let mut sim = m.build(k as u32);
        assert_eq!(&json(&sim.run().unwrap()), rep);
    }
}

#[test]
fn network_mode_uses_the_network_and_free_connectors() {
    let net = r#"network: (
        nodes: [(name: "a", position: (0.0, -6.0, 0.0)), (name: "b", position: (10.0, -6.0, 0.0))],
        edges: [(from: "a", to: "b")],
    ),"#;
    let m = model(&plant("movement: Network", BLOCK, net));
    let r = m.run_replications().unwrap().remove(0);
    // (0,-3) → a: 3 m, a → b: 10 m, b → (10,-3): 3 m. The square lies between y = -4
    // and -2, x = 4..6: the connectors are vertical at x = 0 and 10, clear of it.
    assert!((tis(&r) - 16.0).abs() < 1e-9, "{}", tis(&r));
    // Without a network, Network is an error.
    let bad = Model::parse(&plant("movement: Network", "", "")).unwrap();
    let err = bad.validate().unwrap_err().to_string();
    assert!(err.contains("movement = Network exige `network`"), "{err}");
}

#[test]
fn aisles_are_not_available_yet() {
    let bad = Model::parse(&plant("movement: Aisles", "", "")).unwrap();
    let err = bad.validate().unwrap_err().to_string();
    assert!(err.contains("Aisles"), "{err}");
}

#[test]
fn default_access_point_of_a_rack_is_in_front_of_it() {
    let text = r#"Model(
        format: 2,
        name: "estante",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
             transport: "emp", position: (0.0, -8.0, 0.0)),
            (name: "estante", kind: Rack(bays: 4, levels: 2, bay_width: 1.5), position: (0.0, 0.0, 0.0)),
            (name: "out", kind: Sink, position: (0.0, 8.0, 0.0)),
            (name: "emp", kind: Transporter(speed: 1.0), position: (0.0, -8.0, 0.0),
             visual: (asset: "empilhadeira")),
        ],
        connections: [("src", "estante"), ("estante", "out")],
    )"#;
    let m = model(text);
    let emp = m.objects.iter().find(|o| o.name == "emp").unwrap();
    assert_eq!(m.movement_of(emp), Some(Movement::Free));
    // Forklift: half width 0.6 + margin 0.3.
    let c = m.clearance_of(emp);
    assert!((c - 0.9).abs() < 1e-12);
    // Rack 6 m wide and 1 m deep: the forklift stops in front of it (-y), 0.9 + 0.05 off.
    let a = m.access_point("estante", c).unwrap();
    assert!(
        (a[0] - 0.0).abs() < 1e-12 && (a[1] - (-0.5 - 0.95)).abs() < 1e-12,
        "{a:?}"
    );
    let route = m
        .nav_set()
        .router("emp")
        .unwrap()
        .route([0.0, -8.0, 0.0], [0.0, 0.0, 0.0]);
    assert_eq!(*route.last().unwrap(), [a[0], a[1], 0.0]);
    assert!(m.warnings().is_empty(), "{:?}", m.warnings());
}

#[test]
fn generated_network_stays_clear_and_gives_the_free_lengths() {
    let m = model(&plant("movement: Free", BLOCK, ""));
    let g = m.generate_network(None);
    assert!(g.unreachable.is_empty(), "{:?}", g.unreachable);
    let mut with_net = m.clone();
    with_net.network = Some(g.network.clone());
    with_net.run.movement = Some(Movement::Network);
    with_net.validate().unwrap();
    // No warning: no edge over an obstacle and the fingerprint matches.
    assert!(with_net.warnings().is_empty(), "{:?}", with_net.warnings());
    let free = m.run_replications().unwrap().remove(0);
    let net = with_net.run_replications().unwrap().remove(0);
    assert!(
        (tis(&free) - tis(&net)).abs() < 1e-9,
        "{} vs {}",
        tis(&free),
        tis(&net)
    );
    // Moving the obstacle makes the network stale.
    with_net.obstacles[0].polygon[0][0] -= 0.5;
    let w = with_net.warnings();
    assert!(w.iter().any(|s| s.contains("gere de novo")), "{w:?}");
}

#[test]
fn warnings_for_edges_over_obstacles_and_unreachable_points() {
    // A network edge straight through the square.
    let net = r#"network: (
        nodes: [(name: "a", position: (0.0, -3.0, 0.0)), (name: "b", position: (10.0, -3.0, 0.0))],
        edges: [(from: "a", to: "b")],
    ),"#;
    let m = model(&plant("movement: Network", BLOCK, net));
    let w = m.warnings();
    assert!(
        w.iter()
            .any(|s| s.contains("o trecho a → b da rede cruza 'pilar'")),
        "{w:?}"
    );
    // Access point of `out` inside a wall.
    let wall =
        r#"(name: "parede", polygon: [(9.0, -4.0), (11.0, -4.0), (11.0, -2.0), (9.0, -2.0)])"#;
    let m = model(&plant("movement: Free", wall, ""));
    let w = m.warnings();
    assert!(
        w.iter()
            .any(|s| s.contains("o ponto de acesso de 'out'") && s.contains("'parede'")),
        "{w:?}"
    );
    // The transporter boxed in: no path at all.
    let room = r#"(name: "n", polygon: [(-2.0, -1.2), (2.0, -1.2), (2.0, -1.0), (-2.0, -1.0)]),
        (name: "s", polygon: [(-2.0, -5.0), (2.0, -5.0), (2.0, -4.8), (-2.0, -4.8)]),
        (name: "o", polygon: [(-2.0, -5.0), (-1.8, -5.0), (-1.8, -1.0), (-2.0, -1.0)]),
        (name: "l", polygon: [(1.8, -5.0), (2.0, -5.0), (2.0, -1.0), (1.8, -1.0)])"#;
    let text =
        plant("movement: Free", room, "").replacen("access: (0.0, -3.0)", "access: (0.0, 3.0)", 1);
    let m = model(&text);
    let w = m.warnings();
    assert!(
        w.iter()
            .any(|s| s.contains("'frota' não tem caminho livre")),
        "{w:?}"
    );
    // Legacy models get none of these.
    let m = model(&plant("movement: Legacy", wall, net));
    assert!(m.warnings().is_empty(), "{:?}", m.warnings());
}

#[test]
fn check_lists_the_mode_of_every_resource() {
    let m = model(&plant("movement: Free", "", ""));
    assert_eq!(
        m.movement_lines(),
        vec![
            "frota: livre (menor caminho no piso, desviando dos obstáculos) (veículo, folga 0,50 m)"
        ]
    );
}

#[test]
fn agvs_with_a_network_keep_traffic_control_in_any_mode() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/agvs.ron");
    let text = std::fs::read_to_string(path).unwrap();
    let legacy = model(&text);
    let free = model(&text.replacen("Model(", "Model(format: 2, ", 1));
    assert!(free.nav_set().router("agv_1").is_none());
    let lines = free.movement_lines();
    assert!(
        lines
            .iter()
            .all(|l| !l.starts_with("agv_") || l.contains("AGV pela rede")),
        "{lines:?}"
    );
    let json = |m: &Model| serde_json::to_string(&m.run_replications().unwrap()).unwrap();
    assert_eq!(json(&legacy), json(&free));
}

/// Regression: in `Free` movement the stop for a point that is also an object's position
/// is that object's access point. A homing transporter whose block sits on the same
/// point as other objects (here three queues and their sources, all at the origin) so
/// never reached its own position: on arriving it started a zero-length trip "home"
/// again, forever, at the same instant.
#[test]
fn home_on_a_point_shared_with_objects_does_not_loop() {
    let m = model(include_str!("fixtures/home_on_shared_point.ron"));
    let r = m
        .run_replications()
        .expect("the run ends without a same-instant loop")
        .remove(0);
    for s in ["s0", "s1", "s2"] {
        assert_eq!(r.block(s).unwrap().input, 15, "{s}");
    }
}
