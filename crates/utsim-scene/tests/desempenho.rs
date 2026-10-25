//! Performance targets (RNF) and a cheap scale regression test.
//!
//! The `#[ignore]` tests assert the throughput targets of `docs/desempenho.md`; run them
//! in release mode: `cargo test --release -p utsim-scene --test desempenho -- --ignored --nocapture`.
//! The targets are set at roughly a third of what a 2021 desktop CPU reaches, so a slower
//! machine still passes and only a real regression (or an accidental quadratic) fails.

use std::io::Write;
use std::time::Instant;

use utsim_model::Model;
use utsim_model::synthetic::{Shape, generate};
use utsim_scene::{JsonLines, Scene, SceneOptions, SceneSink};

struct Counter(u64);

impl Write for Counter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn run_events(model: &Model) -> (u64, f64) {
    let t0 = Instant::now();
    let report = model.run_replications().expect("run").remove(0);
    (report.events, t0.elapsed().as_secs_f64())
}

/// Not ignored: 100k items finish within a very generous bound even in a debug build, and
/// every item is accounted for. A quadratic slowdown would take minutes.
#[test]
fn hundred_thousand_items_finish_quickly() {
    let model = generate(Shape::Network, 10, 100_000).unwrap();
    let t0 = Instant::now();
    let report = model.run_replications().unwrap().remove(0);
    let elapsed = t0.elapsed().as_secs_f64();
    assert_eq!(report.blocks.last().unwrap().input, 100_000);
    assert!(elapsed < 60.0, "100 mil itens levaram {elapsed:.1} s");
}

/// Not ignored: the scene pipeline handles a long recording (replay, index, frames, stream).
#[test]
fn long_recording_builds_and_streams() {
    let model = generate(Shape::Line, 10, 10_000).unwrap();
    let t0 = Instant::now();
    let replay = model.replay(0).unwrap();
    let scene = Scene::build(
        &replay,
        &SceneOptions {
            model: Some(&model),
            ..SceneOptions::default()
        },
    );
    let mut sink = JsonLines(Counter(0));
    sink.header(&scene.header()).unwrap();
    for k in 0..100 {
        sink.frame(&scene.frame(scene.end * f64::from(k) / 100.0))
            .unwrap();
    }
    let elapsed = t0.elapsed().as_secs_f64();
    assert!(sink.0.0 > 0);
    assert!(elapsed < 60.0, "gravação longa levou {elapsed:.1} s");
}

/// RNF-P1: the kernel runs at least 1 million events per second.
#[test]
#[ignore = "timing: rode em release com --ignored"]
fn rnf_kernel_one_million_events_per_second() {
    for (shape, n) in [
        (Shape::Line, 10),
        (Shape::Line, 100),
        (Shape::Warehouse, 100),
    ] {
        let model = generate(shape, n, 100_000).unwrap();
        let (events, secs) = run_events(&model);
        let rate = events as f64 / secs;
        println!("{shape:?} {n}: {rate:.0} eventos/s");
        assert!(rate >= 1.0e6, "{shape:?} {n}: {rate:.0} eventos/s < 1e6");
    }
}

/// RNF-P2: 100k items through a 100-object line in under 10 s.
#[test]
#[ignore = "timing: rode em release com --ignored"]
fn rnf_hundred_thousand_items_hundred_objects_under_ten_seconds() {
    let model = generate(Shape::Line, 100, 100_000).unwrap();
    let (_, secs) = run_events(&model);
    println!("linha 100 x 100k itens: {secs:.2} s");
    assert!(secs < 10.0, "{secs:.2} s");
}

/// RNF-P3: recording a replay costs at most 3x the bare run; Scene::build handles at
/// least 2 million trace records per second.
#[test]
#[ignore = "timing: rode em release com --ignored"]
fn rnf_replay_and_scene_build() {
    let model = generate(Shape::Line, 100, 20_000).unwrap();
    let (_, bare) = run_events(&model);
    let t0 = Instant::now();
    let replay = model.replay(0).unwrap();
    let recorded = t0.elapsed().as_secs_f64();
    let t0 = Instant::now();
    let scene = Scene::build(
        &replay,
        &SceneOptions {
            model: Some(&model),
            ..SceneOptions::default()
        },
    );
    let build = t0.elapsed().as_secs_f64();
    let rate = replay.records.len() as f64 / build;
    println!(
        "replay {recorded:.2} s (núcleo {bare:.2} s); build {build:.2} s = {rate:.0} registros/s"
    );
    assert!(
        recorded < 3.0 * bare + 0.2,
        "replay {recorded:.2} s vs {bare:.2} s"
    );
    assert!(rate >= 2.0e6, "{rate:.0} registros/s < 2e6");
    assert!(scene.end > 0.0);
}

/// RNF-P4: one frame of a 500-object scene in under 16 ms (60 quadros/s), and the JSON
/// lines stream writes at least 30 MB/s.
#[test]
#[ignore = "timing: rode em release com --ignored"]
fn rnf_frame_and_stream() {
    let model = generate(Shape::Warehouse, 500, 20_000).unwrap();
    let replay = model.replay(0).unwrap();
    let scene = Scene::build(
        &replay,
        &SceneOptions {
            model: Some(&model),
            ..SceneOptions::default()
        },
    );
    let span = replay.records.last().map_or(1.0, |r| r.t);
    let t0 = Instant::now();
    for k in 0..20 {
        scene.frame(span / 2.0 + f64::from(k) * 0.1);
    }
    let frame_ms = t0.elapsed().as_secs_f64() / 20.0 * 1e3;
    let mut sink = JsonLines(Counter(0));
    let t0 = Instant::now();
    for k in 0..100 {
        sink.frame(&scene.frame(span * f64::from(k) / 100.0))
            .unwrap();
    }
    let mbs = sink.0.0 as f64 / 1e6 / t0.elapsed().as_secs_f64();
    println!("quadro {frame_ms:.2} ms; fluxo {mbs:.0} MB/s");
    assert!(frame_ms < 16.0, "{frame_ms:.2} ms");
    assert!(mbs >= 30.0, "{mbs:.0} MB/s");
}
