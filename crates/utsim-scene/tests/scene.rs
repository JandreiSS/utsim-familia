//! The composed scene (front K): hand-checked poses on small models, determinism, the
//! JSON-lines stream and the shipped examples.

use std::f32::consts::FRAC_PI_2;
use std::path::Path;

use utsim_model::Model;
use utsim_scene::{
    EntityId, EntityKind, Frame, Header, JsonLines, MobileRole, Motion, Pose, Scene, SceneOptions,
    SceneSink,
};

const EPS: f32 = 1e-4;

fn scene_of(model: &Model) -> Scene {
    let replay = model.replay(0).unwrap();
    Scene::build(
        &replay,
        &SceneOptions {
            model: Some(model),
            ..SceneOptions::default()
        },
    )
}

fn block(scene: &Scene, frame: &Frame, name: &str) -> Pose {
    let b = (0..scene.positions.len())
        .find(|&b| {
            scene.static_scene().entities.iter().any(|e| {
                e.id == EntityId::Block(b as u32)
                    && matches!(&e.kind,
                        EntityKind::Block { name: n, .. } | EntityKind::Mobile { name: n, .. }
                        if n == name)
            })
        })
        .unwrap_or_else(|| panic!("no block {name}"));
    frame
        .poses
        .iter()
        .find(|p| p.id == EntityId::Block(b as u32))
        .cloned()
        .unwrap()
}

fn items(frame: &Frame) -> Vec<&Pose> {
    frame
        .poses
        .iter()
        .filter(|p| matches!(p.id, EntityId::Item(_)))
        .collect()
}

/// One item made at t = 0, carried by `t` (speed 2, load 2, unload 2) from the source
/// at the origin to a sink at `target`. Timeline: load 0..2, drive 2..7 (10 m),
/// unload 7..9.
fn carry(target: (f64, f64), extra: &str) -> Model {
    let text = format!(
        r#"Model(
        name: "carry",
        run: (stop_time: 30.0, seed: 1, replications: 1),
        objects: [
            (name: "src", kind: Source(interarrival: Const(100.0), first_at_zero: true),
             position: (0.0, 0.0, 0.0), transport: Some("t")),
            (name: "sink", kind: Sink, position: ({}, {}, 0.0)),
            (name: "t", kind: Transporter(speed: 2.0, load_time: Const(2.0),
             unload_time: Const(2.0)), position: (0.0, 0.0, 0.0){extra}),
        ],
        connections: [("src", "sink")],
    )"#,
        target.0, target.1
    );
    Model::parse(&text).unwrap()
}

#[test]
fn transporter_drives_along_x_at_its_speed() {
    let scene = scene_of(&carry((10.0, 0.0), ""));
    let f = scene.frame(4.5);
    let t = block(&scene, &f, "t");
    // Half of the 10 m leg (2..7).
    assert!((t.position[0] - 5.0).abs() < EPS, "{:?}", t.position);
    assert!(t.position[1].abs() < EPS && t.position[2].abs() < EPS);
    assert!(t.yaw.abs() < EPS, "yaw {}", t.yaw);
    assert_eq!(t.motion, Motion::Moving { speed: 2.0 });
    // The carried item rides on the transporter.
    let carried = items(&f);
    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0].parent, Some(t.id));
    assert!((carried[0].position[0] - 5.0).abs() < 1.0);
    // Stopped at the sink: it keeps the heading of the leg.
    let t = block(&scene, &scene.frame(20.0), "t");
    assert!((t.position[0] - 10.0).abs() < EPS && t.yaw.abs() < EPS);
    assert_eq!(t.motion, Motion::Idle);
}

#[test]
fn heading_follows_plus_y_and_is_held_when_stopped() {
    let scene = scene_of(&carry((0.0, 10.0), ""));
    let t = block(&scene, &scene.frame(4.5), "t");
    assert!((t.position[1] - 5.0).abs() < EPS, "{:?}", t.position);
    assert!((t.yaw - FRAC_PI_2).abs() < EPS, "yaw {}", t.yaw);
    let t = block(&scene, &scene.frame(25.0), "t");
    assert!((t.yaw - FRAC_PI_2).abs() < EPS, "held yaw {}", t.yaw);
}

#[test]
fn load_and_unload_progress_over_their_times() {
    let scene = scene_of(&carry((10.0, 0.0), ""));
    let at = |t: f64| block(&scene, &scene.frame(t), "t").motion;
    assert_eq!(at(1.0), Motion::Loading { progress: 0.5 });
    assert_eq!(at(0.5), Motion::Loading { progress: 0.25 });
    assert_eq!(at(8.5), Motion::Unloading { progress: 0.75 });
    // A generic vehicle has no lift; a person carries at hand height.
    assert_eq!(block(&scene, &scene.frame(4.5), "t").lift, 0.0);
    let person = scene_of(&carry(
        (10.0, 0.0),
        r#", visual: Some((asset: Some("operador")))"#,
    ));
    let p = block(&person, &person.frame(4.5), "t");
    assert!((p.lift - 1.0).abs() < EPS);
    assert_eq!(block(&person, &person.frame(20.0), "t").lift, 0.0);
}

#[test]
fn acceleration_profile_sets_the_instantaneous_speed() {
    // speed 2, accel = decel = 1 over 10 m: 2 s up, 3 s cruise, 2 s down (7 s).
    let text = r#"Model(
        name: "acc",
        run: (stop_time: 40.0, seed: 1, replications: 1),
        objects: [
            (name: "src", kind: Source(interarrival: Const(100.0), first_at_zero: true),
             position: (0.0, 0.0, 0.0), transport: Some("t")),
            (name: "sink", kind: Sink, position: (10.0, 0.0, 0.0)),
            (name: "t", kind: Transporter(speed: 2.0, acceleration: Some(1.0),
             deceleration: Some(1.0)), position: (0.0, 0.0, 0.0)),
        ],
        connections: [("src", "sink")],
    )"#;
    let scene = scene_of(&Model::parse(text).unwrap());
    let speed = |t: f64| match block(&scene, &scene.frame(t), "t").motion {
        Motion::Moving { speed } => speed,
        m => panic!("not moving at {t}: {m:?}"),
    };
    assert!((speed(1.0) - 1.0).abs() < EPS);
    assert!((speed(3.5) - 2.0).abs() < EPS);
    assert!((speed(6.5) - 0.5).abs() < EPS);
}

#[test]
fn forklift_lifts_to_the_rack_cell() {
    // Three loads stored in a one-bay, three-level rack (levels 1.5 apart): the third
    // goes to level 2, whose deck is 2 * 1.5 + 0.08 (beam) above the floor.
    let text = r#"Model(
        name: "rack",
        run: (stop_time: 200.0, seed: 1, replications: 1),
        objects: [
            (name: "src", kind: Source(interarrival: Const(20.0), first_at_zero: true),
             position: (0.0, 0.0, 0.0), transport: Some("f")),
            (name: "rack", kind: Rack(bays: 1, levels: 3, dwell: Some(Const(1000.0)),
             level_height: 1.5), position: (10.0, 0.0, 0.0)),
            (name: "sink", kind: Sink, position: (20.0, 0.0, 0.0)),
            (name: "f", kind: Transporter(speed: 2.0, load_time: Const(2.0),
             unload_time: Const(2.0)), position: (0.0, 0.0, 0.0)),
        ],
        connections: [("src", "rack"), ("rack", "sink")],
    )"#;
    let model = Model::parse(text).unwrap();
    let scene = scene_of(&model);
    let f = (0..scene.positions.len())
        .find(|&b| scene.role(b).is_some())
        .unwrap();
    assert_eq!(scene.role(f), Some(MobileRole::Forklift));
    // Find the third unloading window and look at its middle.
    let mut windows = Vec::new();
    let mut prev = None;
    for k in 0..2000 {
        let t = k as f64 * 0.05;
        let m = scene.mobile_at(f, t).unwrap();
        if let Motion::Unloading { .. } = m.motion {
            if prev.is_none() {
                prev = Some(t);
            }
        } else if let Some(s) = prev.take() {
            windows.push((s, t));
        }
    }
    assert!(windows.len() >= 3, "{windows:?}");
    let (s, e) = windows[2];
    let mid = (s + e) / 2.0;
    let m = scene.mobile_at(f, mid).unwrap();
    assert!((m.lift - 3.08).abs() < 0.01, "lift {} at {mid}", m.lift);
    // Level 0 for the first one; travel height loaded, forks lowered empty.
    let (s, e) = windows[0];
    let m = scene.mobile_at(f, (s + e) / 2.0).unwrap();
    assert!((m.lift - 0.08).abs() < 0.01, "lift {}", m.lift);
    let driving_loaded = (0..400)
        .map(|k| k as f64 * 0.05)
        .filter_map(|t| scene.mobile_at(f, t).map(|m| (t, m)))
        .find(|(t, m)| {
            matches!(m.motion, Motion::Moving { .. }) && scene.timeline.block_content(f, *t) > 0
        })
        .unwrap();
    assert!((driving_loaded.1.lift - 0.3).abs() < EPS);
}

#[test]
fn items_exist_only_between_creation_and_destruction() {
    let model = carry((10.0, 0.0), "");
    let scene = scene_of(&model);
    // The single item is destroyed by the sink at the end of the unload (t = 9).
    assert_eq!(items(&scene.frame(1.0)).len(), 1);
    assert_eq!(items(&scene.frame(8.9)).len(), 1);
    assert!(items(&scene.frame(9.5)).is_empty());
    // Before creation: a source whose first item comes at t = 5.
    let text = r#"Model(
        name: "late",
        run: (stop_time: 6.0, seed: 1, replications: 1),
        objects: [
            (name: "src", kind: Source(interarrival: Const(5.0)), position: (0.0, 0.0, 0.0)),
            (name: "proc", kind: Processor(process: Const(100.0)), position: (4.0, 0.0, 0.0)),
            (name: "sink", kind: Sink, position: (8.0, 0.0, 0.0)),
        ],
        connections: [("src", "proc"), ("proc", "sink")],
    )"#;
    let scene = scene_of(&Model::parse(text).unwrap());
    assert!(items(&scene.frame(4.9)).is_empty());
    let late = scene.frame(5.9);
    let it = items(&late);
    assert_eq!(it.len(), 1);
    assert!(it[0].parent.is_some());
}

#[test]
fn tank_fill_follows_the_level() {
    let text = r#"Model(
        name: "tank",
        run: (stop_time: 20.0, seed: 1, replications: 1),
        objects: [
            (name: "src", kind: Source(interarrival: Const(100.0))),
            (name: "sink", kind: Sink),
        ],
        connections: [("src", "sink")],
        fluids: Some((
            name: "f",
            nodes: [
                (name: "in", kind: Source(rate: 1.0)),
                (name: "tank", kind: Tank(capacity: 10.0)),
            ],
            links: [("in", "tank")],
        )),
    )"#;
    let scene = scene_of(&Model::parse(text).unwrap());
    let fill = |t: f64| {
        scene
            .frame(t)
            .poses
            .iter()
            .find(|p| p.id == EntityId::FluidNode(1))
            .unwrap()
            .fill
            .unwrap()
    };
    assert!((fill(5.0) - 0.5).abs() < 1e-3, "{}", fill(5.0));
    assert!((fill(15.0) - 1.0).abs() < 1e-3);
    let src = scene.frame(5.0);
    let node = src
        .poses
        .iter()
        .find(|p| p.id == EntityId::FluidNode(0))
        .unwrap();
    assert_eq!(node.fill, None);
}

#[test]
fn frames_are_deterministic() {
    let model = example("agvs.ron");
    let a = scene_of(&model);
    let b = scene_of(&model);
    assert_eq!(a.header(), b.header());
    for k in 0..50 {
        let t = k as f64 * 7.3;
        assert_eq!(a.frame(t), b.frame(t));
    }
}

#[test]
fn json_lines_round_trip() {
    let scene = scene_of(&carry((10.0, 0.0), ""));
    let mut out = Vec::new();
    let mut sink = JsonLines(&mut out);
    sink.header(&scene.header()).unwrap();
    for t in [0.0, 1.0, 4.5, 8.5] {
        sink.frame(&scene.frame(t)).unwrap();
    }
    let text = String::from_utf8(out).unwrap();
    let mut lines = text.lines();
    let header: Header = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(header, scene.header());
    assert_eq!(header.protocol, utsim_scene::PROTOCOL);
    for t in [0.0, 1.0, 4.5, 8.5] {
        let frame: Frame = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(frame, scene.frame(t));
    }
    assert!(lines.next().is_none());
}

fn example(name: &str) -> Model {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    Model::load(path.join(name)).unwrap()
}

fn all_finite(pose: &Pose) -> bool {
    pose.position.iter().all(|c| c.is_finite())
        && pose.yaw.is_finite()
        && pose.lift.is_finite()
        && pose.fill.is_none_or(f32::is_finite)
        && match pose.motion {
            Motion::Moving { speed } => speed.is_finite() && speed >= 0.0,
            Motion::Loading { progress } | Motion::Unloading { progress } => {
                (0.0..=1.0).contains(&progress)
            }
            _ => true,
        }
}

#[test]
fn shipped_examples_build_finite_scenes() {
    for name in [
        "agvs.ron",
        "armazem.ron",
        "tanques.ron",
        "rede_de_caminhos.ron",
        "centro_de_distribuicao.ron",
        "celula_com_operadores.ron",
        "linha_com_esteiras.ron",
        "montagem_paletes.ron",
    ] {
        let mut model = example(name);
        model.run.stop_time = model.run.stop_time.min(600.0);
        model.run.warmup = 0.0;
        let scene = scene_of(&model);
        let header = scene.header();
        assert!(!header.scene.entities.is_empty(), "{name}");
        let (lo, hi) = header.scene.bounds;
        assert!(lo.iter().chain(&hi).all(|c| c.is_finite()), "{name}");
        let steps = 300;
        for k in 0..=steps {
            let t = scene.end * k as f64 / steps as f64;
            for pose in &scene.frame(t).poses {
                assert!(all_finite(pose), "{name} at {t}: {pose:?}");
            }
        }
    }
}

#[test]
fn roles_follow_the_model() {
    let agvs = scene_of(&example("agvs.ron"));
    let roles: Vec<MobileRole> = (0..agvs.positions.len())
        .filter_map(|b| agvs.role(b))
        .collect();
    assert!(!roles.is_empty() && roles.iter().all(|r| *r == MobileRole::Agv));
    let cell = scene_of(&example("celula_com_operadores.ron"));
    assert!(
        (0..cell.positions.len()).any(|b| cell.role(b) == Some(MobileRole::Person)),
        "operators are people"
    );
    // Without the model every transporter is a generic vehicle.
    let replay = example("agvs.ron").replay(0).unwrap();
    let bare = Scene::build(&replay, &SceneOptions::default());
    assert!(
        (0..bare.positions.len())
            .filter_map(|b| bare.role(b))
            .all(|r| r == MobileRole::Vehicle)
    );
}
