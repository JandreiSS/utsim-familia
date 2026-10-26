//! Travel time on direct connections (`transfer`) and the non-fatal model warnings.
//!
//! The hand-checked cases use a source at the origin and a sink some metres away, so
//! the arrival time is exactly `delay + distance / speed`.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use utsim_core::{RunReport, TraceEvent};
use utsim_model::experiment::apply_patches;
use utsim_model::{Model, ModelError};

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn run(text: &str) -> RunReport {
    model(text).run_replications().unwrap().remove(0)
}

fn close(what: &str, got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: obtido {got:.9}, esperado {want:.9}"
    );
}

fn time_in_system(r: &RunReport) -> f64 {
    r.block("out").unwrap().extra["time_in_system"].mean
}

/// One item from a source at the origin to a sink at `(dx, 0, 0)`.
fn single(transfer: &str, dx: f64, extra: &str) -> String {
    format!(
        r#"Model(
            name: "uma peça",
            run: (stop_time: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 {transfer}),
                (name: "out", kind: Sink, position: ({dx:?}, 0.0, 0.0)),
            ],
            connections: [("src", "out")],
            {extra}
        )"#
    )
}

fn trace_of(m: &Model) -> Vec<utsim_core::TraceRecord> {
    let mut sim = m.build(0);
    sim.enable_trace();
    m.drive(&mut sim).unwrap();
    sim.take_trace()
}

// ------------------------------------------------------------------ hand-checked

#[test]
fn crossing_20_m_at_1_m_per_s_takes_20_s() {
    let m = model(&single("transfer: (speed: 1.0)", 20.0, ""));
    let r = m.run_replications().unwrap().remove(0);
    close("tempo no sistema", time_in_system(&r), 20.0, 1e-12);
    // The trace has the departure at t = 0 and the arrival at t = 20.
    let trace = trace_of(&m);
    let transit = trace
        .iter()
        .find(|rec| matches!(rec.event, TraceEvent::Transit { .. }))
        .expect("saída registrada");
    assert_eq!(transit.t, 0.0);
    let TraceEvent::Transit {
        duration,
        delay,
        ref path,
        ..
    } = transit.event
    else {
        unreachable!()
    };
    close("duração", duration, 20.0, 1e-12);
    assert_eq!(delay, 0.0);
    assert_eq!(path.len(), 2);
    let moved = trace
        .iter()
        .find(|rec| matches!(rec.event, TraceEvent::Moved { .. }))
        .expect("chegada registrada");
    close("chegada", moved.t, 20.0, 1e-12);
}

#[test]
fn fixed_delay_adds_to_the_travel() {
    // 20 m at 2 m/s = 10 s, plus 5 s of fixed delay.
    let r = run(&single("transfer: (speed: 2.0, delay: 5.0)", 20.0, ""));
    close("tempo no sistema", time_in_system(&r), 15.0, 1e-12);
}

#[test]
fn zero_distance_and_zero_delay_is_still_instantaneous() {
    let r = run(&single("transfer: (speed: 1.0)", 0.0, ""));
    close("tempo no sistema", time_in_system(&r), 0.0, 1e-12);
}

#[test]
fn run_default_applies_and_object_overrides_it() {
    let r = run(&single("", 20.0, "").replace(
        "run: (stop_time: 1000.0)",
        "run: (stop_time: 1000.0, transfer: (speed: 4.0))",
    ));
    close("padrão global", time_in_system(&r), 5.0, 1e-12);
    let r = run(&single("transfer: (speed: 2.0)", 20.0, "").replace(
        "run: (stop_time: 1000.0)",
        "run: (stop_time: 1000.0, transfer: (speed: 4.0))",
    ));
    close("o objeto vence", time_in_system(&r), 10.0, 1e-12);
}

#[test]
fn only_the_origin_object_travels() {
    // The queue forwards instantly (no transfer of its own), so only src -> fila takes time.
    let text = r#"Model(
        name: "duas ligações",
        run: (stop_time: 1000.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
             transfer: (speed: 1.0)),
            (name: "fila", kind: Queue(), position: (10.0, 0.0, 0.0)),
            (name: "out", kind: Sink, position: (50.0, 0.0, 0.0)),
        ],
        connections: [("src", "fila"), ("fila", "out")],
    )"#;
    close("tempo", time_in_system(&run(text)), 10.0, 1e-12);
}

// ------------------------------------------------------------------- capacity

#[test]
fn destination_capacity_counts_items_in_transit() {
    // Items every 1 s travel 10 s to a queue of capacity 2 behind a very slow machine.
    // Content plus travellers can never pass 2.
    let m = model(
        r#"Model(
            name: "capacidade",
            run: (stop_time: 300.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true),
                 transfer: (speed: 1.0)),
                (name: "q", kind: Queue(capacity: 2), position: (10.0, 0.0, 0.0)),
                (name: "m", kind: Processor(process: Const(100.0)), position: (10.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (10.0, 0.0, 0.0)),
            ],
            connections: [("src", "q"), ("q", "m"), ("m", "out")],
        )"#,
    );
    let r = m.run_replications().unwrap().remove(0);
    let q = r.block("q").unwrap();
    assert!(q.content_max <= 2.0, "fila passou da capacidade: {q:?}");

    let (qi, mut inside, mut flying) = (1usize, 0i64, 0i64);
    let mut peak = 0;
    let mut transits = 0;
    for rec in trace_of(&m) {
        match rec.event {
            TraceEvent::Transit { to, .. } if to == qi => {
                flying += 1;
                transits += 1;
            }
            TraceEvent::Moved { from, to, .. } => {
                if to == qi {
                    flying -= 1;
                    inside += 1;
                }
                if from == qi {
                    inside -= 1;
                }
            }
            _ => {}
        }
        peak = peak.max(inside + flying);
        assert!(inside + flying <= 2, "t={}: {inside} + {flying}", rec.t);
    }
    assert_eq!(peak, 2, "a fila deveria chegar a encher");
    assert!(transits >= 3, "a fonte tinha de continuar tentando");
}

// ---------------------------------------------------------------- Little's law

#[test]
fn littles_law_includes_the_transit_time() {
    // Items every 5 s, 20 m at 1 m/s to a queue that hands over to a 3 s machine.
    // W = 20 + 3 = 23 s, throughput 0.2/s, so L = 4.6: 0.6 in the machine, 4 on the way.
    let m = model(
        r#"Model(
            name: "little",
            run: (stop_time: 10000.0, warmup: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(5.0), first_at_zero: true),
                 transfer: (speed: 1.0)),
                (name: "fila", kind: Queue(), position: (20.0, 0.0, 0.0)),
                (name: "m", kind: Processor(process: Const(3.0)), position: (20.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (20.0, 0.0, 0.0)),
            ],
            connections: [("src", "fila"), ("fila", "m"), ("m", "out")],
        )"#,
    );
    let r = m.run_replications().unwrap().remove(0);
    let (warmup, stop) = (1000.0, 10000.0);
    let span = stop - warmup;
    let w = time_in_system(&r);
    close("W", w, 23.0, 1e-9);
    let lambda = r.block("out").unwrap().input as f64 / span;
    close("vazão", lambda, 0.2, 2.0 / span);

    // Blocks hold 0.6 on average; the rest is in transit (integral over the trace).
    let blocks: f64 = ["src", "fila", "m", "out"]
        .iter()
        .map(|n| r.block(n).unwrap().content_mean)
        .sum();
    let mut flying = 0.0;
    let (mut last, mut count) = (warmup, 0i64);
    let mut travelling = HashSet::new();
    // Count what was already travelling at the end of the warm-up.
    for rec in trace_of(&m) {
        let t = rec.t;
        let delta = match rec.event {
            TraceEvent::Transit { item, .. } => {
                travelling.insert(item);
                1
            }
            TraceEvent::Moved { item, .. } if travelling.remove(&item) => -1,
            _ => continue,
        };
        if t > warmup {
            flying += count as f64 * (t.min(stop) - last);
            last = t.min(stop);
        }
        count += delta;
    }
    flying += count as f64 * (stop - last);
    let in_transit = flying / span;
    close("itens a caminho", in_transit, 4.0, 0.01);
    close("L total", blocks + in_transit, lambda * w, 0.01);
    close("L teórico", blocks + in_transit, 4.6, 0.02);
}

// ------------------------------------------------------------------- network

const NETWORK: &str = r#"
    network: Some((
        nodes: [
            (name: "A", position: (0.0, 0.0, 0.0)),
            (name: "B", position: (30.0, 0.0, 0.0)),
            (name: "C", position: (30.0, 40.0, 0.0)),
        ],
        edges: [(from: "A", to: "B"), (from: "B", to: "C")],
    )),"#;

#[test]
fn network_path_uses_the_network_length() {
    let mut text = single("transfer: (speed: 1.0, path: Network)", 0.0, NETWORK);
    text = text.replace(
        "(name: \"out\", kind: Sink, position: (0.0, 0.0, 0.0))",
        "(name: \"out\", kind: Sink, position: (30.0, 40.0, 0.0))",
    );
    let m = model(&text);
    // Along the network: 30 + 40 = 70 m, not the 50 m straight line.
    let r = m.run_replications().unwrap().remove(0);
    close("pela rede", time_in_system(&r), 70.0, 1e-9);
    let path_points = trace_of(&m)
        .iter()
        .find_map(|rec| match &rec.event {
            TraceEvent::Transit { path, .. } => Some(path.len()),
            _ => None,
        })
        .unwrap();
    assert!(path_points >= 3, "o caminho deveria passar pelo nó B");

    let straight = text.replace("path: Network", "path: Straight");
    close("em linha reta", time_in_system(&run(&straight)), 50.0, 1e-9);
}

// ---------------------------------------------------------------- validation

fn invalid(text: &str) -> String {
    match Model::parse(text).unwrap().validate() {
        Err(ModelError::Invalid(errors)) => errors.join("\n"),
        other => panic!("esperava erro de validação, veio {other:?}"),
    }
}

#[test]
fn bad_transfer_definitions_are_rejected() {
    assert!(invalid(&single("transfer: (speed: 0.0)", 5.0, "")).contains("transfer.speed"));
    assert!(
        invalid(&single("transfer: (speed: 1.0, delay: -1.0)", 5.0, "")).contains("transfer.delay")
    );
    assert!(invalid(&single("transfer: (speed: 1.0, path: Network)", 5.0, "")).contains("Network"));
    let both = single("transfer: (speed: 1.0), transport: \"frota\"", 5.0, "").replace(
        "(name: \"out\"",
        "(name: \"frota\", kind: Transporter(speed: 1.0)),\n (name: \"out\"",
    );
    assert!(invalid(&both).contains("transport"));
}

#[test]
fn run_default_skips_conveyors_and_what_feeds_them() {
    let text = r#"Model(
        name: "esteira",
        run: (stop_time: 100.0, transfer: (speed: 1.0)),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1.0)), position: (0.0, 0.0, 0.0)),
            (name: "belt", kind: Conveyor(length: 20.0, speed: 1.0, item_length: 1.0), position: (40.0, 0.0, 0.0)),
            (name: "q", kind: Queue(), position: (80.0, 0.0, 0.0)),
            (name: "out", kind: Sink, position: (90.0, 0.0, 0.0)),
        ],
        connections: [("src", "belt"), ("belt", "q"), ("q", "out")],
    )"#;
    let m = model(text);
    let by_name = |n: &str| m.objects.iter().find(|o| o.name == n).unwrap();
    assert!(m.effective_transfer(by_name("src")).is_none());
    assert!(m.effective_transfer(by_name("belt")).is_none());
    assert!(m.effective_transfer(by_name("q")).is_some());
}

#[test]
fn model_without_transfer_does_not_serialize_the_field() {
    let m = model(&single("", 5.0, ""));
    let text = ron::ser::to_string_pretty(&m, ron::ser::PrettyConfig::new()).unwrap();
    assert!(!text.contains("transfer"));
    assert_eq!(Model::parse(&text).unwrap(), m);
    let with = model(&single("transfer: (speed: 2.0, delay: 1.0)", 5.0, ""));
    let text = ron::ser::to_string_pretty(&with, ron::ser::PrettyConfig::new()).unwrap();
    assert_eq!(Model::parse(&text).unwrap(), with);
}

// ------------------------------------------------------------------ experiments

#[test]
fn experiment_keys_vary_the_speed() {
    let m = model(&single("transfer: (speed: 1.0)", 20.0, ""));
    let set = |k: &str, v: &str| BTreeMap::from([(k.to_string(), v.to_string())]);
    let fast = apply_patches(&m, &set("src.transfer.speed", "4.0")).unwrap();
    close(
        "src.transfer.speed",
        time_in_system(&fast.run_replications().unwrap()[0]),
        5.0,
        1e-12,
    );
    // The run default needs an existing definition to vary.
    assert!(apply_patches(&m, &set("run.transfer.speed", "2.0")).is_err());
    let with_default = {
        let mut d = m.clone();
        d.objects[0].transfer = None;
        d.run.transfer = Some(utsim_core::TransferDef {
            speed: 1.0,
            delay: 0.0,
            path: Default::default(),
        });
        d
    };
    let fast = apply_patches(&with_default, &set("run.transfer.speed", "2.0")).unwrap();
    close(
        "run.transfer.speed",
        time_in_system(&fast.run_replications().unwrap()[0]),
        10.0,
        1e-12,
    );
    let delayed = apply_patches(&with_default, &set("run.transfer.delay", "3.0")).unwrap();
    close(
        "run.transfer.delay",
        time_in_system(&delayed.run_replications().unwrap()[0]),
        23.0,
        1e-12,
    );
    let whole = apply_patches(&m, &set("src.transfer", "(speed: 10.0)")).unwrap();
    close(
        "src.transfer",
        time_in_system(&whole.run_replications().unwrap()[0]),
        2.0,
        1e-12,
    );
}

// ------------------------------------------------------------------- warnings

fn warnings(text: &str) -> Vec<String> {
    model(text).warnings()
}

#[test]
fn long_instant_link_warns_and_transfer_silences_it() {
    let w = warnings(&single("", 25.0, ""));
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("src → out") && w[0].contains("25 m") && w[0].contains("instantânea"));
    assert!(warnings(&single("transfer: (speed: 1.0)", 25.0, "")).is_empty());
    // Short links are fine, and so is a long one carried by a transporter.
    assert!(warnings(&single("", 5.0, "")).is_empty());
    let carried = single("transport: \"frota\"", 25.0, "").replace(
        "(name: \"out\"",
        "(name: \"frota\", kind: Transporter(speed: 1.0), position: (1.0, 0.0, 0.0)),\n (name: \"out\"",
    );
    assert!(warnings(&carried).is_empty(), "{:?}", warnings(&carried));
}

#[test]
fn mobile_resources_with_everything_at_the_origin_warn() {
    let text = r#"Model(
        name: "origem",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(10.0))),
            (name: "q", kind: Queue(), transport: "frota"),
            (name: "out", kind: Sink),
            (name: "frota", kind: Transporter(speed: 1.0)),
        ],
        connections: [("src", "q"), ("q", "out")],
    )"#;
    let w = warnings(text);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("origem") && w[0].contains("distâncias serão zero"));
}

#[test]
fn transporter_serving_objects_in_the_same_place_warns() {
    let text = r#"Model(
        name: "mesmo lugar",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(10.0)), position: (0.0, 0.0, 0.0)),
            (name: "q", kind: Queue(), transport: "frota", position: (5.0, 5.0, 0.0)),
            (name: "out", kind: Sink, position: (5.0, 5.0, 0.0)),
            (name: "frota", kind: Transporter(speed: 1.0), position: (1.0, 1.0, 0.0)),
        ],
        connections: [("src", "q"), ("q", "out")],
    )"#;
    let w = warnings(text);
    assert!(
        w.iter()
            .any(|x| x.contains("frota") && x.contains("mesma posição")),
        "{w:?}"
    );
}

#[test]
fn operator_team_at_one_spot_warns() {
    let text = r#"Model(
        name: "equipe",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(10.0)), position: (0.0, 0.0, 0.0)),
            (name: "m1", kind: Processor(process: Const(1.0), operators: Some((team: "ana"))), position: (3.0, 0.0, 0.0)),
            (name: "m2", kind: Processor(process: Const(1.0), operators: Some((team: "ana"))), position: (3.0, 0.0, 0.0)),
            (name: "out", kind: Sink, position: (6.0, 0.0, 0.0)),
            (name: "ana", kind: Transporter(speed: 1.0), position: (3.0, 2.0, 0.0)),
        ],
        connections: [("src", "m1"), ("src", "m2"), ("m1", "out"), ("m2", "out")],
    )"#;
    let w = warnings(text);
    assert!(
        w.iter()
            .any(|x| x.contains("ana") && x.contains("m1, m2") && x.contains("mesma posição")),
        "{w:?}"
    );
}

#[test]
fn object_far_from_the_network_warns() {
    let text = r#"Model(
        name: "longe da rede",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(10.0)), position: (0.0, 0.0, 0.0)),
            (name: "q", kind: Queue(), transport: "agv", position: (1.0, 0.0, 0.0)),
            (name: "out", kind: Sink, position: (200.0, 0.0, 0.0)),
            (name: "agv", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0)),
        ],
        connections: [("src", "q"), ("q", "out")],
        network: Some((
            nodes: [(name: "A", position: (0.0, 0.0, 0.0)), (name: "B", position: (20.0, 0.0, 0.0))],
            edges: [(from: "A", to: "B")],
        )),
    )"#;
    let w = warnings(text);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("'out'") && w[0].contains("180 m") && w[0].contains("rede"));
}

#[test]
fn shipped_examples_only_warn_where_expected() {
    // Only the operator cell has links over 10 m that are still instantaneous; the
    // operators walk those distances but the items do not. Every other example is clean.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut seen = HashSet::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        if path.extension().is_none_or(|e| e != "ron")
            || stem.starts_with("experimento")
            || stem.starts_with("otimizacao")
        {
            continue;
        }
        let w = Model::load(&path).unwrap().warnings();
        if stem == "celula_com_operadores" {
            assert_eq!(w.len(), 4, "{w:?}");
            assert!(w.iter().all(|x| x.contains("instantânea")));
        } else {
            assert!(w.is_empty(), "{stem}: {w:?}");
        }
        seen.insert(stem);
    }
    assert!(seen.contains("celula_com_operadores") && seen.contains("mm1"));
}
