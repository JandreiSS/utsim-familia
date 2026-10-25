//! Throughput benchmark (plain timing, no framework): `cargo bench -p utsim-scene`.
//!
//! Prints Markdown tables with the throughput of every stage of the pipeline on the
//! synthetic models of `utsim_model::synthetic`:
//! kernel run, replay recording, `Scene::build`, one frame, and the JSON-lines stream.
//!
//! Arguments (after `--`): `--itens N` (default 100000), `--objetos a,b,c`
//! (default 10,100,500), `--quadros N` (frames of the stream, default 300).

use std::io::Write;
use std::time::Instant;

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

/// Median of up to three timings of `f`, in seconds, with the result of the last run.
/// A run longer than a second is not repeated, and the previous result is dropped before
/// the next run, so two huge traces never live at once.
fn timed<T>(mut f: impl FnMut() -> T) -> (f64, T) {
    let mut times = Vec::new();
    let mut last = None;
    for _ in 0..3 {
        drop(last.take());
        let t0 = Instant::now();
        let value = f();
        let elapsed = t0.elapsed().as_secs_f64();
        times.push(elapsed);
        last = Some(value);
        if elapsed > 1.0 {
            break;
        }
    }
    times.sort_by(f64::total_cmp);
    (times[times.len() / 2], last.expect("ran"))
}

fn main() {
    let mut items = 100_000u64;
    let mut sizes = vec![10usize, 100, 500];
    let mut frames = 300u64;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--itens" => items = args.next().and_then(|v| v.parse().ok()).unwrap_or(items),
            "--quadros" => frames = args.next().and_then(|v| v.parse().ok()).unwrap_or(frames),
            "--objetos" => {
                if let Some(list) = args.next() {
                    sizes = list
                        .split(',')
                        .filter_map(|v| v.trim().parse().ok())
                        .collect();
                }
            }
            _ => {}
        }
    }

    println!(
        "| tipo | objetos | itens | eventos | núcleo (s) | eventos/s | itens/s | replay (s) | registros | Scene::build (s) | 1 quadro (ms) | JSONL {frames} quadros (s) | JSONL MB |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (label, shape) in [
        ("linha", Shape::Line),
        ("rede", Shape::Network),
        ("armazém", Shape::Warehouse),
    ] {
        for &n in &sizes {
            let model = generate(shape, n, items).expect("model");
            model.validate().expect("valid");

            let (kernel, report) = timed(|| model.run_replications().expect("run").remove(0));
            let events = report.events;

            let (replay_s, replay) = timed(|| model.replay(0).expect("replay"));
            let records = replay.records.len();

            let options = SceneOptions {
                model: Some(&model),
                ..SceneOptions::default()
            };
            let (build, scene) = timed(|| Scene::build(&replay, &options));

            // The run ends when the last item leaves, well before the generous stop time.
            let span = replay.records.last().map_or(1.0, |r| r.t);
            let mid = span / 2.0;
            let (frame, poses) = timed(|| {
                let mut poses = 0;
                for k in 0..20 {
                    poses = scene.frame(mid + k as f64 * 0.1).poses.len();
                }
                poses
            });
            let frame_ms = frame / 20.0 * 1e3;

            let mut bytes = 0;
            let (stream, _) = timed(|| {
                let mut sink = JsonLines(Counter(0));
                sink.header(&scene.header()).expect("header");
                for k in 0..frames {
                    let t = span * k as f64 / frames as f64;
                    sink.frame(&scene.frame(t)).expect("frame");
                }
                bytes = sink.0.0;
            });
            println!(
                "| {label} | {n} | {items} | {events} | {kernel:.3} | {:.0} | {:.0} | {replay_s:.3} | {records} | {build:.3} | {frame_ms:.3} ({poses} poses) | {stream:.3} | {:.1} |",
                events as f64 / kernel,
                items as f64 / kernel,
                bytes as f64 / 1e6,
            );
        }
    }
}
