//! Conveyors and racks in the viewer's timeline (phase 5): the belt motion rebuilt from
//! the trace must satisfy the belt's own invariants, and racks must show every item in
//! the slot the kernel gave it.

use std::collections::HashMap;
use std::path::Path;

use utsim_core::{State, TraceEvent};
use utsim_model::Model;
use utsim_model::replay::Replay;
use utsim_scene::handling::{BeltParams, Handling};
use utsim_scene::timeline::{ItemPos, Timeline, TransitSetting};

const EPS: f64 = 1e-6;

fn example(name: &str) -> Model {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    Model::load(path.join(name)).unwrap()
}

/// Timeline of a replay with the belt parameters taken from `model`.
fn timeline_of(model: &Model, replay: &Replay) -> Timeline {
    let mut tl = Timeline::build(replay, TransitSetting::Auto);
    for (b, block) in replay.blocks.iter().enumerate() {
        let obj = model.objects.iter().find(|o| o.name == block.name).unwrap();
        if let Some(Handling::Conveyor(p)) = Handling::of(&obj.kind) {
            tl.set_belt_params(b, p);
        }
    }
    tl
}

/// Dense item index of every trace key, in order of first appearance (as the timeline does).
fn item_index(replay: &Replay) -> HashMap<u64, usize> {
    let mut index = HashMap::new();
    let mut touch = |key: u64| {
        let n = index.len();
        index.entry(key).or_insert(n);
    };
    for r in &replay.records {
        match &r.event {
            TraceEvent::Created { item, .. }
            | TraceEvent::Moved { item, .. }
            | TraceEvent::Unpacked { item, .. }
            | TraceEvent::Destroyed { item }
            | TraceEvent::Slot { item, .. } => touch(*item),
            TraceEvent::Packed { item, container } => {
                touch(*item);
                touch(*container);
            }
            _ => {}
        }
    }
    index
}

/// `(item, entry time, exit time)` of every visit to block `b` (exit `None` while inside).
fn visits(replay: &Replay, b: usize) -> Vec<(usize, f64, Option<f64>)> {
    let index = item_index(replay);
    let mut open: HashMap<usize, usize> = HashMap::new();
    let mut out: Vec<(usize, f64, Option<f64>)> = Vec::new();
    for r in &replay.records {
        if let TraceEvent::Moved { item, from, to } = r.event {
            let i = index[&item];
            if from == b
                && let Some(k) = open.remove(&i)
            {
                out[k].2 = Some(r.t);
            }
            if to == b {
                open.insert(i, out.len());
                out.push((i, r.t, None));
            }
        }
    }
    out
}

/// Checks every belt invariant on block `b` of `replay` (parameters `p`).
fn check_belt(replay: &Replay, tl: &Timeline, b: usize, p: BeltParams, what: &str) {
    let track = tl
        .belt_track(b)
        .unwrap_or_else(|| panic!("{what}: no motion"));
    // 1. The reconstruction agrees with the engine's exit times (a snap of more than
    //    rounding would mean an item left before it could have arrived).
    assert!(
        track.max_snap() < EPS,
        "{what}: snapped by {}",
        track.max_snap()
    );
    let visits = visits(replay, b);
    assert!(
        visits.len() > 20,
        "{what}: too few items ({})",
        visits.len()
    );
    let mut exits = 0;
    for &(item, t_in, t_out) in &visits {
        // The item starts at the entry of the belt.
        assert!(
            track.position_of(item, t_in).unwrap().abs() < EPS,
            "{what}: entry"
        );
        if let Some(t_out) = t_out {
            // 2. It is at the very end of the belt when it leaves, and never took less
            //    than L / v to get there.
            let at_exit = track.position_of(item, t_out).unwrap();
            assert!(
                (at_exit - p.length).abs() < EPS,
                "{what}: exit at {at_exit}"
            );
            assert!(
                t_out - t_in >= p.length / p.speed - 1e-6,
                "{what}: item {item} crossed {} in {}",
                p.length,
                t_out - t_in
            );
            exits += 1;
        }
    }
    assert!(exits > 10, "{what}: only {exits} items left");

    // 3. At sampled instants: inside the belt, never behind its previous position, and
    //    at least one item length behind the item ahead.
    let end = replay.stop_time;
    let steps = 6000;
    let mut last: HashMap<usize, f64> = HashMap::new();
    let mut most = 0;
    for k in 0..=steps {
        let t = end * k as f64 / steps as f64;
        let mut on: Vec<(usize, f64)> = visits
            .iter()
            .filter(|v| v.1 <= t && v.2.is_none_or(|out| t < out))
            .map(|v| (v.0, track.position_of(v.0, t).unwrap()))
            .collect();
        for &(item, pos) in &on {
            assert!(
                (-EPS..=p.length + EPS).contains(&pos),
                "{what}: t={t} item {item} at {pos}"
            );
            if let Some(&before) = last.get(&item) {
                assert!(pos >= before - EPS, "{what}: item {item} went backwards");
            }
            last.insert(item, pos);
        }
        on.sort_by(|a, b| b.1.total_cmp(&a.1));
        for w in on.windows(2) {
            assert!(
                w[0].1 - w[1].1 >= p.item_length - 1e-6,
                "{what}: t={t} gap {} < {}",
                w[0].1 - w[1].1,
                p.item_length
            );
        }
        // The kernel's content count agrees with the items we place on the belt.
        assert_eq!(on.len(), tl.block_content(b, t), "{what}: count at {t}");
        most = most.max(on.len());
    }
    assert!(most >= 2, "{what}: the belt never held two items");

    // 4. While the belt is down or off shift nothing moves.
    let mut frozen = 0;
    let dt = end / 20_000.0;
    for k in 0..20_000 {
        let (t0, t1) = (k as f64 * dt, (k + 1) as f64 * dt);
        let stopped = |t| matches!(tl.block_state(b, t), Some(State::Down | State::OffShift));
        if stopped(t0) && stopped(t1) {
            for v in &visits {
                let in_both = v.1 <= t0 && v.2.is_none_or(|out| t1 < out);
                if in_both {
                    let (a, c) = (
                        track.position_of(v.0, t0).unwrap(),
                        track.position_of(v.0, t1).unwrap(),
                    );
                    assert!((a - c).abs() < EPS, "{what}: moved while stopped at {t0}");
                    frozen += 1;
                }
            }
        }
    }
    eprintln!(
        "{what}: {} visits, most on belt {most}, frozen samples {frozen}",
        visits.len()
    );
}

#[test]
fn belts_of_the_example_line_keep_their_invariants() {
    let mut model = example("linha_com_esteiras.ron");
    model.run.stop_time = 1500.0;
    model.run.warmup = 0.0;
    let replay = model.replay(0).unwrap();
    let tl = timeline_of(&model, &replay);
    let mut seen = 0;
    for (b, block) in replay.blocks.iter().enumerate() {
        let obj = model.objects.iter().find(|o| o.name == block.name).unwrap();
        if let Some(Handling::Conveyor(p)) = Handling::of(&obj.kind) {
            check_belt(&replay, &tl, b, p, &block.name);
            seen += 1;
        }
    }
    assert_eq!(seen, 2);
}

#[test]
fn an_unobstructed_belt_delivers_exactly_when_the_item_arrives() {
    // The second belt of the example feeds a sink, which always accepts: every item
    // must leave at the very moment it reaches the end.
    let mut model = example("linha_com_esteiras.ron");
    model.run.stop_time = 800.0;
    model.run.warmup = 0.0;
    let replay = model.replay(0).unwrap();
    let tl = timeline_of(&model, &replay);
    let b = replay
        .blocks
        .iter()
        .position(|x| x.name == "esteira2")
        .unwrap();
    let list = tl.belt_track(b).unwrap().arrivals_and_exits();
    assert!(list.len() > 50);
    for (item, arrived, left) in list {
        assert!(
            (left - arrived).abs() < 1e-6,
            "item {item}: arrived {arrived}, left {left}"
        );
    }
    // The first belt feeds a busy machine, so some items wait at its end.
    let b1 = replay
        .blocks
        .iter()
        .position(|x| x.name == "esteira1")
        .unwrap();
    let waits = tl.belt_track(b1).unwrap().arrivals_and_exits();
    assert!(waits.iter().any(|&(_, a, l)| l - a > 0.5));
}

/// A slow machine after a belt with breakdowns and shifts, accumulating or not.
fn stressed(accumulating: bool) -> Model {
    Model::parse(&format!(
        r#"Model(
            name: "esteira sob estresse",
            run: (stop_time: 1200.0, seed: 3, replications: 1),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 1.2)), position: (0.0, 0.0, 0.0)),
                (name: "belt", kind: Conveyor(length: 6.0, speed: 1.0, item_length: 1.0, accumulating: {accumulating}),
                    position: (5.0, 0.0, 0.0),
                    downtime: (mtbf: Exp(mean: 40.0), mttr: Uniform(min: 2.0, max: 6.0)),
                    shifts: (period: 100.0, off: [(start: 60.0, duration: 12.0)])),
                (name: "mach", kind: Processor(process: Exp(mean: 1.5)), position: (10.0, 0.0, 0.0)),
                (name: "snk", kind: Sink, position: (13.0, 0.0, 0.0)),
            ],
            connections: [("src", "belt"), ("belt", "mach"), ("mach", "snk")],
        )"#
    ))
    .unwrap()
}

#[test]
fn belts_with_breakdowns_and_shifts_keep_their_invariants() {
    for accumulating in [true, false] {
        let model = stressed(accumulating);
        let replay = model.replay(0).unwrap();
        let tl = timeline_of(&model, &replay);
        let b = 1;
        let Some(Handling::Conveyor(p)) = Handling::of(&model.objects[b].kind) else {
            unreachable!()
        };
        // The scenario must actually stop the belt.
        let downs = replay
            .records
            .iter()
            .filter(|r| {
                matches!(
                    r.event,
                    TraceEvent::State {
                        block: 1,
                        state: State::Down | State::OffShift
                    }
                )
            })
            .count();
        assert!(downs >= 4, "only {downs} stops");
        check_belt(&replay, &tl, b, p, &format!("accumulating={accumulating}"));
    }
}

#[test]
fn items_stand_in_the_rack_slot_the_kernel_chose() {
    let mut model = example("armazem.ron");
    model.run.stop_time = 600.0;
    model.run.warmup = 0.0;
    let replay = model.replay(0).unwrap();
    let tl = timeline_of(&model, &replay);
    let rack = replay.blocks.iter().position(|b| b.kind == "Rack").unwrap();
    let index = item_index(&replay);
    let slots = 6 * 4;
    let mut stored = 0;
    let mut full_once = false;
    for r in &replay.records {
        let TraceEvent::Slot { item, block, slot } = r.event else {
            continue;
        };
        assert_eq!(block, rack);
        assert!(slot < slots);
        let i = index[&item];
        // From the Slot event on, the timeline says the item is in that slot.
        assert_eq!(tl.rack_slot(rack, i, r.t), Some(slot));
        assert_eq!(tl.item_pos(i, r.t), ItemPos::At(rack));
        stored += 1;
    }
    assert!(stored > 50, "{stored}");
    // At any instant the occupied slots are distinct and as many as the block holds.
    let steps = 3000;
    for k in 0..=steps {
        let t = replay.stop_time * k as f64 / steps as f64;
        let mut live = Vec::new();
        tl.live_items(t, &mut live);
        let mut used: Vec<usize> = live
            .iter()
            .filter(|&&i| tl.item_pos(i, t) == ItemPos::At(rack))
            .map(|&i| tl.rack_slot(rack, i, t).expect("stored item has a slot"))
            .collect();
        assert_eq!(used.len(), tl.block_content(rack, t), "t={t}");
        used.sort_unstable();
        used.dedup();
        assert_eq!(
            used.len(),
            tl.block_content(rack, t),
            "slots repeat at t={t}"
        );
        full_once |= used.len() == slots;
    }
    // The 24 positions of the example fill up at some point.
    assert!(full_once, "the rack never filled up");
    assert_eq!(tl.max_slot(rack), Some(slots - 1));
}

#[test]
fn moves_into_belts_and_racks_are_not_tweened() {
    let mut model = example("linha_com_esteiras.ron");
    model.run.stop_time = 100.0;
    model.run.warmup = 0.0;
    let replay = model.replay(0).unwrap();
    let tl = Timeline::build(&replay, TransitSetting::Fixed(0.5));
    let belt = replay
        .blocks
        .iter()
        .position(|b| b.kind == "Conveyor")
        .unwrap();
    for r in &replay.records {
        if let TraceEvent::Moved { item, to, from } = r.event
            && (to == belt || from == belt)
        {
            let i = item_index(&replay)[&item];
            let p = tl.item_pos(i, r.t + 1e-9);
            assert!(!matches!(p, ItemPos::Transit { .. }), "{p:?}");
        }
    }
}
