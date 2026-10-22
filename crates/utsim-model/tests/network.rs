//! Path networks: shortest routes, travel times along them and validation.

use utsim_core::Router;
use utsim_core::task::path_length;
use utsim_model::Model;
use utsim_objects::network::{EdgeDef, NetworkDef, NetworkRouter, NodeDef, router};

fn node(name: &str, x: f64, y: f64) -> NodeDef {
    NodeDef {
        name: name.into(),
        position: [x, y, 0.0],
    }
}

fn edge(from: &str, to: &str, two_way: bool) -> EdgeDef {
    EdgeDef {
        from: from.into(),
        to: to.into(),
        two_way,
        capacity: None,
    }
}

/// Grid (spacing 10):
///   D(0,10) <- E(10,10) <- F(20,10)
///     |          ^|          ^
///     v          |v          |
///   A(0,0)  ->  B(10,0) ->  C(20,0)
/// The perimeter is a one-way loop A>B>C>F>E>D>A; B-E is a two-way cross corridor.
fn loop_with_shortcut() -> NetworkDef {
    NetworkDef {
        nodes: vec![
            node("A", 0.0, 0.0),
            node("B", 10.0, 0.0),
            node("C", 20.0, 0.0),
            node("D", 0.0, 10.0),
            node("E", 10.0, 10.0),
            node("F", 20.0, 10.0),
        ],
        edges: vec![
            edge("A", "B", false),
            edge("B", "C", false),
            edge("C", "F", false),
            edge("F", "E", false),
            edge("E", "D", false),
            edge("D", "A", false),
            edge("B", "E", true),
        ],
    }
}

fn p(x: f64, y: f64) -> [f64; 3] {
    [x, y, 0.0]
}

#[test]
fn one_way_edges_make_routes_asymmetric() {
    let net = loop_with_shortcut();
    assert!(net.validate().is_empty(), "{:?}", net.validate());
    let r = NetworkRouter::new(&net);
    // A -> D: A>B (10), B>E (10), E>D (10) = 30; the loop A>B>C>F>E>D costs 50.
    let path = r.route(p(0.0, 0.0), p(0.0, 10.0));
    assert_eq!(
        path,
        vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]
    );
    assert_eq!(path_length(&path), 30.0);
    // D -> A: the one-way edge D>A, 10.
    let back = r.route(p(0.0, 10.0), p(0.0, 0.0));
    assert_eq!(back, vec![p(0.0, 10.0), p(0.0, 0.0)]);
    // E -> F: E>B (10, two-way), B>C (10), C>F (10) = 30; the loop E>D>A>B>C>F is 50.
    let ef = r.route(p(10.0, 10.0), p(20.0, 10.0));
    assert_eq!(path_length(&ef), 30.0);
    assert_eq!(ef.len(), 4);
    // F -> C: C>F is one-way the other way, so F>E>B>C = 30.
    let fc = r.route(p(20.0, 10.0), p(20.0, 0.0));
    assert_eq!(
        fc,
        vec![p(20.0, 10.0), p(10.0, 10.0), p(10.0, 0.0), p(20.0, 0.0)]
    );
    assert_eq!(path_length(&fc), 30.0);
}

#[test]
fn points_off_the_network_attach_to_the_nearest_node() {
    let r = NetworkRouter::new(&loop_with_shortcut());
    // (-3,-4) is 5 from A; (20,12) is 2 from F. A>B>C>F = 30.
    let path = r.route(p(-3.0, -4.0), p(20.0, 12.0));
    assert_eq!(
        path,
        vec![
            p(-3.0, -4.0),
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(20.0, 0.0),
            p(20.0, 10.0),
            p(20.0, 12.0)
        ]
    );
    assert_eq!(path_length(&path), 5.0 + 30.0 + 2.0);
    // Same point: no movement.
    assert_eq!(r.route(p(1.0, 2.0), p(1.0, 2.0)), vec![p(1.0, 2.0)]);
    // Same attachment node: through the node, no duplicated waypoints.
    let near = r.route(p(0.0, 0.0), p(1.0, 0.0));
    assert_eq!(near, vec![p(0.0, 0.0), p(1.0, 0.0)]);
}

#[test]
fn ties_resolve_deterministically_by_node_order() {
    // Square with two equal paths A>B>D and A>C>D (20 each): B is listed first.
    let net = NetworkDef {
        nodes: vec![
            node("A", 0.0, 0.0),
            node("B", 10.0, 0.0),
            node("C", 0.0, 10.0),
            node("D", 10.0, 10.0),
        ],
        edges: vec![
            edge("A", "B", true),
            edge("A", "C", true),
            edge("B", "D", true),
            edge("C", "D", true),
        ],
    };
    for _ in 0..50 {
        let r = NetworkRouter::new(&net);
        assert_eq!(
            r.route(p(0.0, 0.0), p(10.0, 10.0)),
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)]
        );
        // (5,5) is equidistant from all four nodes: the first one (A) wins.
        let path = r.route(p(5.0, 5.0), p(10.0, 10.0));
        assert_eq!(
            path,
            vec![p(5.0, 5.0), p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)]
        );
    }
}

#[test]
fn router_without_network_is_a_straight_line() {
    let r = router(None);
    assert_eq!(r.route(p(0.0, 0.0), p(6.0, 8.0)).len(), 2);
    let r = router(Some(&loop_with_shortcut()));
    assert!(r.route(p(0.0, 0.0), p(20.0, 10.0)).len() > 2);
}

#[test]
fn validate_reports_unreachable_nodes() {
    let split = NetworkDef {
        nodes: vec![
            node("A", 0.0, 0.0),
            node("B", 1.0, 0.0),
            node("C", 5.0, 0.0),
        ],
        edges: vec![edge("A", "B", true)],
    };
    let errors = split.validate();
    assert!(
        errors.iter().any(|e| e.contains("o nó 'A' não alcança: C")),
        "{errors:?}"
    );
    assert_eq!(errors.len(), 3, "{errors:?}");

    // A one-way dead end is also reported (B cannot go back).
    let dead_end = NetworkDef {
        nodes: vec![node("A", 0.0, 0.0), node("B", 1.0, 0.0)],
        edges: vec![edge("A", "B", false)],
    };
    let errors = dead_end.validate();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("o nó 'B' não alcança: A"));

    // The same through the model.
    let m = Model::parse(
        r#"Model(
            name: "rede partida",
            run: (stop_time: 10.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "out")],
            network: Some((
                nodes: [(name: "a", position: (0.0, 0.0, 0.0)), (name: "b", position: (5.0, 0.0, 0.0))],
                edges: [],
            )),
        )"#,
    )
    .unwrap();
    let err = m.validate().unwrap_err().to_string();
    assert!(err.contains("não alcança"), "{err}");
}

#[test]
fn forklift_delivery_times_follow_the_network_path() {
    let (n, min, max) = forklift_run(20.0, 110.0);
    // Items every 20 (t=0,20,..). Item 0: forklift already at the queue, delivered at
    // 7, done at 8. Item n>=1: the forklift is at the machine, comes back (7), loads
    // and delivers (7): done at 20n+15, so 15 in the system. Done by t=110: n=0..=4.
    assert_eq!(n, 5);
    assert!(
        (min - 8.0).abs() < 1e-12 && (max - 15.0).abs() < 1e-12,
        "{min} {max}"
    );
    // Items every 10: the 14-long round trip saturates the forklift, which only leaves
    // for a task once it exists. Item 0 as above. Item k>=1 is picked up at 14k+3,
    // delivered at 14k+10, done at 14k+11: 4k+11 in the system. By t=100 items
    // k=0..=6 are done (95); the longest stay is 4*6+11 = 35.
    let (n, min, max) = forklift_run(10.0, 100.0);
    assert_eq!(n, 7);
    assert!(
        (min - 8.0).abs() < 1e-12 && (max - 35.0).abs() < 1e-12,
        "{min} {max}"
    );
}

fn forklift_run(interarrival: f64, stop: f64) -> (u64, f64, f64) {
    // Queue at (0,0), machine at (6,8): the straight line is 10, but the corridor
    // goes (0,0) -> (0,8) -> (6,8) = 8 + 6 = 14. Speed 2 -> 7 per trip.
    let text = r#"Model(
            name: "empilhadeira em corredor",
            run: (stop_time: STOP),
            objects: [
                (name: "src", kind: Source(interarrival: Const(GAP), first_at_zero: true)),
                (name: "q", kind: Queue(), transport: "empilhadeira"),
                (name: "p", kind: Processor(capacity: 2, process: Const(1.0)), position: (6.0, 8.0, 0.0)),
                (name: "out", kind: Sink, position: (6.0, 8.0, 0.0)),
                (name: "empilhadeira", kind: Transporter(speed: 2.0)),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
            network: Some((
                nodes: [
                    (name: "doca", position: (0.0, 0.0, 0.0)),
                    (name: "esquina", position: (0.0, 8.0, 0.0)),
                    (name: "maquina", position: (6.0, 8.0, 0.0)),
                ],
                edges: [(from: "doca", to: "esquina"), (from: "esquina", to: "maquina")],
            )),
        )"#
    .replace("STOP", &format!("{stop:?}"))
    .replace("GAP", &format!("{interarrival:?}"));
    let m = Model::parse(&text).unwrap();
    m.validate().expect("valid");
    let r = &m.run_replications().unwrap()[0];
    let out = r.block("out").unwrap();
    let tis = &out.extra["time_in_system"];
    (out.input, tis.min, tis.max)
}
