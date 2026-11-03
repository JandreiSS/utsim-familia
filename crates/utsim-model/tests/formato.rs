//! Every shipped example is a format-1 file (no `format`, or `format: 1`): read through the
//! `1 → 2` migration, it comes out in format 2, with `run.movement: Legacy` when it has
//! mobile resources: the behaviour the golden snapshots lock. Also: format 2 round trip.

use utsim_model::{FORMAT_VERSION, Model, Movement};

fn model_texts() -> Vec<(std::path::PathBuf, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "ron") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        // Experiments and optimisation problems are not models.
        let first = text.lines().find(|l| !l.trim_start().starts_with("//"));
        if first.is_some_and(|l| l.starts_with("Model")) {
            out.push((path, text));
        }
    }
    out.sort();
    out
}

#[test]
fn examples_are_format_1_and_migrate_to_legacy() {
    assert_eq!(FORMAT_VERSION, 2);
    let mut checked = 0;
    let mut with_mobile = 0;
    for (path, text) in model_texts() {
        assert!(
            !text.contains("format: 2"),
            "{}: os exemplos ficam no formato 1 (testam a migração)",
            path.display()
        );
        let model = Model::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(model.format, Some(2), "{}", path.display());
        let mobile = model.objects.iter().any(|o| model.movement_of(o).is_some());
        // Models without mobile resources have nothing to keep: no mode written.
        let want = mobile.then_some(Movement::Legacy);
        assert_eq!(model.run.movement, want, "{}", path.display());
        if mobile {
            with_mobile += 1;
        }
        for o in &model.objects {
            if let Some(m) = model.movement_of(o) {
                assert_eq!(m, Movement::Legacy, "{}: {}", path.display(), o.name);
            }
        }
        checked += 1;
    }
    assert!(checked >= 10, "só {checked} exemplos verificados");
    assert!(
        with_mobile >= 5,
        "só {with_mobile} exemplos com recursos móveis"
    );
}

#[test]
fn migrated_model_saved_in_format_2_reads_back_the_same() {
    let config = ron::ser::PrettyConfig::new()
        .struct_names(true)
        .extensions(ron::extensions::Extensions::IMPLICIT_SOME);
    for (path, text) in model_texts() {
        let model = Model::parse(&text).unwrap();
        let saved = ron::ser::to_string_pretty(&model, config.clone()).unwrap();
        assert!(saved.contains("format: 2"), "{}", path.display());
        assert_eq!(
            saved.contains("movement: Legacy"),
            model.run.movement.is_some(),
            "{}",
            path.display()
        );
        let again = Model::parse(&saved).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(again, model, "{}", path.display());
    }
}

#[test]
fn transporter_override_wins_over_the_model() {
    let text = r#"Model(
        format: 2,
        name: "m",
        run: (stop_time: 10.0, movement: Legacy),
        objects: [
            (name: "a", kind: Transporter(speed: 1.0, movement: Free, clearance: 0.4)),
            (name: "b", kind: Transporter(speed: 1.0)),
        ],
    )"#;
    let m = Model::parse(text).unwrap();
    assert_eq!(m.movement_of(&m.objects[0]), Some(Movement::Free));
    assert_eq!(m.movement_of(&m.objects[1]), Some(Movement::Legacy));
    assert_eq!(m.clearance_of(&m.objects[0]), 0.4);
}
