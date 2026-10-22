//! Time series and histograms in the run report.

use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn line(run: &str) -> Model {
    model(&format!(
        r#"Model(
            name: "dd1",
            run: ({run}),
            objects: [
                (name: "src", kind: Source(interarrival: Const(2.0), first_at_zero: true)),
                (name: "q", kind: Queue()),
                (name: "p", kind: Processor(process: Const(1.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#
    ))
}

#[test]
fn deterministic_line_is_sampled_every_unit() {
    let m = line("stop_time: 10.0, sample_interval: 1.0");
    let r = &m.run_replications().unwrap()[0];
    let p = r.block("p").unwrap();
    // t = 0, 1, ..., 10 inclusive.
    assert_eq!(p.series.len(), 11);
    for (k, point) in p.series.iter().enumerate() {
        assert_eq!(point.t, k as f64);
        // Arrivals (and starts) at even t, completions at odd t.
        let busy = k % 2 == 0;
        assert_eq!(point.content, usize::from(busy), "content at t={k}");
        assert_eq!(point.state, if busy { "processing" } else { "idle" }, "state at t={k}");
    }
    assert!(r.block("q").unwrap().series.iter().all(|s| s.content == 0));
    assert!(r.blocks.iter().all(|b| b.series.len() == 11));
}

#[test]
fn sampling_starts_at_warmup_and_does_not_change_the_run() {
    let plain = line("stop_time: 10.0, warmup: 4.0");
    let sampled = line("stop_time: 10.0, warmup: 4.0, sample_interval: 2.5");
    let a = &plain.run_replications().unwrap()[0];
    let b = &sampled.run_replications().unwrap()[0];
    assert!(a.blocks.iter().all(|x| x.series.is_empty()));
    let times: Vec<f64> = b.block("p").unwrap().series.iter().map(|s| s.t).collect();
    assert_eq!(times, vec![4.0, 6.5, 9.0]);
    assert_eq!(a.events, b.events, "sampling events must not be counted");
    assert_eq!(a.metrics(), b.metrics());
}

#[test]
fn series_is_omitted_from_json_when_empty() {
    let m = line("stop_time: 10.0");
    let r = &m.run_replications().unwrap()[0];
    let json = serde_json::to_string(r).unwrap();
    assert!(!json.contains("series") && !json.contains("histogram"));
}

#[test]
fn histogram_counts_add_up_to_the_tally() {
    let m = model(
        r#"Model(
            name: "mm1",
            run: (stop_time: 2000.0, warmup: 100.0, seed: 7, histogram_bins: 12),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue()),
                (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#,
    );
    let r = &m.run_replications().unwrap()[0];
    let mut checked = 0;
    for b in &r.blocks {
        let tallies = std::iter::once(&b.staytime).chain(b.extra.values());
        for t in tallies {
            if t.count == 0 {
                assert!(t.histogram.is_none());
                continue;
            }
            let h = t.histogram.as_ref().expect("histogram present");
            assert_eq!(h.counts.len(), 12);
            assert_eq!(h.counts.iter().sum::<u64>(), t.count, "block {}", b.name);
            assert_eq!((h.min, h.max), (t.min, t.max));
            checked += 1;
        }
    }
    assert!(checked >= 4);
    assert!(r.block("saida").unwrap().extra["time_in_system"].histogram.is_some());
}

#[test]
fn invalid_output_settings_are_rejected() {
    let bad = Model::parse(
        r#"Model(name: "x", run: (stop_time: 5.0, sample_interval: 0.0, histogram_bins: 0),
            objects: [(name: "s", kind: Sink)], connections: [])"#,
    )
    .unwrap();
    assert!(bad.validate().is_err());
}
