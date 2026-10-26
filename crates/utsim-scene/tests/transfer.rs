//! Items on a timed direct connection (`transfer`) in the replay timeline: they leave the
//! origin block at once, travel in neither block, and arrive at the end of the trip.

use utsim_model::Model;
use utsim_scene::timeline::{ItemPos, Timeline, TransitSetting};

fn timeline(text: &str) -> Timeline {
    let model = Model::parse(text).unwrap();
    let replay = model.replay(0).unwrap();
    Timeline::build(&replay, TransitSetting::Auto)
}

const STRAIGHT: &str = r#"Model(
    name: "reta",
    run: (stop_time: 100.0),
    objects: [
        (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
         transfer: (speed: 1.0, delay: 4.0)),
        (name: "q", kind: Processor(process: Const(30.0)), position: (20.0, 0.0, 0.0)),
        (name: "out", kind: Sink, position: (20.0, 0.0, 0.0)),
    ],
    connections: [("src", "q"), ("q", "out")],
)"#;

#[test]
fn item_flies_between_departure_and_arrival() {
    let tl = timeline(STRAIGHT);
    // Departs at 0, waits 4 s, then 20 s of motion: arrives at 24.
    assert_eq!(
        tl.item_pos(0, 0.0),
        ItemPos::Flight {
            from: 0,
            to: 1,
            progress: 0.0,
            flight: 0
        }
    );
    assert!(matches!(
        tl.item_pos(0, 3.9),
        ItemPos::Flight { progress, .. } if progress == 0.0
    ));
    let ItemPos::Flight {
        progress, flight, ..
    } = tl.item_pos(0, 14.0)
    else {
        panic!("deveria estar a caminho");
    };
    assert!((progress - 0.5).abs() < 1e-12);
    // Halfway along the straight line, in model space.
    let p = tl.flight_point(flight, progress).unwrap();
    assert!((p[0] - 10.0).abs() < 1e-9 && p[1].abs() < 1e-9);
    assert!(!tl.flight_is_routed(flight));
    assert_eq!(tl.item_pos(0, 24.0), ItemPos::At(1));
    assert_eq!(tl.item_pos(0, 60.0), ItemPos::Gone);
}

#[test]
fn travelling_item_belongs_to_no_block() {
    let tl = timeline(STRAIGHT);
    for t in [0.0, 10.0, 23.9] {
        assert_eq!(tl.block_content(0, t), 0, "origem em t={t}");
        assert_eq!(tl.block_content(1, t), 0, "destino em t={t}");
    }
    assert_eq!(tl.block_content(1, 24.0), 1);
    // The arrival still counts at the destination, once.
    assert_eq!(tl.block_received(1, 23.9), 0);
    assert_eq!(tl.block_received(1, 24.0), 1);
}

#[test]
fn network_trip_follows_the_route() {
    let tl = timeline(
        r#"Model(
        name: "rede",
        run: (stop_time: 200.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
             transfer: (speed: 1.0, path: Network)),
            (name: "out", kind: Sink, position: (30.0, 40.0, 0.0)),
        ],
        connections: [("src", "out")],
        network: Some((
            nodes: [
                (name: "A", position: (0.0, 0.0, 0.0)),
                (name: "B", position: (30.0, 0.0, 0.0)),
                (name: "C", position: (30.0, 40.0, 0.0)),
            ],
            edges: [(from: "A", to: "B"), (from: "B", to: "C")],
        )),
    )"#,
    );
    let ItemPos::Flight { flight, .. } = tl.item_pos(0, 30.0) else {
        panic!("deveria estar a caminho");
    };
    assert!(tl.flight_is_routed(flight));
    // 30 m along A -> B puts the item exactly at node B; 70 m long in total.
    let p = tl.flight_point(flight, 30.0 / 70.0).unwrap();
    assert!((p[0] - 30.0).abs() < 1e-9 && p[1].abs() < 1e-9, "{p:?}");
    assert!(matches!(tl.item_pos(0, 69.9), ItemPos::Flight { .. }));
    assert_eq!(tl.item_pos(0, 70.0), ItemPos::Gone);
    assert_eq!(tl.block_received(1, 69.9), 0);
    assert_eq!(tl.block_received(1, 70.0), 1);
}
