//! Property tests over random command sequences.
//!
//! A seed produces a reproducible sequence of commands (valid, invalid, against
//! existing or missing objects, and batches). After every step the document must
//! keep its invariants; accepted and rejected commands must behave as specified.

use std::collections::HashSet;
use std::path::PathBuf;

use rand::Rng;
use utsim_core::rng::{SimRng, stream};
use utsim_edit::{Command, Document, LIBRARY, template};
use utsim_model::Model;

const SEEDS: u64 = 50;
const STEPS: usize = 200;
/// Shorter prefix for the gesture tests, which only need a varied starting document.
const SHORT_STEPS: usize = 40;

/// Stream ids keep the generator and the gesture tests on independent streams.
const GEN_STREAM: u64 = 0;
const GESTURE_STREAM: u64 = 1;

const BASES: [&str; 6] = [
    "chegada",
    "fila",
    "maquina",
    "montagem",
    "separador",
    "saida",
];

/// Fields the generator sets, with values that are sometimes valid and sometimes not.
const SET_FIELDS: &[(&str, &[&str])] = &[
    ("process", &["Const(2.0)", "Exp(mean: 1.5)", "Const(-1.0)"]),
    ("capacity", &["3", "1", "0"]),
    ("interarrival", &["Exp(mean: 2.0)", "Const(0.0)"]),
    ("setup", &["None", "Some(Const(0.5))"]),
    ("components", &["[1]", "[]"]),
    ("mode", &["Join", "Pack", "Split(2)"]),
    ("bogus", &["1"]),
];

/// `run.stop_time` values; all are <= 500 so `replay(0)` stays fast.
const STOP_TIMES: [&str; 4] = ["500.0", "120.0", "0.0", "-3.0"];

fn random_position(rng: &mut SimRng) -> [f64; 3] {
    [
        rng.random_range(-100.0..100.0),
        rng.random_range(-100.0..100.0),
        0.0,
    ]
}

/// An existing object most of the time, a missing name otherwise.
fn pick_name(rng: &mut SimRng, doc: &Document) -> String {
    let names: Vec<&str> = doc
        .model()
        .objects
        .iter()
        .map(|o| o.name.as_str())
        .collect();
    if !names.is_empty() && rng.random_bool(0.85) {
        names[rng.random_range(0..names.len())].to_string()
    } else {
        format!("fantasma{}", rng.random_range(0..3))
    }
}

fn gen_command(rng: &mut SimRng, doc: &Document, allow_batch: bool) -> Command {
    match rng.random_range(0..100u32) {
        0..=19 => gen_add(rng, doc),
        20..=27 => Command::Remove {
            name: pick_name(rng, doc),
        },
        28..=35 => gen_rename(rng, doc),
        36..=45 => Command::Move {
            name: pick_name(rng, doc),
            position: random_position(rng),
        },
        46..=65 => gen_set(rng, doc),
        66..=80 => gen_connect(rng, doc),
        81..=88 => gen_disconnect(rng, doc),
        _ if allow_batch => {
            let n = rng.random_range(2..=3);
            Command::Batch((0..n).map(|_| gen_command(rng, doc, false)).collect())
        }
        _ => gen_connect(rng, doc),
    }
}

fn gen_add(rng: &mut SimRng, doc: &Document) -> Command {
    let (kind, _) = LIBRARY[rng.random_range(0..LIBRARY.len())];
    let base = BASES[rng.random_range(0..BASES.len())];
    let name = match rng.random_range(0..10u32) {
        0 => pick_name(rng, doc),
        1 => "  ".to_string(),
        _ => doc.unique_name(base),
    };
    let object = template(kind, &name, random_position(rng)).expect("LIBRARY kinds have templates");
    Command::Add(Box::new(object))
}

fn gen_rename(rng: &mut SimRng, doc: &Document) -> Command {
    let to = if rng.random_bool(0.7) {
        doc.unique_name("novo")
    } else {
        pick_name(rng, doc)
    };
    Command::Rename {
        from: pick_name(rng, doc),
        to,
    }
}

fn gen_set(rng: &mut SimRng, doc: &Document) -> Command {
    let (key, value) = match rng.random_range(0..100u32) {
        0..=14 => (
            "run.stop_time".to_string(),
            STOP_TIMES[rng.random_range(0..STOP_TIMES.len())],
        ),
        15..=17 => ("run.nao_existe".to_string(), "1.0"),
        18..=22 => (format!("{}.nao_existe", pick_name(rng, doc)), "1"),
        _ => {
            let (field, values) = SET_FIELDS[rng.random_range(0..SET_FIELDS.len())];
            let value = values[rng.random_range(0..values.len())];
            (format!("{}.{field}", pick_name(rng, doc)), value)
        }
    };
    Command::Set {
        key,
        value: value.to_string(),
    }
}

fn gen_connect(rng: &mut SimRng, doc: &Document) -> Command {
    let conns = &doc.model().connections;
    if !conns.is_empty() && rng.random_bool(0.15) {
        // Duplicate of an existing connection.
        let (from, to) = conns[rng.random_range(0..conns.len())].clone();
        return Command::Connect { from, to };
    }
    let from = pick_name(rng, doc);
    let to = if rng.random_bool(0.05) {
        from.clone()
    } else {
        pick_name(rng, doc)
    };
    Command::Connect { from, to }
}

fn gen_disconnect(rng: &mut SimRng, doc: &Document) -> Command {
    let conns = &doc.model().connections;
    if !conns.is_empty() && rng.random_bool(0.6) {
        let (from, to) = conns[rng.random_range(0..conns.len())].clone();
        Command::Disconnect { from, to }
    } else {
        Command::Disconnect {
            from: pick_name(rng, doc),
            to: pick_name(rng, doc),
        }
    }
}

#[derive(Default)]
struct Stats {
    accepted: usize,
    rejected: usize,
    undos: usize,
    redos: usize,
    /// Accepted states whose model passed validation and ran `replay(0)`.
    replays: usize,
}

/// What a rejected command must leave untouched.
#[derive(PartialEq, Debug)]
struct Snapshot {
    model: Model,
    can_undo: bool,
    can_redo: bool,
    dirty: bool,
}

fn snapshot(doc: &Document) -> Snapshot {
    Snapshot {
        model: doc.model().clone(),
        can_undo: doc.can_undo(),
        can_redo: doc.can_redo(),
        dirty: doc.is_dirty(),
    }
}

/// Names unique, connections point to existing objects, no self-loops, no duplicates.
fn check_invariants(doc: &Document, ctx: &str) {
    let model = doc.model();
    let mut names = HashSet::new();
    for o in &model.objects {
        assert!(
            names.insert(o.name.as_str()),
            "{ctx}: nome repetido '{}'",
            o.name
        );
    }
    let mut pairs = HashSet::new();
    for (from, to) in &model.connections {
        assert!(
            names.contains(from.as_str()),
            "{ctx}: ligação de '{from}' inexistente"
        );
        assert!(
            names.contains(to.as_str()),
            "{ctx}: ligação para '{to}' inexistente"
        );
        assert_ne!(from, to, "{ctx}: auto-ligação em '{from}'");
        assert!(
            pairs.insert((from.as_str(), to.as_str())),
            "{ctx}: ligação ({from} -> {to}) repetida"
        );
    }
}

/// Saved text parses back to the same model and is byte-stable.
fn check_round_trip(doc: &Document, ctx: &str) {
    let text = doc.to_text();
    assert_eq!(text, doc.to_text(), "{ctx}: to_text não é estável");
    let parsed = Model::parse(&text).unwrap_or_else(|e| panic!("{ctx}: to_text não reabre: {e}"));
    assert_eq!(
        &parsed,
        doc.model(),
        "{ctx}: modelo salvo difere do modelo em memória"
    );
}

/// `issues()` agrees with `validate()`; an empty list means the model runs.
/// Runs `replay(0)` for runnable models with `stop_time <= 500` (the default
/// model, with 10000, is only validated). Returns whether a replay ran.
fn check_issues(doc: &Document, ctx: &str) -> bool {
    let issues = doc.issues();
    let validation = doc.model().validate();
    assert_eq!(
        issues.is_empty(),
        validation.is_ok(),
        "{ctx}: issues() e validate() discordam: {issues:?}"
    );
    if issues.is_empty() && doc.model().run.stop_time <= 500.0 {
        doc.replay(0)
            .unwrap_or_else(|e| panic!("{ctx}: modelo válido não roda: {e}"));
        true
    } else {
        false
    }
}

/// Runs every per-state check. Returns whether the model was runnable.
fn check_all(doc: &Document, ctx: &str) -> bool {
    check_invariants(doc, ctx);
    check_round_trip(doc, ctx);
    check_issues(doc, ctx)
}

/// Plays one seed's sequence, checking properties after every step. Returns the
/// final document and counters, so callers can check the run was not vacuous.
fn run_sequence(seed: u64, steps: usize) -> (Document, Stats) {
    let mut rng = stream(seed, 0, GEN_STREAM);
    let mut doc = Document::new("sequencia");
    let initial = doc.model().clone();
    doc.apply(Command::Set {
        key: "run.stop_time".into(),
        value: "500.0".into(),
    })
    .expect("setting the stop time works on an empty model");
    let mut stats = Stats::default();
    // The state right after the last accepted command: the end of the history.
    let mut final_model = doc.model().clone();

    for step in 0..steps {
        let roll = rng.random_range(0..100u32);
        // Undo and redo interleaved with commands.
        if roll < 4 && doc.can_undo() {
            let ctx = format!("seed {seed}, passo {step}, undo");
            assert!(doc.undo());
            stats.undos += 1;
            if check_all(&doc, &ctx) {
                stats.replays += 1;
            }
            continue;
        }
        if (4..7).contains(&roll) && doc.can_redo() {
            let ctx = format!("seed {seed}, passo {step}, redo");
            assert!(doc.redo());
            stats.redos += 1;
            if check_all(&doc, &ctx) {
                stats.replays += 1;
            }
            continue;
        }

        let command = gen_command(&mut rng, &doc, true);
        let ctx = format!("seed {seed}, passo {step}, comando {command:?}");
        let before = snapshot(&doc);
        match doc.apply(command.clone()) {
            Err(message) => {
                stats.rejected += 1;
                assert_eq!(
                    snapshot(&doc),
                    before,
                    "{ctx}: comando recusado ({message}) mudou o documento"
                );
            }
            Ok(()) => {
                stats.accepted += 1;
                assert!(
                    !doc.can_redo(),
                    "{ctx}: refazer não foi limpo por novo comando"
                );
                assert!(
                    doc.can_undo(),
                    "{ctx}: comando aceito não gerou passo de desfazer"
                );
                assert!(
                    doc.is_dirty(),
                    "{ctx}: comando aceito não marcou documento como alterado"
                );
                final_model = doc.model().clone();
            }
        }
        if check_all(&doc, &ctx) {
            stats.replays += 1;
        }
    }

    // Walk to the end of the history first: the sequence may have finished with
    // undos, which leave states on the redo stack. Then the redo chain reaches the
    // last accepted state and nothing else.
    while doc.redo() {}
    assert_eq!(
        doc.model(),
        &final_model,
        "seed {seed}: refazer até o fim não chegou ao último estado aceito"
    );
    assert!(!doc.can_redo());
    while doc.undo() {}
    assert_eq!(
        doc.model(),
        &initial,
        "seed {seed}: desfazer tudo não voltou ao inicial"
    );
    assert!(!doc.can_undo());
    while doc.redo() {}
    assert_eq!(
        doc.model(),
        &final_model,
        "seed {seed}: refazer tudo não voltou ao final"
    );
    (doc, stats)
}

#[test]
fn random_sequences_keep_invariants() {
    let mut total = Stats::default();
    for seed in 0..SEEDS {
        let (_, stats) = run_sequence(seed, STEPS);
        total.accepted += stats.accepted;
        total.rejected += stats.rejected;
        total.undos += stats.undos;
        total.redos += stats.redos;
        total.replays += stats.replays;
    }
    // The generator must exercise both outcomes, undo/redo and runnable models.
    assert!(total.accepted > 0, "nenhum comando aceito");
    assert!(total.rejected > 0, "nenhum comando recusado");
    assert!(
        total.undos > 0 && total.redos > 0,
        "undo/redo não exercitados"
    );
    assert!(total.replays > 0, "nenhum estado válido foi executado");
}

#[test]
fn coalescing_gestures_form_single_undo_steps() {
    for seed in 0..SEEDS {
        let ctx = format!("seed {seed}");
        let mut rng = stream(seed, 0, GESTURE_STREAM);
        let (mut doc, _) = run_sequence(seed, SHORT_STEPS);
        // Make sure there is something to move, then work from a clean redo stack.
        doc.apply(Command::Add(Box::new(
            template("Sink", &doc.unique_name("gesto"), [0.0; 3]).unwrap(),
        )))
        .unwrap();
        let name = doc.model().objects.last().unwrap().name.clone();

        // Same key: N moves and end_gesture are one undo step.
        let before = doc.model().clone();
        let n = rng.random_range(2..=10);
        let mut last = [0.0; 3];
        for _ in 0..n {
            last = random_position(&mut rng);
            doc.apply_coalescing(
                Command::Move {
                    name: name.clone(),
                    position: last,
                },
                Some("mover:gesto"),
            )
            .unwrap_or_else(|e| panic!("{ctx}: movimento recusado: {e}"));
        }
        doc.end_gesture();
        let after = doc.model().clone();
        assert_eq!(after.objects.last().unwrap().position, last, "{ctx}");
        assert!(doc.undo(), "{ctx}");
        assert_eq!(
            doc.model(),
            &before,
            "{ctx}: gesto não virou um único passo"
        );
        assert!(doc.redo(), "{ctx}");
        assert_eq!(doc.model(), &after, "{ctx}: refazer do gesto difere");

        // A new gesture with the same key after end_gesture is a separate step.
        doc.end_gesture();
        doc.apply_coalescing(
            Command::Move {
                name: name.clone(),
                position: random_position(&mut rng),
            },
            Some("mover:gesto"),
        )
        .unwrap();
        doc.end_gesture();
        assert!(doc.undo(), "{ctx}");
        assert_eq!(
            doc.model(),
            &after,
            "{ctx}: gesto após end_gesture fundiu com o anterior"
        );

        // Different keys: each move is its own step, so undoing walks back one by one.
        let start = doc.model().clone();
        let mut states = Vec::new();
        let m = rng.random_range(2..=6);
        for i in 0..m {
            doc.apply_coalescing(
                Command::Move {
                    name: name.clone(),
                    position: random_position(&mut rng),
                },
                Some(&format!("mover:{i}")),
            )
            .unwrap();
            states.push(doc.model().clone());
        }
        for k in (0..m).rev() {
            assert!(doc.undo(), "{ctx}");
            let expected = if k == 0 { &start } else { &states[k - 1] };
            assert_eq!(
                doc.model(),
                expected,
                "{ctx}: passo {k} de chaves distintas fundiu"
            );
        }
        assert_eq!(doc.model(), &start, "{ctx}");
    }
}

#[test]
fn save_and_open_round_trip() {
    let (mut doc, _) = run_sequence(7, STEPS);
    assert!(doc.is_dirty());
    let path: PathBuf =
        std::env::temp_dir().join(format!("utsim-sequences-{}-{}.ron", std::process::id(), 7));

    doc.save_as(&path).expect("save_as writes the temp file");
    assert!(!doc.is_dirty(), "salvar deixou o documento sujo");
    assert_eq!(doc.path(), Some(path.as_path()));

    let mut reopened = Document::open(&path).expect("the saved file opens");
    assert_eq!(reopened.model(), doc.model());
    assert!(!reopened.is_dirty(), "abrir deixou o documento sujo");

    let name = reopened.model().objects.first().map(|o| o.name.clone());
    if let Some(name) = name {
        reopened
            .apply(Command::Move {
                name,
                position: [1.0, 2.0, 3.0],
            })
            .unwrap();
        assert!(
            reopened.is_dirty(),
            "alteração não marcou o documento como sujo"
        );
        reopened.save().expect("save uses the opened path");
        assert!(!reopened.is_dirty());
        assert_eq!(Model::load(&path).unwrap(), *reopened.model());
    }

    let _ = std::fs::remove_file(&path);
}
