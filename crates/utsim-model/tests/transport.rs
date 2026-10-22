//! Transporters and dispatchers: exact hand-computed trips, dispatch rules,
//! capacity, interruptions, priorities and a queueing-theory sanity check.
//!
//! Unless said otherwise the layout is `src -> q -> out` with the queue `q` served by
//! a forklift and the sink `out` as destination. Items are created at `q`.

use std::collections::HashMap;

use utsim_core::task::{MSG_ARRIVED, MSG_RELEASE};
use utsim_core::{
    Behavior, BlockId, Ctx, Item, OutputRule, RequestId, RunConfig, RunReport, Simulation,
    TaskKind, TraceEvent,
};
use utsim_model::Model;
use utsim_objects::Transporter;

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

/// `n` items every `gap` from t = 0 into a queue at `q_pos` carried to a sink at
/// `out_pos`. `carrier` is the extra objects (transporters, dispatcher) as RON.
fn line(n: u32, gap: f64, q_pos: &str, out_pos: &str, stop: f64, carrier: &str) -> String {
    format!(
        r#"Model(
            name: "transporte",
            run: (stop_time: {stop}),
            objects: [
                (name: "src", kind: Source(interarrival: Const({gap}), first_at_zero: true, max_arrivals: {n})),
                (name: "q", kind: Queue(), transport: "frota", position: {q_pos}),
                (name: "out", kind: Sink, position: {out_pos}),
                {carrier}
            ],
            connections: [("src", "q"), ("q", "out")],
        )"#
    )
}

fn tis(r: &RunReport) -> (f64, f64, f64) {
    let t = &r.block("out").unwrap().extra["time_in_system"];
    (t.min, t.max, t.mean)
}

// ---------------------------------------------------------------- speed profile

#[test]
fn trapezoidal_profile_empty_and_loaded() {
    // Forklift at the origin, q at (3,4,0), sink at (9,12,0); speed 2, a = d = 1.
    // Empty leg: length 5. Ramps need v²/2·(1/a+1/d) = 4 <= 5, so the profile is
    // trapezoidal: 5/2 + 2/2·(1+1) = 2.5 + 2 = 4.5.
    // Loaded leg: length |(6,8)| = 10: 10/2 + 2 = 7. No load/unload time.
    // The item (created at t = 0) reaches the sink at 4.5 + 7 = 11.5.
    let r = run(&line(
        1,
        1000.0,
        "(3.0, 4.0, 0.0)",
        "(9.0, 12.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0, acceleration: 1.0, deceleration: 1.0))"#,
    ));
    let (min, max, _) = tis(&r);
    close("tempo no sistema", min, 11.5, 1e-9);
    close("tempo no sistema", max, 11.5, 1e-9);
    let f = r.block("frota").unwrap();
    close("travel_empty", f.states["travel_empty"], 4.5 / 100.0, 1e-9);
    close(
        "travel_loaded",
        f.states["travel_loaded"],
        7.0 / 100.0,
        1e-9,
    );
}

#[test]
fn asymmetric_ramps_with_load_and_unload() {
    // Everything starts at the origin, sink at (6,8,0): length 10, speed 2, a = 1,
    // d = 2. Ramps need 4/2·(1 + 1/2) = 3 <= 10: 10/2 + 2/2·1.5 = 6.5.
    // Load 1, unload 0.5: delivered at 1 + 6.5 + 0.5 = 8.
    let r = run(&line(
        1,
        1000.0,
        "(0.0, 0.0, 0.0)",
        "(6.0, 8.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0, acceleration: 1.0, deceleration: 2.0,
            load_time: Const(1.0), unload_time: Const(0.5)))"#,
    ));
    close("tempo no sistema", tis(&r).0, 8.0, 1e-9);
    let f = r.block("frota").unwrap();
    close("loading", f.states["loading"], 0.01, 1e-9);
    close("unloading", f.states["unloading"], 0.005, 1e-9);
}

#[test]
fn short_trip_has_a_triangular_profile() {
    // 2 m with a = d = 1 never reaches speed 2: peak speed √2, time 2√2.
    let r = run(&line(
        1,
        1000.0,
        "(0.0, 0.0, 0.0)",
        "(2.0, 0.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0, acceleration: 1.0, deceleration: 1.0))"#,
    ));
    close("tempo no sistema", tis(&r).0, 2.0 * 2f64.sqrt(), 1e-9);
}

// --------------------------------------------------------------------- capacity

fn capacity_model(capacity: usize, via_dispatcher: bool) -> String {
    // Items at t = 0, 1, 2. Layout: q and the forklift at the origin, sink at (6,8,0)
    // (10 away, speed 2 -> 5 per leg). Load 1 and unload 1 per item.
    let carrier = if via_dispatcher {
        format!(
            r#"(name: "f", kind: Transporter(speed: 2.0, capacity: {capacity}, load_time: Const(1.0), unload_time: Const(1.0))),
               (name: "frota", kind: Dispatcher(team: ["f"])),"#
        )
    } else {
        format!(
            r#"(name: "frota", kind: Transporter(speed: 2.0, capacity: {capacity}, load_time: Const(1.0), unload_time: Const(1.0))),"#
        )
    };
    line(
        3,
        1.0,
        "(0.0, 0.0, 0.0)",
        "(6.0, 8.0, 0.0)",
        100.0,
        &carrier,
    )
}

#[test]
fn capacity_one_serves_items_one_trip_each() {
    // A (t=0): load 1, travel 5, unload 1: delivered at 7 (time in system 7).
    // The forklift is at the sink at t = 7; B and C wait in q.
    // B: back to q 5 (t=12), load (13), travel (18), unload (19): 19 - 1 = 18.
    // C: back to q 5 (t=24), load (25), travel (30), unload (31): 31 - 2 = 29.
    let r = run(&capacity_model(1, false));
    let (min, max, mean) = tis(&r);
    close("min", min, 7.0, 1e-9);
    close("max", max, 29.0, 1e-9);
    close("média", mean, (7.0 + 18.0 + 29.0) / 3.0, 1e-9);
}

#[test]
fn capacity_two_collects_two_items_from_the_same_queue_in_one_trip() {
    // A as above (delivered at 7). At t = 7 both B and C are queued at q, so one
    // trip takes both: back to q 5 (t=12), load B (13), load C (14), travel 5 (19),
    // unload B (20) -> 20 - 1 = 19, no travel (same sink), unload C (21) -> 21 - 2 = 19.
    let r = run(&capacity_model(2, false));
    let (min, max, mean) = tis(&r);
    close("min", min, 7.0, 1e-9);
    close("max", max, 19.0, 1e-9);
    close("média", mean, (7.0 + 19.0 + 19.0) / 3.0, 1e-9);
    assert_eq!(r.block("frota").unwrap().input, 3);
}

#[test]
fn dispatcher_groups_waiting_tasks_for_a_big_forklift() {
    // Same scenario through a dispatcher: A goes to the (free) forklift at once;
    // B and C wait in the dispatcher. When the forklift reports idle (capacity 2),
    // the dispatcher forwards both together, so the result is identical to the direct case.
    let r = run(&capacity_model(2, true));
    let (min, max, mean) = tis(&r);
    close("min", min, 7.0, 1e-9);
    close("max", max, 19.0, 1e-9);
    close("média", mean, 15.0, 1e-9);
}

// ----------------------------------------------------------------- dispatch rules

#[test]
fn round_robin_alternates_between_free_members() {
    // 9 items 100 apart, each trip takes 10, so both forklifts are free for every task.
    // FirstAvailable would give everything to f1; RoundRobin gives f1 the 1st, 3rd, ...
    // 9th (5 items) and f2 the 2nd, ..., 8th (4 items).
    let team = |rule: &str| {
        line(
            9,
            100.0,
            "(0.0, 0.0, 0.0)",
            "(6.0, 8.0, 0.0)",
            1000.0,
            &format!(
                r#"(name: "f1", kind: Transporter(speed: 2.0)),
                   (name: "f2", kind: Transporter(speed: 2.0)),
                   (name: "frota", kind: Dispatcher(team: ["f1", "f2"], rule: {rule})),"#
            ),
        )
    };
    let rr = run(&team("RoundRobin"));
    assert_eq!(rr.block("f1").unwrap().input, 5);
    assert_eq!(rr.block("f2").unwrap().input, 4);
    let fa = run(&team("FirstAvailable"));
    assert_eq!(fa.block("f1").unwrap().input, 9);
    assert_eq!(fa.block("f2").unwrap().input, 0);
}

fn nearest_model(order: &str, q2: f64) -> String {
    // Two forklifts: fa at x = 0, fb at x = 20 (speed 1). Sink `out` at (10,0,0).
    // q1 at (18,0,0) gets an item at t = 0, q2 at (q2,0,0) one at t = 50.
    format!(
        r#"Model(
            name: "mais proximo",
            run: (stop_time: 200.0),
            objects: [
                (name: "src1", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1)),
                (name: "src2", kind: Source(interarrival: Const(50.0), max_arrivals: 1)),
                (name: "q1", kind: Queue(), transport: "frota", position: (18.0, 0.0, 0.0)),
                (name: "q2", kind: Queue(), transport: "frota", position: ({q2:?}, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (10.0, 0.0, 0.0)),
                (name: "fa", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0)),
                (name: "fb", kind: Transporter(speed: 1.0), position: (20.0, 0.0, 0.0)),
                (name: "frota", kind: Dispatcher(team: {order}, rule: Nearest)),
            ],
            connections: [("src1", "q1"), ("src2", "q2"), ("q1", "out"), ("q2", "out")],
        )"#
    )
}

#[test]
fn nearest_picks_the_closest_free_member_and_tracks_positions() {
    // t = 0, item at q1 (18,0): fb is 2 away, fa 18 -> fb. fb: empty 2, loaded to
    // (10,0): 8 -> delivered at 10 (time in system 10). fb now rests at (10,0,0).
    // t = 50, item at q2 (8,0): fa is 8 away; fb is 2 away from where it STOPPED
    // (it would be 12 away if the dispatcher still believed it was at x = 20) -> fb.
    // fb: empty 2, loaded 2 -> time in system 4.
    let r = run(&nearest_model(r#"["fa", "fb"]"#, 8.0));
    assert_eq!(r.block("fb").unwrap().input, 2);
    assert_eq!(r.block("fa").unwrap().input, 0);
    let (min, max, mean) = tis(&r);
    close("min", min, 4.0, 1e-9);
    close("max", max, 10.0, 1e-9);
    close("média", mean, 7.0, 1e-9);
}

#[test]
fn nearest_ties_go_to_the_first_team_member() {
    // Single item at q (10,0,0); fa starts at x = 0 and fb at x = 20.
    let tie = |order: &str| {
        let text = format!(
            r#"Model(
                name: "empate",
                run: (stop_time: 100.0),
                objects: [
                    (name: "src", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1)),
                    (name: "q", kind: Queue(), transport: "frota", position: (10.0, 0.0, 0.0)),
                    (name: "out", kind: Sink, position: (10.0, 0.0, 0.0)),
                    (name: "fa", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0)),
                    (name: "fb", kind: Transporter(speed: 1.0), position: (20.0, 0.0, 0.0)),
                    (name: "frota", kind: Dispatcher(team: {order}, rule: Nearest)),
                ],
                connections: [("src", "q"), ("q", "out")],
            )"#
        );
        let r = run(&text);
        (r.block("fa").unwrap().input, r.block("fb").unwrap().input)
    };
    // Both forklifts are 10 away from q: the earlier one in the team wins.
    assert_eq!(tie(r#"["fa", "fb"]"#), (1, 0));
    assert_eq!(tie(r#"["fb", "fa"]"#), (0, 1));
}

// -------------------------------------------------------------- interruptions

#[test]
fn breakdown_during_travel_delays_delivery_by_exactly_the_repair_time() {
    // Forklift and q at the origin, sink 10 away at speed 2: travel 0 -> 5.
    // First failure at t = mtbf = 3 (2 time units of travel left), repair 4 -> back
    // at 7, delivered at 9 = 5 + 4. The next failure is at 7 + 3 = 10, after delivery.
    let r = run(&line(
        1,
        1000.0,
        "(0.0, 0.0, 0.0)",
        "(6.0, 8.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0), downtime: (mtbf: Const(3.0), mttr: Const(4.0)))"#,
    ));
    close("tempo no sistema", tis(&r).0, 9.0, 1e-9);
}

#[test]
fn repeated_breakdowns_accumulate() {
    // Same trip with mtbf 2, mttr 4. Fail at 2 (3 left), back at 6, due 9. Next
    // failure at 6 + 2 = 8 (1 left), back at 12, due 13. Next failure at 14: after.
    // Delay = 8 = two repairs.
    let r = run(&line(
        1,
        1000.0,
        "(0.0, 0.0, 0.0)",
        "(6.0, 8.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0), downtime: (mtbf: Const(2.0), mttr: Const(4.0)))"#,
    ));
    close("tempo no sistema", tis(&r).0, 13.0, 1e-9);
}

#[test]
fn breakdown_during_loading_with_handling_times() {
    // Load 2, travel 5 (speed 2, 10 away), unload 0.5; mtbf 4, mttr 2.
    // Load 0 -> 2; travel starts at 2. Fail at 4: 3 of travel left; back at 6, travel
    // ends at 9, unload ends at 9.5. The next failure is at 6 + 4 = 10. 9.5 = 7.5 + 2.
    let r = run(&line(
        1,
        1000.0,
        "(0.0, 0.0, 0.0)",
        "(6.0, 8.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0, load_time: Const(2.0), unload_time: Const(0.5)),
            downtime: (mtbf: Const(4.0), mttr: Const(2.0)))"#,
    ));
    close("tempo no sistema", tis(&r).0, 9.5, 1e-9);
}

#[test]
fn shifts_pause_loading_and_unloading_keeping_the_time_left() {
    // Load 5, travel 5, unload 1 (no stop: 11). Off shift [3,5) and [12.5,15.5).
    // Load: 3 done, 2 left; back at 5; loaded at 7. Travel 7 -> 12. Unload from 12:
    // 0.5 done at 12.5, off until 15.5, 0.5 left -> delivered at 16.
    let r = run(&line(
        1,
        1000.0,
        "(0.0, 0.0, 0.0)",
        "(6.0, 8.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0, load_time: Const(5.0), unload_time: Const(1.0)),
            shifts: (period: 100.0, off: [(start: 3.0, duration: 2.0), (start: 12.5, duration: 3.0)]))"#,
    ));
    close("tempo no sistema", tis(&r).0, 16.0, 1e-9);
}

#[test]
fn a_task_for_a_forklift_that_is_off_shift_waits_for_it() {
    // Off shift from t = 0 to 3; the item (t = 0) is picked at 3, travel 5: 8.
    let r = run(&line(
        1,
        1000.0,
        "(0.0, 0.0, 0.0)",
        "(6.0, 8.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0),
            shifts: (period: 100.0, off: [(start: 0.0, duration: 3.0)]))"#,
    ));
    close("tempo no sistema", tis(&r).0, 8.0, 1e-9);
}

#[test]
fn interrupted_travel_is_traced_in_pieces() {
    // Travel (0,0,0) -> (6,8,0), 5 time units. Off shift [2,4): at t = 2 the forklift
    // has done 2/5 of the path (linear in time): (2.4, 3.2, 0). Trace:
    //   t=0 Travel [(0,0,0),(6,8,0)] 5;  t=2 Travel [p,p] 0 (stop);  t=4 Travel [p,(6,8,0)] 3.
    let m = model(&line(
        1,
        1000.0,
        "(0.0, 0.0, 0.0)",
        "(6.0, 8.0, 0.0)",
        100.0,
        r#"(name: "frota", kind: Transporter(speed: 2.0),
            shifts: (period: 100.0, off: [(start: 2.0, duration: 2.0)]))"#,
    ));
    let replay = m.replay(0).unwrap();
    let carrier = 3; // object index of the forklift
    let travels: Vec<_> = replay
        .records
        .iter()
        .filter_map(|r| match &r.event {
            TraceEvent::Travel {
                carrier: c,
                path,
                duration,
            } if *c == carrier => Some((r.t, path.clone(), *duration)),
            _ => None,
        })
        .collect();
    assert_eq!(travels.len(), 3, "{travels:?}");
    let p = [2.4, 3.2, 0.0];
    let same = |a: &[f64; 3], b: &[f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
    assert_eq!(travels[0].0, 0.0);
    close("duração 1", travels[0].2, 5.0, 1e-9);
    close("t parada", travels[1].0, 2.0, 1e-9);
    assert!(same(&travels[1].1[0], &p) && same(&travels[1].1[1], &p));
    close("t retomada", travels[2].0, 4.0, 1e-9);
    close("duração 3", travels[2].2, 3.0, 1e-9);
    assert!(same(&travels[2].1[0], &p) && same(&travels[2].1[1], &[6.0, 8.0, 0.0]));
    close("entrega", tis(&replay.report).0, 7.0, 1e-9);
}

// ------------------------------------------------------------------------ home

#[test]
fn home_travel_is_interrupted_by_a_new_task() {
    // Forklift home (0,0,0), speed 1; q at (10,0,0); sink at (10,10,0).
    // Item 1 (t=0): empty 10 (t=10), loaded 10 -> delivered at 20 (time in system 20).
    // The queue is empty, so it heads home: |(10,10)| = 10√2 = 14.142 (would arrive at 34.14).
    // Item 2 (t=30): it has driven 10 of the 14.142 -> 1/√2 of the way: it is at
    // (10,10)·(1 - 1/√2) = (a', a') with a' = 10 - 10/√2. Distance to q (10,0):
    // sqrt((10/√2)² + a'²). Then 10 loaded: time in system = that + 10.
    let text = |home: bool| {
        line(
            2,
            30.0,
            "(10.0, 0.0, 0.0)",
            "(10.0, 10.0, 0.0)",
            100.0,
            &format!(
                r#"(name: "frota", kind: Transporter(speed: 1.0, home: {home}), position: (0.0, 0.0, 0.0))"#
            ),
        )
    };
    let r = run(&text(true));
    let s = 10.0 / 2f64.sqrt();
    let a = 10.0 - s;
    let second = (s * s + a * a).sqrt() + 10.0;
    let t = &r.block("out").unwrap().extra["time_in_system"];
    close("2º item (parte do meio do caminho)", t.min, second, 1e-9);
    close("1º item", t.max, 20.0, 1e-9);
    // Without home it stays at the sink: 10 empty + 10 loaded = 20 for the second item.
    let r = run(&text(false));
    let t = &r.block("out").unwrap().extra["time_in_system"];
    close("sem home", t.min, 20.0, 1e-9);
    close("sem home", t.max, 20.0, 1e-9);
}

// ------------------------------------------------------------------- priorities

/// Test station: asks one transporter for several `Operate` tasks at t = 0, with
/// the given priorities, records the arrival time of each and releases it 1 later.
struct Boss {
    carrier: BlockId,
    wanted: Vec<(&'static str, i32)>,
    labels: HashMap<u64, &'static str>,
}

impl Behavior for Boss {
    fn kind(&self) -> &'static str {
        "boss"
    }
    fn reset(&mut self, cx: &mut Ctx) {
        cx.schedule(0.0, 0, None);
    }
    fn accepts(&self, _: &Item, _: usize, _: usize) -> bool {
        false
    }
    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _: Option<utsim_core::ItemId>) {
        if tag == 0 {
            for (label, priority) in self.wanted.clone() {
                let kind = TaskKind::Operate {
                    station: cx.me(),
                    tag: 0,
                };
                let RequestId(id) = cx.request_task(self.carrier, kind, priority);
                self.labels.insert(id, label);
            }
        } else {
            cx.send(self.carrier, MSG_RELEASE, u64::from(tag - 1));
        }
    }
    fn on_message(&mut self, cx: &mut Ctx, _: BlockId, tag: u32, payload: u64) {
        if tag == MSG_ARRIVED {
            let label = self.labels[&payload];
            let now = cx.now();
            cx.record(label, now);
            cx.schedule(1.0, payload as u32 + 1, None);
        }
    }
}

#[test]
fn higher_priority_tasks_are_served_first_fifo_among_equals() {
    // Four Operate requests at t = 0: A (0), B (5), C (9), D (5). Order served:
    // C, B, D, A. The transporter starts 10 away from the station at speed 1:
    // C arrives at 10, is released at 11 (stay 1) and the next one is already there
    // (no travel): B at 11, D at 12, A at 13.
    let mut sim = Simulation::new(RunConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
        replication: 0,
        sample_interval: None,
        histogram_bins: None,
    });
    let f = sim.add_block("f", OutputRule::default(), Box::new(Transporter::new(1.0)));
    sim.set_position(f, [10.0, 0.0, 0.0]);
    let boss = sim.add_block(
        "boss",
        OutputRule::default(),
        Box::new(Boss {
            carrier: f,
            wanted: vec![("A", 0), ("B", 5), ("C", 9), ("D", 5)],
            labels: HashMap::new(),
        }),
    );
    sim.set_position(boss, [0.0, 0.0, 0.0]);
    let r = sim.run().unwrap();
    let at = |l: &str| r.block("boss").unwrap().extra[l].min;
    close("C", at("C"), 10.0, 1e-9);
    close("B", at("B"), 11.0, 1e-9);
    close("D", at("D"), 12.0, 1e-9);
    close("A", at("A"), 13.0, 1e-9);
    let f = r.block("f").unwrap();
    close("travel_empty", f.states["travel_empty"], 0.10, 1e-9);
    close("utilize", f.states["utilize"], 0.04, 1e-9);
}

// ------------------------------------------------------------------ statistics

#[test]
fn single_transporter_behaves_like_an_md1_server() {
    // q at (0,0,0), sink at (7,0,0), speed 4: each leg takes 1.75. The forklift
    // starts at the sink and always ends a job there, so EVERY job costs
    // R = empty 1.75 + loaded 1.75 = 3.5 (deterministic service, FIFO).
    // Arrivals are Poisson, mean gap 5 -> λ = 0.2, ρ = 0.7.
    // M/D/1 (Pollaczek-Khinchine): Lq = ρ²/(2(1-ρ)) = 0.49/0.6 = 0.81667,
    // Wq = Lq/λ = 4.08333, W = Wq + R = 7.58333.
    // An item sits in `q` until the forklift reaches it: its service starts 1.75
    // earlier, so the content of `q` is Lq + λ·1.75 = 0.81667 + 0.35 = 1.16667.
    let text = r#"Model(
        name: "md1",
        run: (stop_time: 200000.0, replications: 4, seed: 7),
        objects: [
            (name: "src", kind: Source(interarrival: Exp(mean: 5.0))),
            (name: "q", kind: Queue(), transport: "f", position: (0.0, 0.0, 0.0)),
            (name: "out", kind: Sink, position: (7.0, 0.0, 0.0)),
            (name: "f", kind: Transporter(speed: 4.0), position: (7.0, 0.0, 0.0)),
        ],
        connections: [("src", "q"), ("q", "out")],
    )"#;
    let reps = model(text).run_replications().unwrap();
    let n = reps.len() as f64;
    let mean = |f: &dyn Fn(&RunReport) -> f64| reps.iter().map(f).sum::<f64>() / n;
    let rho: f64 = 0.7;
    let lq = rho * rho / (2.0 * (1.0 - rho));
    let content = mean(&|r| r.block("q").unwrap().content_mean);
    let w = mean(&|r| tis(r).2);
    let util = mean(&|r| {
        let s = &r.block("f").unwrap().states;
        s["travel_empty"] + s["travel_loaded"]
    });
    assert!((util - rho).abs() / rho < 0.02, "utilização {util}");
    assert!(
        (content - (lq + 0.2 * 1.75)).abs() / (lq + 0.35) < 0.08,
        "conteúdo de q {content}, esperado {}",
        lq + 0.35
    );
    let w_theory = lq / 0.2 + 3.5;
    assert!(
        (w - w_theory).abs() / w_theory < 0.08,
        "tempo no sistema {w}, esperado {w_theory}"
    );
}
