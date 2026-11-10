//! Robustness: random and mutated model files never panic and never hang.
//!
//! Every input goes through `Model::parse` -> `validate` -> `build` + a short run (and a
//! replay). The outcome must be Ok or a Portuguese error; a panic, a hang or an English
//! message is a bug.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use proptest::prelude::*;
use utsim_model::Model;

/// Prefixes of every error `ModelError` can produce (all Portuguese).
const PREFIXES: &[&str] = &[
    "erro de sintaxe",
    "modelo inválido",
    "modelo no formato",
    "erro de modelo",
    "não foi possível",
];

fn examples() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "ron") {
            out.push(std::fs::read_to_string(path).unwrap());
        }
    }
    out.sort();
    out
}

fn check_error(e: &dyn std::fmt::Display, stage: &str) -> Result<(), String> {
    let text = e.to_string();
    if PREFIXES.iter().any(|p| text.starts_with(p)) && !text.contains("panicked") {
        Ok(())
    } else {
        Err(format!("{stage}: mensagem fora do padrão: {text}"))
    }
}

/// The whole pipeline on one text. Returns Err with a description on a contract breach.
fn pipeline(text: &str) -> Result<(), String> {
    pipeline_in(text, &AtomicUsize::new(0))
}

/// Phases of the pipeline, in order: a hang names the one it was in.
const PHASES: [&str; 4] = [
    "leitura e ajuste dos dados (Model::parse)",
    "validação",
    "rodada",
    "replay",
];

/// [`pipeline`], writing the index of the phase it is in to `phase` (for the hang report).
fn pipeline_in(text: &str, phase: &AtomicUsize) -> Result<(), String> {
    phase.store(0, Ordering::SeqCst);
    let mut model = match Model::parse(text) {
        Ok(m) => m,
        Err(e) => return check_error(&e, "parse"),
    };
    phase.store(1, Ordering::SeqCst);
    if let Err(e) = model.validate() {
        if let utsim_model::ModelError::Invalid(list) = &e {
            if list.is_empty() {
                return Err("validate: lista de erros vazia".into());
            }
        }
        return check_error(&e, "validate");
    }
    // Keep the run short: the point is construction and the first events. The budget is
    // a number of events, not seconds of the machine's clock, so a loaded machine gives the
    // same verdict as an idle one.
    model.run.stop_time = model.run.stop_time.min(100.0);
    model.run.warmup = model.run.warmup.min(model.run.stop_time / 2.0);
    model.run.replications = 1;
    model.run.sample_interval = model.run.sample_interval.map(|v| v.max(1.0));
    phase.store(2, Ordering::SeqCst);
    let mut sim = model.build(0);
    sim.limit_events(EVENT_LIMIT);
    if let Err(e) = model.drive(&mut sim) {
        check_error(&e, "run")?;
    }
    // The replay: the same run with the trace on, then the static part of the replay.
    phase.store(3, Ordering::SeqCst);
    let mut sim = model.build(0);
    sim.enable_trace();
    sim.limit_events(EVENT_LIMIT);
    if let Err(e) = model.drive(&mut sim) {
        check_error(&e, "replay")?;
    }
    let _ = model.empty_replay(0);
    Ok(())
}

/// Events each run of the pipeline may process before it stops with an error.
const EVENT_LIMIT: u64 = 2_000_000;

/// Wall-clock fallback for what the event budget cannot stop: a loop inside one event, or
/// a phase without events (reading the model and fitting its data). The report names the
/// phase.
const WALL_LIMIT: Duration = Duration::from_secs(300);

/// File that holds the input of the running case of this test thread until it ends: when
/// the process dies without a panic (out of memory, stack overflow), the file left in
/// `target/robustez/` is the input that killed it.
fn case_file() -> std::path::PathBuf {
    let name: String = std::thread::current()
        .name()
        .unwrap_or("principal")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/robustez");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{name}.ron"))
}

/// Runs `pipeline` in a thread: a panic or a hang (> [`WALL_LIMIT`]) is reported, not
/// propagated. The input stays in [`case_file`] while the case runs.
fn guarded(text: &str) -> Result<(), String> {
    // Debugging aid: an abort (out of memory, stack overflow) kills the process and leaves
    // no message, so `UTSIM_FUZZ_TRACE=file` keeps the last input on disk.
    if let Ok(path) = std::env::var("UTSIM_FUZZ_TRACE") {
        let _ = std::fs::write(path, text);
    }
    let file = case_file();
    let _ = std::fs::write(&file, text);
    let result = guarded_run(text);
    // The case ended inside the process (passed or failed with a message): no file left.
    let _ = std::fs::remove_file(&file);
    result
}

fn guarded_run(text: &str) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    let owned = text.to_string();
    let phase = Arc::new(AtomicUsize::new(0));
    let in_thread = Arc::clone(&phase);
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| pipeline_in(&owned, &in_thread));
        let _ = tx.send(match result {
            Ok(r) => r,
            Err(p) => {
                let msg = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                Err(format!("PÂNICO: {msg}"))
            }
        });
    });
    match rx.recv_timeout(WALL_LIMIT) {
        Ok(r) => r.map_err(|e| format!("{e}\n--- entrada ---\n{text}")),
        Err(_) => {
            // The stuck thread cannot be stopped: report and end the whole test process.
            // `eprintln!` is captured by the test harness and lost on `exit`, so the input
            // goes straight to the process stderr and to a file in `target/`.
            use std::io::Write;
            let fase = PHASES
                .get(phase.load(Ordering::SeqCst))
                .copied()
                .unwrap_or("?");
            let msg = format!(
                "TRAVOU (> {} s) na fase: {fase}\n--- entrada ---\n{text}\n",
                WALL_LIMIT.as_secs()
            );
            let _ = std::io::stderr().lock().write_all(msg.as_bytes());
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("robustez_travou.ron"), text);
            std::process::exit(101);
        }
    }
}

/// Byte ranges of numeric literals (not part of an identifier).
fn numbers(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let starts = b[i].is_ascii_digit()
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'));
        if starts {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.' || b[j] == b'e') {
                j += 1;
            }
            out.push((i, j));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

const NASTY: &[&str] = &[
    "NaN",
    "inf",
    "-inf",
    "-1.0",
    "-0.0",
    "0",
    "0.0",
    "1e308",
    "1e-308",
    "99999999999",
    "18446744073709551615",
    "4294967296",
    "1.7976931348623157e308",
    "0.000000001",
];

/// Applies the mutation numbered `op` (with random parameters `a`, `b`).
fn mutate(text: &str, op: u8, a: usize, b: usize) -> String {
    let nums = numbers(text);
    let names: Vec<(usize, usize)> = text
        .match_indices("name: \"")
        .map(|(i, m)| {
            let s = i + m.len();
            (s, s + text[s..].find('"').unwrap_or(0))
        })
        .collect();
    let at_char = |k: usize| {
        let mut k = k % (text.len().max(1));
        while !text.is_char_boundary(k) {
            k -= 1;
        }
        k
    };
    match op % 8 {
        // Truncation.
        0 => text[..at_char(a)].to_string(),
        // Deleted character.
        1 => {
            let k = at_char(a);
            let mut s = text.to_string();
            if k < s.len() {
                s.remove(k);
            }
            s
        }
        // Number replaced by a nasty value.
        2 if !nums.is_empty() => {
            let (s, e) = nums[a % nums.len()];
            format!("{}{}{}", &text[..s], NASTY[b % NASTY.len()], &text[e..])
        }
        // Two numbers swapped.
        3 if nums.len() >= 2 => {
            let (i, j) = (a % nums.len(), b % nums.len());
            if i == j {
                return text.to_string();
            }
            let ((s1, e1), (s2, e2)) = (nums[i.min(j)], nums[i.max(j)]);
            format!(
                "{}{}{}{}{}",
                &text[..s1],
                &text[s2..e2],
                &text[e1..s2],
                &text[s1..e1],
                &text[e2..]
            )
        }
        // Empty name.
        4 if !names.is_empty() => {
            let (s, e) = names[a % names.len()];
            format!("{}{}", &text[..s], &text[e..])
        }
        // Duplicate name: one object takes another's name.
        5 if names.len() >= 2 => {
            let (s1, e1) = names[a % names.len()];
            let (s2, e2) = names[b % names.len()];
            if s1 == s2 {
                return text.to_string();
            }
            let other = text[s2..e2].to_string();
            let mut s = text.to_string();
            s.replace_range(s1..e1, &other);
            s
        }
        // Unknown field.
        6 => text.replacen("Model(", "Model(campo_desconhecido: 1, ", 1),
        // Garbage inserted.
        _ => {
            let k = at_char(a);
            let junk = [
                "(", ")", "]", "[", "\"", ",", "Some(", "None", "::", "0x", "'",
            ];
            format!("{}{}{}", &text[..k], junk[b % junk.len()], &text[k..])
        }
    }
}

/// Inputs that once hung, ran out of memory or panicked. Each must now be refused with a
/// message (validation) or end with a model error, never abort.
#[test]
fn known_findings_are_errors() {
    let wrap = |objects: &str, connections: &str, extra: &str| {
        format!(
            "Model(name: \"m\", run: (stop_time: 100.0), objects: [{objects}],
             connections: [{connections}] {extra})"
        )
    };
    let src = "(name: \"a\", kind: Source(interarrival: Exp(mean: 1.0)))";
    let sink = "(name: \"s\", kind: Sink)";
    let cases = [
        // Source that would create more than 1e9 items.
        wrap(
            "(name: \"a\", kind: Source(interarrival: Exp(mean: 1e-300))), (name: \"s\", kind: Sink)",
            "(\"a\", \"s\")",
            "",
        ),
        // Rack with an absurd number of positions.
        wrap(
            &format!(
                "{src}, (name: \"r\", kind: Rack(bays: 4294967296, levels: 4294967296)), {sink}"
            ),
            "(\"a\", \"r\"), (\"r\", \"s\")",
            "",
        ),
        // Separator creating billions of copies.
        wrap(
            &format!(
                "{src}, (name: \"p\", kind: Separator(mode: Split(99999999999), process: Const(1.0))), {sink}"
            ),
            "(\"a\", \"p\"), (\"p\", \"s\")",
            "",
        ),
        // Processor asking for billions of operators.
        wrap(
            &format!(
                "{src}, (name: \"t\", kind: Transporter(speed: 1.0)), (name: \"p\", kind: Processor(process: Const(1.0), operators: Some(OperatorUse(team: \"t\", count: 99999999999)))), {sink}"
            ),
            "(\"a\", \"p\"), (\"p\", \"s\")",
            "",
        ),
    ];
    for text in cases {
        let err = Model::parse(&text).unwrap().validate().unwrap_err();
        assert!(err.to_string().starts_with("modelo inválido"), "{err}");
    }
    // A flow source with a zero interval and no limit filled memory with tokens.
    let flow = "Model(name: \"m\", run: (stop_time: 10.0), objects: [(name: \"s\", kind: Sink)],
        flows: [(name: \"f\", activities: [
            (name: \"chegada\", kind: Source(interarrival: Exp(mean: 0.0)), next: [\"fim\"]),
            (name: \"fim\", kind: Sink)])])";
    guarded(flow).unwrap();
    assert!(
        Model::parse(flow).unwrap().validate().is_err(),
        "fluxo com intervalo zero deve ser inválido"
    );
    // The largest f64 among the values of a data entry (examples/dados_reais.ron mutated):
    // the bootstrap samples summed past the largest f64 and the refits got an infinite
    // mean, whose quantiles crawled for tens of minutes. Now the fit refuses those values
    // and the empirical choice ends at once.
    let values = "0.3295, 0.2603, 1.7976931348623157e308, 0.229, 0.4144, 0.3463, 0.2232, 0.403, \
        0.215, 0.3735, 0.2279, 0.2363, 0.3698, 0.5307, 0.2495, 0.2893, 0.451, 0.5791, 0.4308, \
        0.3587, 0.5905, 0.2186, 0.5434, 0.3158, 0.2577, 0.2471, 0.3234, 0.5265, 0.2723, 0.4326";
    let dados = format!(
        "Model(name: \"m\", run: (stop_time: 100.0), objects: [{src},
            (name: \"p\", kind: Processor(process: Const(1.0), setup: None)), {sink}],
         connections: [(\"a\", \"p\"), (\"p\", \"s\")],
         data: [(field: \"p.setup\", values: [{values}], choose: Empirical)])"
    );
    let started = std::time::Instant::now();
    guarded(&dados).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(120),
        "o ajuste com 1.8e308 nos dados levou {:?}",
        started.elapsed()
    );
}

#[test]
fn all_examples_pass_the_pipeline() {
    for text in examples() {
        // Experiment and optimisation files are not models: they may fail with a message.
        guarded(&text).unwrap();
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 600, ..ProptestConfig::default() })]

    #[test]
    fn mutated_examples_never_panic(
        pick in 0usize..64,
        ops in proptest::collection::vec((any::<u8>(), any::<u16>(), any::<u16>()), 1..4),
    ) {
        let all = examples();
        let mut text = all[pick % all.len()].clone();
        for (op, a, b) in ops {
            text = mutate(&text, op, a as usize * 7, b as usize);
        }
        prop_assert!(guarded(&text).is_ok(), "{}", guarded(&text).unwrap_err());
    }

    #[test]
    fn random_text_never_panics(text in "\\PC{0,200}") {
        prop_assert!(guarded(&text).is_ok(), "{}", guarded(&text).unwrap_err());
    }

    #[test]
    fn random_ron_shapes_never_panic(
        stop in prop_oneof![Just("1.0".to_string()), Just("NaN".to_string()), Just("-5.0".to_string()), Just("1e308".to_string()), Just("0.0".to_string())],
        mean in prop_oneof![Just("1.0".to_string()), Just("0.0".to_string()), Just("-1.0".to_string()), Just("NaN".to_string()), Just("1e-300".to_string()), Just("1e300".to_string())],
        cap in prop_oneof![Just("None".to_string()), Just("Some(0)".to_string()), Just("Some(1)".to_string()), Just("Some(18446744073709551615)".to_string())],
        name in prop_oneof![Just("".to_string()), Just("a".to_string()), Just("  ".to_string())],
    ) {
        let text = format!(
            "Model(name: \"m\", run: (stop_time: {stop}, replications: 1), objects: [
               (name: \"{name}\", kind: Source(interarrival: Exp(mean: {mean}))),
               (name: \"q\", kind: Queue(capacity: {cap})),
               (name: \"s\", kind: Sink)],
             connections: [(\"{name}\", \"q\"), (\"q\", \"s\")])"
        );
        prop_assert!(guarded(&text).is_ok(), "{}", guarded(&text).unwrap_err());
    }
}
