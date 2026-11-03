//! Teams in the scene (`docs/plano_chegadas_e_equipes_3d.md`, E8a and E1a): where a
//! moving member is going (`task_target`) and the team and request link lines.

use utsim_model::Model;
use utsim_scene::{EntityId, Scene, SceneOptions};

/// One forklift `m` at the origin: an item at q0 (1, 0) at t = 0 to s0 (2, 0), then one
/// at qF (50, 0) to sF (51, 0).
const TEAM: &str = r#"Model(
    name: "equipe",
    run: (stop_time: 300.0),
    objects: [
        (name: "src0", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1)),
        (name: "srcF", kind: Source(interarrival: Const(1.0), max_arrivals: 1)),
        (name: "q0", kind: Queue(), transport: "eq", position: (1.0, 0.0, 0.0)),
        (name: "qF", kind: Queue(), transport: "eq", position: (50.0, 0.0, 0.0)),
        (name: "s0", kind: Sink, position: (2.0, 0.0, 0.0)),
        (name: "sF", kind: Sink, position: (51.0, 0.0, 0.0)),
        (name: "m", kind: Transporter(speed: 1.0)),
        (name: "eq", kind: Dispatcher(team: ["m"])),
    ],
    connections: [("src0", "q0"), ("srcF", "qF"), ("q0", "s0"), ("qF", "sF")],
)"#;

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

#[test]
fn moving_members_say_where_they_are_going() {
    let model = Model::parse(TEAM).unwrap();
    let scene = scene_of(&model);
    let m = model.objects.iter().position(|o| o.name == "m").unwrap() as u32;
    let target = |t: f64| {
        scene
            .frame(t)
            .poses
            .iter()
            .find(|p| p.id == EntityId::Block(m))
            .unwrap()
            .task_target
    };
    assert_eq!(target(0.5), Some([1.0, 0.0, 0.0]));
    assert_eq!(target(1.5), Some([2.0, 0.0, 0.0]));
    assert_eq!(target(20.0), Some([50.0, 0.0, 0.0]));
    // Done at 51: it stands still.
    assert_eq!(target(100.0), None);
    // Other entities never carry it.
    let frame = scene.frame(20.0);
    assert!(
        frame
            .poses
            .iter()
            .filter(|p| p.id != EntityId::Block(m))
            .all(|p| p.task_target.is_none())
    );
}
