//! Dimensões dos objetos no resultado: o operador pega a peça na ponta de saída da esteira
//! e a deixa na ponta de entrada, o tamanho de uma máquina leva o ponto de acesso (e a
//! distância percorrida) junto e a área de uma espera dá a sua capacidade.
//!
//! Operador `op` a 1 m/s, sem aceleração e com folga 0,5: ele para a 0,5 + 0,05 m da borda
//! do objeto. A esteira tem 1,1 m de largura, então o lado dela fica em y = −1,1.

use utsim_core::RunReport;
use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn tis(r: &RunReport, sink: &str) -> f64 {
    r.block(sink).unwrap().extra["time_in_system"].max
}

/// Fonte → esteira de `length` m centrada em (0, 0), virada para +x → operador leva até
/// `out` em (30, 2), com o ponto de acesso dado em (30, −1,1) (fora da sua área).
fn belt_to_sink(length: f64, start: (f64, f64), movement: &str) -> Model {
    model(&format!(
        r#"Model(
            format: 2,
            name: "ponta_de_saida",
            run: (stop_time: 1000.0, movement: {movement}),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (-15.0, 5.0, 0.0)),
                (name: "esteira", kind: Conveyor(length: {length}, speed: 1.0, item_length: 1.0),
                 transport: "op", position: (0.0, 0.0, 0.0), visual: (yaw: Some(0.0))),
                (name: "out", kind: Sink, position: (30.0, 2.0, 0.0), access: (0.0, -3.1)),
                (name: "op", kind: Transporter(speed: 1.0, clearance: 0.5), position: ({}, {}, 0.0)),
            ],
            connections: [("src", "esteira"), ("esteira", "out")],
        )"#,
        start.0, start.1
    ))
}

#[test]
fn a_long_belt_is_unloaded_at_its_output_end() {
    // 20 m of belt: the output end is at x = 10 and the operator stands beside it, half a
    // metre in, at (9,5; −1,1). From there to `out` is 20,5 m.
    let m = belt_to_sink(20.0, (9.5, -1.1), "Free");
    let ends = m.belt_ends();
    assert_eq!(ends.len(), 1);
    assert_eq!(
        (ends[0].1, ends[0].2),
        ([-10.0, 0.0, 0.0], [10.0, 0.0, 0.0])
    );
    let r = m.run_replications().unwrap().remove(0);
    // 20 s on the belt and 20,5 m carried: the distance starts at the output end.
    assert!((tis(&r, "out") - 40.5).abs() < 1e-9, "{}", tis(&r, "out"));
    // Legacy movement keeps the old behaviour, centre to centre: from the middle of the
    // belt to the middle of `out`, 30 by 2 m away.
    let legacy = belt_to_sink(20.0, (0.0, 0.0), "Legacy");
    let r = legacy.run_replications().unwrap().remove(0);
    let carried = (30.0f64 * 30.0 + 2.0 * 2.0).sqrt();
    assert!(
        (tis(&r, "out") - (20.0 + carried)).abs() < 1e-9,
        "{}",
        tis(&r, "out")
    );
}

#[test]
fn a_shorter_belt_moves_its_ends_and_changes_the_trip() {
    // Same start, 10 m of belt: 10 s on it, 5 m to walk to the output end at (4,5; −1,1)
    // and 25,5 m carried. Same total by geometry, but each piece follows the length.
    let long = belt_to_sink(20.0, (0.0, -1.1), "Free");
    let short = belt_to_sink(10.0, (0.0, -1.1), "Free");
    let rl = long.run_replications().unwrap().remove(0);
    let rs = short.run_replications().unwrap().remove(0);
    // 20 + 9,5 empty + 20,5 carried; 10 + 4,5 empty + 25,5 carried.
    assert!((tis(&rl, "out") - 50.0).abs() < 1e-9, "{}", tis(&rl, "out"));
    assert!((tis(&rs, "out") - 40.0).abs() < 1e-9, "{}", tis(&rs, "out"));
}

#[test]
fn items_are_dropped_at_the_input_end_of_a_belt() {
    // Queue at (−20, 2) with its access point at (−20; −1,1), served by the operator, who brings the item to a 20 m belt whose
    // input end is at x = −10: he stops at (−9,5; −1,1), 10,5 m from the queue.
    let m = model(
        r#"Model(
            format: 2,
            name: "ponta_de_entrada",
            run: (stop_time: 1000.0, movement: Free),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (-20.0, 5.0, 0.0)),
                (name: "q", kind: Queue(), transport: "op", position: (-20.0, 2.0, 0.0), access: (0.0, -3.1)),
                (name: "esteira", kind: Conveyor(length: 20.0, speed: 1.0, item_length: 1.0),
                 position: (0.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (30.0, 0.0, 0.0)),
                (name: "op", kind: Transporter(speed: 1.0, clearance: 0.5), position: (-20.0, -1.1, 0.0)),
            ],
            connections: [("src", "q"), ("q", "esteira"), ("esteira", "out")],
        )"#,
    );
    let r = m.run_replications().unwrap().remove(0);
    // 10,5 m carried and 20 s on the belt (to its centre it would be 20 m carried).
    assert!((tis(&r, "out") - 30.5).abs() < 1e-9, "{}", tis(&r, "out"));
}

/// Fila em (0, 0) com acesso em (0, −3), operador leva até a máquina em (10, 0).
fn to_machine(size: &str) -> Model {
    model(&format!(
        r#"Model(
            format: 2,
            name: "tamanho_da_maquina",
            run: (stop_time: 1000.0, movement: Free),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1),
                 position: (0.0, 5.0, 0.0)),
                (name: "q", kind: Queue(), transport: "op", position: (0.0, 0.0, 0.0), access: (0.0, -3.0)),
                (name: "maquina", kind: Processor(process: Const(0.0)), position: (10.0, 0.0, 0.0){size}),
                (name: "out", kind: Sink, position: (10.0, 8.0, 0.0)),
                (name: "op", kind: Transporter(speed: 1.0, clearance: 0.5), position: (0.0, -3.0, 0.0)),
            ],
            connections: [("src", "q"), ("q", "maquina"), ("maquina", "out")],
        )"#
    ))
}

#[test]
fn the_size_of_a_machine_changes_the_distance_to_it() {
    // Default 1,8 × 1,8 m: the operator stops at (10; −1,45). A 4 × 4 m machine: at
    // (10; −2,55). Same item, shorter trip, so the result changes with the size.
    let default = to_machine("");
    let big = to_machine(", size: (4.0, 4.0, 2.0)");
    assert_eq!(big.footprint(&big.objects[2]), Some([4.0, 4.0]));
    let d = |y: f64| (10.0f64.powi(2) + (y + 3.0).powi(2)).sqrt();
    let r = default.run_replications().unwrap().remove(0);
    assert!(
        (tis(&r, "out") - d(-1.45)).abs() < 1e-6,
        "{}",
        tis(&r, "out")
    );
    let r = big.run_replications().unwrap().remove(0);
    assert!(
        (tis(&r, "out") - d(-2.55)).abs() < 1e-6,
        "{}",
        tis(&r, "out")
    );
    // The height alone does not move anything on the floor.
    let tall = to_machine(", size: (1.8, 1.8, 5.0)");
    let r = tall.run_replications().unwrap().remove(0);
    assert!(
        (tis(&r, "out") - d(-1.45)).abs() < 1e-6,
        "{}",
        tis(&r, "out")
    );
}

fn waiting_area(queue: &str, extra: &str) -> Result<Model, String> {
    let m = Model::parse(&format!(
        r#"Model(
            format: 2,
            name: "area_da_espera",
            run: (stop_time: 100.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
                (name: "q", kind: Queue({queue}){extra}),
                (name: "maquina", kind: Processor(process: Const(50.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "maquina"), ("maquina", "out")],
        )"#
    ))
    .map_err(|e| e.to_string())?;
    m.validate().map_err(|e| e.to_string())?;
    Ok(m)
}

#[test]
fn the_area_of_a_queue_sets_its_capacity() {
    const AREA: &str = ", size: (4.0, 1.5, 1.0), area_per_item: 1.5";
    // 4 × 1,5 m at 1,5 m² per item: 4 items.
    let m = waiting_area("", AREA).unwrap();
    assert_eq!(m.queue_capacity(&m.objects[1]), Some(4));
    // A capacity given too caps it.
    let capped = waiting_area("capacity: Some(2)", AREA).unwrap();
    assert_eq!(capped.queue_capacity(&capped.objects[1]), Some(2));
    // The default footprint (4 × 1,4 m) at 1,4 m² per item: 4 items.
    let default = waiting_area("", ", area_per_item: 1.4").unwrap();
    assert_eq!(default.queue_capacity(&default.objects[1]), Some(4));
    // The run follows: the queue never holds more than its area allows, while without
    // a limit it keeps growing behind the slow machine.
    let r = m.run_replications().unwrap().remove(0);
    assert_eq!(r.block("q").unwrap().content_max, 4.0);
    let free = waiting_area("", "").unwrap();
    let r = free.run_replications().unwrap().remove(0);
    assert!(r.block("q").unwrap().content_max > 10.0);
    // No room for one item: a model error.
    let e = waiting_area("", ", size: (1.0, 1.0, 1.0), area_per_item: 2.0")
        .err()
        .unwrap_or_default();
    assert!(e.contains("nenhum item cabe"), "{e}");
    // Only for queues.
    let e = waiting_area("", "")
        .ok()
        .map(|mut m| {
            m.objects[2].area_per_item = Some(1.0);
            m.validate()
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    assert!(e.contains("area_per_item vale só para Queue"), "{e}");
}

#[test]
fn belts_end_to_end_keep_their_ends_apart() {
    // A ends at x = 5, where B starts, and B ends at the centre of `out`: each end stays
    // its own (a hair apart), so a drop on B does not stop beside A.
    let m = model(
        r#"Model(
            format: 2,
            name: "esteiras_em_serie",
            run: (stop_time: 100.0, movement: Free),
            objects: [
                (name: "src", kind: Source(interarrival: Const(10.0)), position: (-5.0, 5.0, 0.0)),
                (name: "a", kind: Conveyor(length: 10.0, speed: 1.0, item_length: 1.0),
                 position: (0.0, 0.0, 0.0), visual: (yaw: Some(0.0))),
                (name: "b", kind: Conveyor(length: 10.0, speed: 1.0, item_length: 1.0),
                 position: (10.0, 0.0, 0.0), visual: (yaw: Some(0.0))),
                (name: "out", kind: Sink, position: (15.0, 0.0, 0.0)),
            ],
            connections: [("src", "a"), ("a", "b"), ("b", "out")],
        )"#,
    );
    let ends = m.belt_ends();
    let (a_out, b_in, b_out) = (ends[0].2, ends[1].1, ends[1].2);
    assert_ne!(a_out, b_in);
    assert!((a_out[0] - b_in[0]).abs() < 1e-12 && a_out[1] == b_in[1]);
    assert_ne!(b_out, m.objects[3].position);
    assert!((b_out[0] - 15.0).abs() < 1e-12);
}
