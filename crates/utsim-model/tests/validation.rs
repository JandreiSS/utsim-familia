//! End-to-end checks of the kernel against known results.

use std::path::Path;

use utsim_core::summarize;
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn assert_close(what: &str, got: f64, want: f64, rel_tol: f64) {
    let rel = (got - want).abs() / want.abs().max(1e-12);
    assert!(
        rel <= rel_tol,
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
}

#[test]
fn mm1_matches_queueing_theory() {
    let m = model(
        r#"Model(
            name: "mm1",
            run: (stop_time: 60000.0, warmup: 2000.0, seed: 2026, replications: 8),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue()),
                (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#,
    );
    let s = summarize(&m.run_replications().unwrap());
    let (lambda, mu) = (1.0, 1.25);
    let rho: f64 = lambda / mu;
    assert_close("rho", s["maquina.state.processing"].mean, rho, 0.02);
    assert_close(
        "Lq",
        s["fila.content.mean"].mean,
        rho * rho / (1.0 - rho),
        0.08,
    );
    assert_close(
        "Wq",
        s["fila.staytime.mean"].mean,
        rho / (mu - lambda),
        0.08,
    );
    assert_close(
        "W",
        s["saida.time_in_system.mean"].mean,
        1.0 / (mu - lambda),
        0.06,
    );
}

#[test]
fn deterministic_line_is_exact() {
    // One arrival every 2 starting at t=0, processing takes 1: half busy, nobody waits.
    let m = model(
        r#"Model(
            name: "dd1",
            run: (stop_time: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(2.0), first_at_zero: true)),
                (name: "q", kind: Queue()),
                (name: "p", kind: Processor(process: Const(1.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#,
    );
    let r = &m.run_replications().unwrap()[0];
    let p = r.block("p").unwrap();
    assert_eq!(r.block("out").unwrap().input, 500);
    assert_close("utilização", p.states["processing"], 0.5, 1e-9);
    assert_eq!(r.block("q").unwrap().content_mean, 0.0);
    assert_close(
        "tempo no sistema",
        r.block("out").unwrap().extra["time_in_system"].mean,
        1.0,
        1e-12,
    );
}

#[test]
fn blocking_without_buffer() {
    // P1 finishes every 1, but P2 needs 2: P1 spends half its time blocked.
    let m = model(
        r#"Model(
            name: "bloqueio",
            run: (stop_time: 10000.0, warmup: 100.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(0.1))),
                (name: "p1", kind: Processor(process: Const(1.0))),
                (name: "p2", kind: Processor(process: Const(2.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "p1"), ("p1", "p2"), ("p2", "out")],
        )"#,
    );
    let r = &m.run_replications().unwrap()[0];
    let p1 = r.block("p1").unwrap();
    assert_close("p1 processing", p1.states["processing"], 0.5, 1e-6);
    assert_close("p1 blocked", p1.states["blocked"], 0.5, 1e-6);
    assert_close(
        "p2 processing",
        r.block("p2").unwrap().states["processing"],
        1.0,
        1e-9,
    );
    assert_close(
        "vazão",
        r.block("out").unwrap().input as f64,
        9900.0 / 2.0,
        1e-3,
    );
}

#[test]
fn same_seed_same_result_and_replications_differ() {
    let m =
        Model::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mm1.ron")).unwrap();
    let a = m.build(0).run().unwrap().metrics();
    let b = m.build(0).run().unwrap().metrics();
    let c = m.build(1).run().unwrap().metrics();
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn routing_by_label_and_round_robin() {
    let m = model(
        r#"Model(
            name: "rotas",
            run: (stop_time: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0), labels: { "tipo": Const(2.0) })),
                (name: "q", kind: Queue(), output: ByLabel("tipo")),
                (name: "a", kind: Sink),
                (name: "b", kind: Sink),
                (name: "src2", kind: Source(interarrival: Const(1.0)), output: RoundRobin),
                (name: "c", kind: Sink),
                (name: "d", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "a"), ("q", "b"), ("src2", "c"), ("src2", "d")],
        )"#,
    );
    let r = &m.run_replications().unwrap()[0];
    assert_eq!(r.block("a").unwrap().input, 0);
    assert_eq!(r.block("b").unwrap().input, 1000);
    assert_eq!(r.block("c").unwrap().input, 500);
    assert_eq!(r.block("d").unwrap().input, 500);
}

#[test]
fn invalid_models_are_reported() {
    let m = Model::parse(
        r#"Model(
            name: "ruim",
            run: (stop_time: 10.0),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: -1.0))),
                (name: "src", kind: Sink),
            ],
            connections: [("src", "nada")],
        )"#,
    )
    .unwrap();
    let err = m.validate().unwrap_err().to_string();
    assert!(err.contains("nome repetido"), "{err}");
    assert!(err.contains("Exp exige mean > 0"), "{err}");
    assert!(err.contains("'nada' não existe"), "{err}");
}

#[test]
fn bundled_examples_are_valid() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with("experimento") {
            // Experiment files are checked by running them.
            utsim_model::experiment::Experiment::load(&path)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        } else if path.extension().is_some_and(|e| e == "ron") {
            let m = Model::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            m.validate()
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        }
    }
}

#[test]
fn breakdowns_follow_availability() {
    // Always-busy machine with MTBF 90 / MTTR 10: availability 0.9. Timers pause
    // during repairs, so throughput is exactly the available time / process time.
    let m = model(
        r#"Model(
            name: "quebras",
            run: (stop_time: 200000.0, warmup: 1000.0, seed: 5, replications: 4),
            objects: [
                (name: "src", kind: Source(interarrival: Const(0.5))),
                (name: "q", kind: Queue(capacity: 10)),
                (name: "p", kind: Processor(process: Const(1.0)),
                 downtime: (mtbf: Exp(mean: 90.0), mttr: Exp(mean: 10.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#,
    );
    let reports = m.run_replications().unwrap();
    let s = summarize(&reports);
    assert_close("down", s["p.state.down"].mean, 0.1, 0.05);
    assert_close("processing", s["p.state.processing"].mean, 0.9, 0.01);
    for r in &reports {
        let p = r.block("p").unwrap();
        let busy = p.states["processing"] * (r.end_time - r.stats_from);
        assert!(
            (p.output as f64 - busy).abs() <= 2.0,
            "saídas {} vs tempo produtivo {busy}",
            p.output
        );
    }
}

#[test]
fn weighted_routing_splits_flow() {
    let m = model(
        r#"Model(
            name: "pesos",
            run: (stop_time: 100000.0, seed: 3),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0)), output: Weighted([1.0, 3.0])),
                (name: "a", kind: Sink),
                (name: "b", kind: Sink),
            ],
            connections: [("src", "a"), ("src", "b")],
        )"#,
    );
    let r = &m.run_replications().unwrap()[0];
    assert_close(
        "fração para b",
        r.block("b").unwrap().input as f64 / 100000.0,
        0.75,
        0.01,
    );
}

#[test]
fn shifts_close_the_block_on_schedule() {
    // Always-busy machine, off 2 of every 10 time units: exactly 80% processing.
    let m = model(
        r#"Model(
            name: "turnos",
            run: (stop_time: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(0.25), first_at_zero: true)),
                (name: "q", kind: Queue(capacity: 10)),
                (name: "p", kind: Processor(process: Const(1.0)),
                 shifts: (period: 10.0, off: [(start: 8.0, duration: 2.0)])),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#,
    );
    let r = &m.run_replications().unwrap()[0];
    let p = r.block("p").unwrap();
    assert_close("off_shift", p.states["off_shift"], 0.2, 1e-9);
    assert_close("processing", p.states["processing"], 0.8, 1e-9);
    assert_eq!(p.output, 800);
}

#[test]
fn touching_shift_windows_stay_closed() {
    // Regression: [0,3) and [3,5) touch, and the last window ends at the period end
    // while the next cycle starts at 0. The block must stay off for 5 of every 10.
    let m = model(
        r#"Model(
            name: "janelas encostadas",
            run: (stop_time: 100.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(0.25), first_at_zero: true)),
                (name: "q", kind: Queue(capacity: 10)),
                (name: "p", kind: Processor(process: Const(1.0)),
                 shifts: (period: 10.0, off: [(start: 0.0, duration: 3.0), (start: 3.0, duration: 2.0), (start: 8.0, duration: 2.0)])),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#,
    );
    let r = &m.run_replications().unwrap()[0];
    assert_close(
        "off_shift",
        r.block("p").unwrap().states["off_shift"],
        0.7,
        1e-9,
    );
}

#[test]
fn replay_trace_is_consistent_with_the_report() {
    let m = Model::load(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/montagem_paletes.ron"),
    )
    .unwrap();
    let mut short = m.clone();
    short.run.stop_time = 500.0;
    short.run.warmup = 0.0;
    let replay = short.replay(0).unwrap();
    let direct = short.build(0).run().unwrap();
    assert_eq!(
        replay.report.metrics(),
        direct.metrics(),
        "rastrear não pode mudar o resultado"
    );
    let moved_into = |b: usize| {
        replay
            .records
            .iter()
            .filter(|r| matches!(r.event, utsim_core::TraceEvent::Moved { to, .. } if to == b))
            .count() as u64
    };
    for (i, b) in replay.report.blocks.iter().enumerate() {
        assert_eq!(moved_into(i), b.input, "entradas de {}", b.name);
    }
}

#[test]
fn forklift_carries_items_with_exact_travel_times() {
    // Queue at (0,0), machine at (6,8): 10 units apart; forklift speed 2 -> 5 per trip.
    // Item 0 (t=0): forklift already there, delivered at 5, done at 6.
    // Item n >= 1 (t=10n): forklift returns (5) and comes back (5): delivered 10n+10,
    // done 10n+11, so 11 time units in the system. By t=100 items 0..=8 are done.
    // The machine has capacity 2 on purpose: room is reserved when the transport is
    // requested, so with capacity 1 an item arriving while the previous one is still
    // on the forklift could not be dispatched yet (simuladores comerciais does the same).
    let m = model(
        r#"Model(
            name: "empilhadeira",
            run: (stop_time: 100.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(10.0), first_at_zero: true)),
                (name: "q", kind: Queue(), transport: "empilhadeira"),
                (name: "p", kind: Processor(capacity: 2, process: Const(1.0)), position: (6.0, 8.0, 0.0)),
                (name: "out", kind: Sink, position: (6.0, 8.0, 0.0)),
                (name: "empilhadeira", kind: Transporter(speed: 2.0)),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#,
    );
    let r = &m.run_replications().unwrap()[0];
    let out = r.block("out").unwrap();
    assert_eq!(out.input, 9);
    let tis = &out.extra["time_in_system"];
    assert_close("primeiro item", tis.min, 6.0, 1e-12);
    assert_close("demais itens", tis.max, 11.0, 1e-12);
    let f = r.block("empilhadeira").unwrap();
    assert_eq!(f.input, 10, "itens carregados (o 10º chega em t=100)");
    // Travelling 5 + 5 per cycle except the very first trip: busy almost all the time.
    assert!(
        f.states["travel_loaded"] > 0.45 && f.states["travel_empty"] > 0.4,
        "{:?}",
        f.states
    );
    let q = r.block("q").unwrap();
    assert_close(
        "espera média na fila",
        q.staytime.mean,
        5.0 * 9.0 / 10.0,
        1e-9,
    );
}

#[test]
fn dispatcher_shares_work_between_two_forklifts() {
    // Same layout, items every 5: one forklift (10 per cycle) could not keep up,
    // two alternate and keep the queue short.
    let text = |carrier: &str| {
        format!(
            r#"Model(
            name: "frota",
            run: (stop_time: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(5.0), first_at_zero: true)),
                (name: "q", kind: Queue(), transport: "{carrier}"),
                (name: "p", kind: Processor(capacity: 5, process: Const(1.0)), position: (6.0, 8.0, 0.0)),
                (name: "out", kind: Sink, position: (6.0, 8.0, 0.0)),
                (name: "f1", kind: Transporter(speed: 2.0)),
                (name: "f2", kind: Transporter(speed: 2.0)),
                (name: "frota", kind: Dispatcher(team: ["f1", "f2"])),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#
        )
    };
    let one = &model(&text("f1")).run_replications().unwrap()[0];
    let two = &model(&text("frota")).run_replications().unwrap()[0];
    assert!(
        one.block("q").unwrap().content_mean > 10.0,
        "um só transportador acumula fila"
    );
    assert!(
        two.block("q").unwrap().content_mean < 1.0,
        "a frota dá conta"
    );
    let (a, b) = (
        two.block("f1").unwrap().input,
        two.block("f2").unwrap().input,
    );
    assert!(a > 50 && b > 50, "as duas trabalham: {a} e {b}");
}

#[test]
fn mobile_resources_are_validated() {
    let m = Model::parse(
        r#"Model(
            name: "ruim",
            run: (stop_time: 10.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0)), transport: "out"),
                (name: "out", kind: Sink),
                (name: "f", kind: Transporter(speed: 0.0)),
                (name: "d", kind: Dispatcher(team: ["out"])),
            ],
            connections: [("src", "out"), ("src", "f")],
        )"#,
    )
    .unwrap();
    let err = m.validate().unwrap_err().to_string();
    for expected in [
        "transport 'out' precisa ser um Transporter",
        "speed deve ser finita e > 0",
        "membro 'out' da equipe precisa ser um Transporter",
        "'f' é um recurso móvel",
    ] {
        assert!(err.contains(expected), "faltou '{expected}' em:\n{err}");
    }
}
