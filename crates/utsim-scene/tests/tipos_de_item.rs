//! Item types in the scene (`docs/plano_chegadas_e_equipes_3d.md`, C3): the header lists
//! the colour and look of each type and every item pose says its type.

use std::collections::HashMap;

use utsim_core::TraceEvent;
use utsim_model::Model;
use utsim_scene::{EntityId, Scene, SceneOptions};

const TYPED: &str = r##"Model(
    format: 2,
    name: "tipos",
    run: (stop_time: 50.0),
    item_types: [
        (name: "A", color: "#3b82f6", asset: "caixa_pequena"),
        (name: "B", color: "#f59e0b"),
    ],
    objects: [
        (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true,
            item_type_mix: [("A", 1.0), ("B", 1.0)]), position: (0.0, 0.0, 0.0)),
        (name: "q", kind: Queue(), position: (3.0, 0.0, 0.0)),
        (name: "m", kind: Processor(process: Const(100.0)), position: (6.0, 0.0, 0.0)),
        (name: "out", kind: Sink, position: (9.0, 0.0, 0.0)),
    ],
    connections: [("src", "q"), ("q", "m"), ("m", "out")],
)"##;

fn scene_of(model: &Model) -> (Scene, utsim_model::replay::Replay) {
    let replay = model.replay(0).unwrap();
    let scene = Scene::build(
        &replay,
        &SceneOptions {
            model: Some(model),
            ..SceneOptions::default()
        },
    );
    (scene, replay)
}

#[test]
fn header_lists_the_types_and_items_carry_theirs() {
    let model = Model::parse(TYPED).unwrap();
    let (scene, replay) = scene_of(&model);
    let styles = &scene.header().scene.item_types;
    assert_eq!(styles.len(), 2);
    assert_eq!((styles[0].index, styles[0].name.as_str()), (1, "A"));
    assert_eq!(styles[0].color.as_deref(), Some("#3b82f6"));
    assert_eq!(styles[0].asset.as_deref(), Some("caixa_pequena"));
    assert_eq!(styles[1].asset, None);

    let truth: HashMap<u64, u32> = replay
        .records
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::ItemType { item, item_type } => Some((item, item_type)),
            _ => None,
        })
        .collect();
    let frame = scene.frame(10.0);
    let items: Vec<_> = frame
        .poses
        .iter()
        .filter_map(|p| match p.id {
            EntityId::Item(id) => Some((id, p.item_type)),
            _ => None,
        })
        .collect();
    assert!(items.len() >= 10, "{}", items.len());
    for (id, t) in items {
        assert_eq!(t, truth.get(&id).copied(), "item {id}");
    }
    let json = serde_json::to_string(&frame).unwrap();
    assert!(json.contains("\"item_type\":"));
}

#[test]
fn models_without_types_keep_the_stream_unchanged() {
    let model =
        Model::parse(&TYPED.replace(r#"item_type_mix: [("A", 1.0), ("B", 1.0)]"#, "")).unwrap();
    let (scene, _) = scene_of(&model);
    let header = serde_json::to_string(&scene.header()).unwrap();
    let frame = serde_json::to_string(&scene.frame(10.0)).unwrap();
    // The model still declares types (header) but no item has one.
    assert!(header.contains("item_types"));
    assert!(!frame.contains("item_type"));
    let plain = Model::parse(
        &TYPED
            .replace(
                r##"item_types: [
        (name: "A", color: "#3b82f6", asset: "caixa_pequena"),
        (name: "B", color: "#f59e0b"),
    ],"##,
                "",
            )
            .replace(r#"item_type_mix: [("A", 1.0), ("B", 1.0)]"#, ""),
    )
    .unwrap();
    let (scene, _) = scene_of(&plain);
    let header = serde_json::to_string(&scene.header()).unwrap();
    assert!(!header.contains("item_type"));
}
