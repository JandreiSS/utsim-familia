//! Property tests of the reconstruction (`docs/plano_lean_seis_sigma.md`, LSS-RNF-05): the
//! same set of operations gives the same state whatever the order they arrive in, with or
//! without a snapshot, and every field ends with the value of its last operation.

use std::collections::BTreeMap;

use proptest::prelude::*;
use serde_json::Value;
use utsim_lss_core::{Edit, EntityRef, Id, Insert, Journal, Kind, Op, State};

const FIELDS: [&str; 3] = ["titulo", "coluna", "prazo"];

#[derive(Debug, Clone)]
enum Step {
    /// Author writes a field of an entity, from what they have seen.
    Write {
        author: usize,
        entity: usize,
        field: usize,
        value: u8,
    },
    /// Author `to` receives everything `from` has.
    Sync { from: usize, to: usize },
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => (0..3usize, 0..3usize, 0..FIELDS.len(), 0..4u8).prop_map(|(author, entity, field, value)| Step::Write { author, entity, field, value }),
        1 => (0..3usize, 0..3usize).prop_map(|(from, to)| Step::Sync { from, to }),
    ]
}

/// Runs the scenario: three replicas that write and sync now and then. Returns every
/// operation made.
fn simulate(steps: &[Step]) -> Vec<Op> {
    let authors: Vec<Id> = (0..3).map(|i| Id::v7(1, [i as u8 + 1; 10])).collect();
    let entities: Vec<EntityRef> = (0..3)
        .map(|i| EntityRef::new(Kind::Action, Id::v7(2, [i as u8 + 10; 10])))
        .collect();
    let mut seen: Vec<BTreeMap<Id, Op>> = vec![BTreeMap::new(); 3];
    let mut n = 0u64;
    for s in steps {
        match *s {
            Step::Write {
                author,
                entity,
                field,
                value,
            } => {
                let local = State::replay(seen[author].values());
                let edit = Edit::new(entities[entity], FIELDS[field], Value::from(value));
                let ops = local.prepare(authors[author], n as i64, [edit], || {
                    n += 1;
                    Id::v7(1_000 + n, [(n % 256) as u8; 10])
                });
                for op in ops {
                    seen[author].insert(op.id, op);
                }
            }
            Step::Sync { from, to } => {
                let theirs: Vec<Op> = seen[from].values().cloned().collect();
                for op in theirs {
                    seen[to].insert(op.id, op);
                }
            }
        }
    }
    let mut all: BTreeMap<Id, Op> = BTreeMap::new();
    for s in seen {
        all.extend(s);
    }
    all.into_values().collect()
}

fn shuffled(ops: &[Op], keys: &[u64]) -> Vec<Op> {
    let mut idx: Vec<usize> = (0..ops.len()).collect();
    idx.sort_by_key(|&i| {
        (
            keys.get(i % keys.len().max(1)).copied().unwrap_or(0) ^ (i as u64 * 0x9E37_79B9),
            i,
        )
    });
    idx.into_iter().map(|i| ops[i].clone()).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn any_arrival_order_gives_the_same_state(
        steps in prop::collection::vec(step(), 1..60),
        keys in prop::collection::vec(any::<u64>(), 1..64),
        keys2 in prop::collection::vec(any::<u64>(), 1..64),
        cut in 0usize..100,
    ) {
        let ops = simulate(&steps);
        let reference = State::replay(&ops);

        // Replay of a shuffled set.
        let a = shuffled(&ops, &keys);
        prop_assert_eq!(&State::replay(&a), &reference);

        // One at a time, in arrival order, with duplicates.
        let mut j = Journal::new();
        for op in a.iter().chain(a.iter().take(3)) {
            let r = j.insert(op.clone());
            prop_assert_ne!(r, Insert::NeedsFullReplay);
        }
        prop_assert_eq!(j.state(), &reference);

        // A snapshot made by a replica that saw only part of the operations, then the rest
        // arriving in another order: either usable and equal, or refused.
        let b = shuffled(&ops, &keys2);
        let cut = if b.is_empty() { 0 } else { cut % (b.len() + 1) };
        let snap = Journal::from_ops(b[..cut].to_vec()).snapshot();
        let rebuilt = match Journal::from_snapshot(snap.clone(), b.clone()) {
            Ok(j) => j.state().clone(),
            Err(all) => Journal::from_ops(all).state().clone(),
        };
        prop_assert_eq!(&rebuilt, &reference);

        // Incremental inserts over a snapshot, falling back to the full replay when told.
        let mut j = Journal::from_snapshot(snap, Vec::new()).unwrap();
        let mut full = false;
        for op in &b[cut..] {
            if j.insert(op.clone()) == Insert::NeedsFullReplay {
                full = true;
                break;
            }
        }
        if full {
            j = Journal::from_ops(ops.clone());
        }
        prop_assert_eq!(j.state(), &reference);
    }

    #[test]
    fn every_field_ends_with_its_last_operation(steps in prop::collection::vec(step(), 1..60)) {
        let ops = simulate(&steps);
        let state = State::replay(&ops);
        let mut last: BTreeMap<(Id, String), &Op> = BTreeMap::new();
        for op in &ops {
            let slot = last.entry((op.entity.id, op.field.clone())).or_insert(op);
            if op.key() > slot.key() {
                *slot = op;
            }
        }
        for ((entity, field), op) in last {
            let e = state.entity(entity).unwrap();
            prop_assert_eq!(e.get(&field), Some(&op.value));
            prop_assert_eq!(e.version(&field), Some(op.id));
        }
        // Every conflict keeps the later operation.
        for c in &state.conflicts {
            prop_assert!((c.kept.lamport, c.kept.author, c.kept.op) > (c.replaced.lamport, c.replaced.author, c.replaced.op));
        }
    }

    #[test]
    fn writers_who_always_sync_never_conflict(writes in prop::collection::vec((0..3usize, 0..3usize, 0..FIELDS.len(), 0..4u8), 1..40)) {
        // Before each write everyone syncs with everyone.
        let mut steps = Vec::new();
        for (author, entity, field, value) in writes {
            for from in 0..3 {
                for to in 0..3 {
                    steps.push(Step::Sync { from, to });
                }
            }
            steps.push(Step::Write { author, entity, field, value });
        }
        let state = State::replay(&simulate(&steps));
        prop_assert!(state.conflicts.is_empty());
    }
}

#[test]
fn two_people_editing_the_same_field_offline_conflict_once() {
    let steps = [
        Step::Write {
            author: 0,
            entity: 0,
            field: 0,
            value: 1,
        },
        Step::Sync { from: 0, to: 1 },
        Step::Write {
            author: 0,
            entity: 0,
            field: 0,
            value: 2,
        },
        Step::Write {
            author: 1,
            entity: 0,
            field: 0,
            value: 3,
        },
        Step::Sync { from: 0, to: 1 },
        Step::Sync { from: 1, to: 0 },
    ];
    let ops = simulate(&steps);
    let state = State::replay(&ops);
    assert_eq!(state.conflicts.len(), 1);
    let c = &state.conflicts[0];
    let values = [c.kept.value.clone(), c.replaced.value.clone()];
    assert!(values.contains(&Value::from(2)) && values.contains(&Value::from(3)));
}

/// Two people edit different lines of the same minutes from the same version (P7, conflict by
/// paragraph): both changes stay and there is no conflict, also over a snapshot (serialized)
/// taken between the two edits. The same line edited by both is still a conflict.
#[test]
fn edits_of_different_lines_of_a_text_merge() {
    let ana = Id::v7(1, [1; 10]);
    let bia = Id::v7(1, [2; 10]);
    let minutes = EntityRef::new(Kind::Action, Id::v7(2, [9; 10]));
    let mut n = 0u64;
    let mut next = || {
        n += 1;
        Id::v7(1_000 + n, [(n % 256) as u8; 10])
    };
    let text = |s: &str| Value::from(s);
    let base = State::new().prepare(
        ana,
        1,
        [Edit::new(
            minutes,
            "descricao",
            text("Abertura.\nPauta: FMEA.\nFim."),
        )],
        &mut next,
    );
    let seen = State::replay(&base);
    let run = |a: &str, b: &str, next: &mut dyn FnMut() -> Id| {
        let oa = seen.prepare(
            ana,
            2,
            [Edit::new(minutes, "descricao", text(a))],
            &mut *next,
        );
        let ob = seen.prepare(
            bia,
            3,
            [Edit::new(minutes, "descricao", text(b))],
            &mut *next,
        );
        let mut ops = base.clone();
        ops.extend(oa);
        ops.extend(ob);
        ops
    };
    let ops = run(
        "Abertura às 9h.\nPauta: FMEA.\nFim.",
        "Abertura.\nPauta: FMEA.\nFim às 10h.",
        &mut next,
    );
    let state = State::replay(&ops);
    assert!(state.conflicts.is_empty(), "{:?}", state.conflicts);
    let merged = "Abertura às 9h.\nPauta: FMEA.\nFim às 10h.";
    assert_eq!(state.entity(minutes.id).unwrap().text("descricao"), merged);
    // Over a snapshot taken after the first edit, sent through its file form.
    let mut sorted = ops.clone();
    sorted.sort_by_key(|o| o.key());
    let snap = Journal::from_ops(sorted[..2].to_vec()).snapshot();
    let snap = serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
    let j = Journal::from_snapshot(snap, sorted.clone()).unwrap();
    assert_eq!(j.state(), &state);

    let ops = run(
        "Abertura às 9h.\nPauta: FMEA.\nFim.",
        "Abertura às 8h.\nPauta: FMEA.\nFim.",
        &mut next,
    );
    assert_eq!(State::replay(&ops).conflicts.len(), 1);
}
