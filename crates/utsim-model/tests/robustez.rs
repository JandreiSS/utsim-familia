//! Robustness: random and mutated model files never panic and never hang.
//!
//! Every input goes through `Model::parse` -> `validate` -> `build` + a short run (and a
//! replay). The outcome must be Ok or a Portuguese error; a panic, a hang or an English
//! message is a bug.

use std::sync::mpsc;
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
    let mut model = match Model::parse(text) {
        Ok(m) => m,
        Err(e) => return check_error(&e, "parse"),
    };
    if let Err(e) = model.validate() {
        if let utsim_model::ModelError::Invalid(list) = &e {
            if list.is_empty() {
                return Err("validate: lista de erros vazia".into());
            }
        }
        return check_error(&e, "validate");
    }
    // Keep the run short: the point is construction and the first events.
    model.run.stop_time = model.run.stop_time.min(100.0);
    model.run.warmup = model.run.warmup.min(model.run.stop_time / 2.0);
    model.run.replications = 1;
    model.run.sample_interval = model.run.sample_interval.map(|v| v.max(1.0));
    match model.run_replications() {
        Ok(_) => {}
        Err(e) => check_error(&e, "run")?,
    }
    match model.replay(0) {
        Ok(_) => {}
        Err(e) => check_error(&e, "replay")?,
    }
    Ok(())
}

/// Runs `pipeline` in a thread: a panic or a hang (> 60 s) is reported, not propagated.
fn guarded(text: &str) -> Result<(), String> {
    // Debugging aid: an abort (out of memory, stack overflow) kills the process and leaves
    // no message, so `UTSIM_FUZZ_TRACE=file` keeps the last input on disk.
    if let Ok(path) = std::env::var("UTSIM_FUZZ_TRACE") {
        let _ = std::fs::write(path, text);
    }
    let (tx, rx) = mpsc::channel();
    let owned = text.to_string();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| pipeline(&owned));
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
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(r) => r.map_err(|e| format!("{e}\n--- entrada ---\n{text}")),
        Err(_) => {
            // The stuck thread cannot be stopped: report and end the whole test process.
            // `eprintln!` is captured by the test harness and lost on `exit`, so the input
            // goes straight to the process stderr and to a file in `target/`.
            use std::io::Write;
            let msg = format!("TRAVOU (> 60 s)\n--- entrada ---\n{text}\n");
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
