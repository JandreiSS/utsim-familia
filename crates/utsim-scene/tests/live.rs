//! The incremental scene: frames of a run fed record by record equal the batch frames.

use std::path::Path;

use utsim_model::Model;
use utsim_scene::live::LIVE_TRANSIT;
use utsim_scene::timeline::TransitSetting;
use utsim_scene::{Frame, LiveScene, Scene, SceneOptions};

const EPS: f32 = 1e-5;

fn load(name: &str) -> Model {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../examples/{name}.ron"));
    let m = Model::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    m.validate().unwrap();
    m
}

/// The batch scene with the same options the live scene uses.
fn batch(model: &Model, replay: &utsim_model::replay::Replay) -> Scene {
    Scene::build(
        replay,
        &SceneOptions {
            transit: TransitSetting::Fixed(LIVE_TRANSIT),
            model: Some(model),
        },
    )
}

/// Same poses, same order, numbers within `EPS`.
fn assert_same(a: &Frame, b: &Frame, what: &str) {
    assert_eq!(a.time, b.time, "{what}: time");
    assert_eq!(
        a.poses.len(),
        b.poses.len(),
        "{what}: pose count at t={}",
        a.time
    );
    for (x, y) in a.poses.iter().zip(&b.poses) {
        assert_eq!(x.id, y.id, "{what}: id at t={}", a.time);
        assert_eq!(
            x.motion, y.motion,
            "{what}: motion of {:?} at t={}",
            x.id, a.time
        );
        assert_eq!(
            x.state, y.state,
            "{what}: state of {:?} at t={}",
            x.id, a.time
        );
        assert_eq!(
            x.parent, y.parent,
            "{what}: parent of {:?} at t={}",
            x.id, a.time
        );
        for k in 0..3 {
            assert!(
                (x.position[k] - y.position[k]).abs() <= EPS,
                "{what}: position[{k}] of {:?} at t={}: {} vs {}",
                x.id,
                a.time,
                x.position[k],
                y.position[k]
            );
        }
        assert!((x.yaw - y.yaw).abs() <= EPS, "{what}: yaw of {:?}", x.id);
        assert!((x.lift - y.lift).abs() <= EPS, "{what}: lift of {:?}", x.id);
        assert_eq!(
            x.fill.is_some(),
            y.fill.is_some(),
            "{what}: fill of {:?}",
            x.id
        );
    }
}

fn instants(end: f64, n: usize) -> Vec<f64> {
    (0..=n).map(|k| end * k as f64 / n as f64).collect()
}

const EXAMPLES: [&str; 3] = ["tutorial_02_linha", "agvs", "linha_com_esteiras"];

#[test]
fn live_equals_batch_after_the_whole_run() {
    for name in EXAMPLES {
        let model = load(name);
        let replay = model.replay(0).unwrap();
        let want = batch(&model, &replay);
        let mut live = LiveScene::new(&model, 0);
        // Arrives in uneven chunks, like the ticks of a paced run.
        let mut records = replay.records.iter().cloned().peekable();
        let mut chunk = 1;
        while records.peek().is_some() {
            live.push(records.by_ref().take(chunk));
            chunk = chunk * 3 % 97 + 1;
        }
        assert_eq!(live.records(), replay.records.len(), "{name}");
        assert_eq!(live.header(), want.header(), "{name}: header");
        let end = want.end;
        for t in instants(end, 400) {
            assert_same(&live.frame(t), &want.frame(t), name);
        }
    }
}

#[test]
fn live_equals_batch_of_the_prefix_at_every_step() {
    for name in EXAMPLES {
        let model = load(name);
        let full = model.replay(0).unwrap();
        let mut live = LiveScene::new(&model, 0);
        let step = (full.records.len() / 12).max(1);
        let mut at = 0;
        while at < full.records.len() {
            let to = (at + step).min(full.records.len());
            live.push(full.records[at..to].iter().cloned());
            at = to;
            let mut prefix = full.clone();
            prefix.records.truncate(at);
            let want = batch(&model, &prefix);
            let horizon = live.horizon();
            for t in instants(horizon, 40) {
                assert_same(
                    &live.frame(t),
                    &want.frame(t),
                    &format!("{name} prefix {at}"),
                );
            }
        }
    }
}

/// A frame a little behind the newest record already equals the frame of the finished
/// run: nothing the batch path looks ahead for is missing (models without carriers, whose
/// load windows are the one thing that needs the end of a state).
#[test]
fn frames_behind_the_horizon_match_the_finished_run() {
    for name in ["tutorial_02_linha", "linha_com_esteiras"] {
        let model = load(name);
        let full = model.replay(0).unwrap();
        let want = batch(&model, &full);
        let mut live = LiveScene::new(&model, 0);
        let step = (full.records.len() / 10).max(1);
        let mut at = 0;
        while at < full.records.len() {
            let to = (at + step).min(full.records.len());
            live.push(full.records[at..to].iter().cloned());
            at = to;
            let safe = live.horizon() - 2.0 * LIVE_TRANSIT;
            for t in instants(safe.max(0.0), 30) {
                assert_same(&live.frame(t), &want.frame(t), &format!("{name} lag {at}"));
            }
        }
    }
}

#[test]
fn empty_live_scene_has_the_layout_and_no_items() {
    let model = load("tutorial_02_linha");
    let mut live = LiveScene::new(&model, 0);
    let header = live.header();
    assert!(!header.scene.entities.is_empty());
    assert_eq!(header.end, model.run.stop_time);
    let frame = live.frame(0.0);
    assert!(
        frame
            .poses
            .iter()
            .all(|p| matches!(p.id, utsim_scene::EntityId::Block(_)))
    );
}
