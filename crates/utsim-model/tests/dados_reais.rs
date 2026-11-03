//! Campos de distribuição alimentados por dados reais (`data: [..]`, módulo `dados`).

use std::path::{Path, PathBuf};

use utsim_core::Dist;
use utsim_core::rng::stream;
use utsim_fit::auto::{self, AutoConfig, Choice, Criterion};
use utsim_model::Model;
use utsim_model::dados::{self, DataInput, DataStatus};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn example_models() -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = std::fs::read_dir(examples())
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            let text = std::fs::read_to_string(&p).ok()?;
            (p.extension()? == "ron" && text.contains("Model(")).then_some((p, text))
        })
        .collect();
    out.sort();
    out
}

/// A fresh folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("utsim_dados_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample(dist: &Dist, n: usize, seed: u64) -> Vec<f64> {
    let mut rng = stream(seed, 0, 0);
    (0..n).map(|_| dist.sample(&mut rng)).collect()
}

const LINE: &str = r#"Model(
    name: "linha",
    run: (stop_time: 1000.0),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0), labels: {"peso": Const(1.0)})),
        (name: "torno", kind: Processor(process: Const(1.0)), downtime: Some((mtbf: Exp(mean: 100.0), mttr: Const(5.0)))),
        (name: "saida", kind: Sink),
    ],
    connections: [("chegada", "torno"), ("torno", "saida")],
    DATA
)"#;

fn line_with(data: &str) -> String {
    LINE.replace("DATA", data)
}

fn ron_values(v: &[f64]) -> String {
    let items: Vec<String> = v.iter().map(|x| format!("{x:?}")).collect();
    format!("[{}]", items.join(", "))
}

fn quick() -> &'static str {
    "bootstrap: Some(49)"
}

fn process(m: &Model) -> Dist {
    dados::get_dist(m, "torno.process").unwrap().unwrap()
}

#[test]
fn old_models_are_untouched() {
    // Golden: every example parses exactly as the plain RON reader does, without `data`,
    // and saving it does not write the new field.
    let options =
        ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    for (path, text) in example_models() {
        let Ok(plain) = options.from_str::<Model>(&text) else {
            continue;
        };
        if !plain.data.is_empty() {
            continue;
        }
        let parsed = Model::load(&path).unwrap();
        // Loading also migrates format 1 to 2 (`format` and `run.movement`); that migration
        // has its own tests, so only the data fields are compared here.
        let mut plain = plain;
        plain.format = parsed.format;
        plain.run.movement = parsed.run.movement;
        assert_eq!(parsed, plain, "{}", path.display());
        assert!(!ron::ser::to_string(&parsed).unwrap().contains("data:"));
    }
}

#[test]
fn every_distribution_field_of_the_examples_can_be_read_and_written() {
    let mut total = 0;
    for (path, text) in example_models() {
        let Ok(model) = Model::parse_in(&text, path.parent()) else {
            continue;
        };
        let fields = dados::dist_fields(&model);
        total += fields.len();
        if path.ends_with("fluxo_docas.ron") {
            assert!(fields.iter().any(|f| f.starts_with("flows.")), "{fields:?}");
        }
        for f in &fields {
            let current = dados::get_dist(&model, f)
                .unwrap_or_else(|e| panic!("{}: {f}: {e}", path.display()));
            let mut copy = model.clone();
            let probe = Dist::Const(4.25);
            dados::set_dist(&mut copy, f, &probe)
                .unwrap_or_else(|e| panic!("{}: {f}: {e}", path.display()));
            assert_eq!(dados::get_dist(&copy, f).unwrap(), Some(probe));
            if let Some(d) = current {
                dados::set_dist(&mut copy, f, &d).unwrap();
                assert_eq!(copy, model, "{}: {f}", path.display());
            }
        }
    }
    assert!(total > 60, "só {total} campos de distribuição nos exemplos");
    let m = Model::parse(&line_with("")).unwrap();
    let fields = dados::dist_fields(&m);
    for f in [
        "chegada.interarrival",
        "chegada.labels.peso",
        "torno.process",
        "torno.setup",
        "torno.downtime.mtbf",
        "torno.downtime.mttr",
    ] {
        assert!(fields.contains(&f.to_string()), "{f} em {fields:?}");
    }
}

#[test]
fn field_with_data_equals_the_fitted_distribution() {
    let values = sample(
        &Dist::Gamma {
            shape: 3.0,
            scale: 1.5,
        },
        200,
        21,
    );
    let text = line_with(&format!(
        "data: [(field: \"torno.process\", values: {}, {})]",
        ron_values(&values),
        quick()
    ));
    let m = Model::parse(&text).unwrap();
    let input = &m.data[0];
    let direct = auto::auto_fit("torno.process", &values, &input.config()).unwrap();
    assert_eq!(process(&m), direct.dist);
    assert_eq!(input.status, DataStatus::Fitted);
    let rec = input.fitted.as_ref().unwrap();
    assert_eq!(rec.hash, auto::data_hash(&values));
    assert_eq!(rec.family, direct.family);
    assert_eq!(rec.n, 200);
    m.validate().unwrap();
    m.run_replications().unwrap();
}

#[test]
fn fit_is_deterministic_and_the_record_is_reused() {
    let dir = scratch("determinismo");
    let values = sample(&Dist::LogNormal { mean: 4.0, sd: 1.5 }, 150, 22);
    let csv: String = std::iter::once("tempo".to_string())
        .chain(values.iter().map(|v| format!("{v}")))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.join("tempos.csv"), &csv).unwrap();
    let text = line_with(&format!(
        "data: [(field: \"torno.process\", source: \"tempos.csv\", {})]",
        quick()
    ));
    std::fs::write(dir.join("m.ron"), &text).unwrap();
    let a = Model::load(dir.join("m.ron")).unwrap();
    let b = Model::load(dir.join("m.ron")).unwrap();
    assert_eq!(process(&a), process(&b));
    assert_eq!(a.data[0].fitted, b.data[0].fitted);

    // Saved with the record: the next load reuses it (and gives the same distribution).
    let saved = ron::ser::to_string(&a).unwrap();
    assert!(saved.contains("fitted:") && saved.contains("fnv1a64:"));
    std::fs::write(dir.join("m.ron"), &saved).unwrap();
    let c = Model::load(dir.join("m.ron")).unwrap();
    assert_eq!(c.data[0].status, DataStatus::Reused);
    assert_eq!(process(&c), process(&a));
    assert!(c.warnings().iter().all(|w| !w.contains("mudaram")));

    // Other settings: refitted without a "data changed" warning.
    let other = saved.replace("criterion:Aic", "criterion:Ks");
    assert_ne!(other, saved);
    let d = Model::parse_in(&other, Some(&dir)).unwrap();
    assert_eq!(d.data[0].status, DataStatus::SettingsChanged);
}

#[test]
fn changed_data_is_refitted_with_a_warning() {
    let dir = scratch("mudanca");
    let write = |v: &[f64]| {
        let csv: Vec<String> = v.iter().map(|x| x.to_string()).collect();
        std::fs::write(dir.join("t.csv"), csv.join("\n")).unwrap();
    };
    write(&sample(&Dist::Exp { mean: 2.0 }, 120, 23));
    let text = line_with(&format!(
        "data: [(field: \"torno.process\", source: \"t.csv\", {})]",
        quick()
    ));
    let first = Model::parse_in(&text, Some(&dir)).unwrap();
    let saved = ron::ser::to_string(&first).unwrap();

    write(&sample(
        &Dist::Normal {
            mean: 20.0,
            sd: 2.0,
        },
        120,
        24,
    ));
    let second = Model::parse_in(&saved, Some(&dir)).unwrap();
    assert!(matches!(
        second.data[0].status,
        DataStatus::DataChanged { .. }
    ));
    assert_ne!(process(&second), process(&first));
    let warnings = second.warnings();
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("torno.process: os dados mudaram")),
        "{warnings:?}"
    );

    // File gone: the recorded fit is used, with a warning.
    std::fs::remove_file(dir.join("t.csv")).unwrap();
    let third = Model::parse_in(&saved, Some(&dir)).unwrap();
    assert!(matches!(
        third.data[0].status,
        DataStatus::RecordOnly { .. }
    ));
    assert_eq!(process(&third), process(&first));
    assert!(third.warnings().iter().any(|w| w.contains("último ajuste")));
    // Without a record, a missing file is an error.
    assert!(Model::parse_in(&text, Some(&dir)).is_err());
}

#[test]
fn bad_data_is_flagged() {
    // Few points.
    let text = line_with(&format!(
        "data: [(field: \"torno.process\", values: [3.1, 2.4, 5.0, 4.2, 3.3], {})]",
        quick()
    ));
    let m = Model::parse(&text).unwrap();
    let w = m.warnings();
    assert!(w.iter().any(|w| w.contains("Muito poucos")), "{w:?}");

    // A clear upward trend.
    let mut rng = stream(25, 0, 0);
    let trend: Vec<f64> = (0..120)
        .map(|i| 5.0 + 0.1 * i as f64 + Dist::Exp { mean: 1.0 }.sample(&mut rng))
        .collect();
    let text = line_with(&format!(
        "data: [(field: \"chegada.interarrival\", values: {}, {})]",
        ron_values(&trend),
        quick()
    ));
    let m = Model::parse(&text).unwrap();
    let w = m.warnings();
    assert!(
        w.iter()
            .any(|w| w.starts_with("chegada.interarrival (dados)") && w.contains("tendência")),
        "{w:?}"
    );
}

#[test]
fn choices_and_errors() {
    let values = sample(&Dist::Exp { mean: 2.0 }, 100, 26);
    let with = |extra: &str, field: &str| {
        line_with(&format!(
            "data: [(field: \"{field}\", values: {}, {}, {extra})]",
            ron_values(&values),
            quick()
        ))
    };
    let m = Model::parse(&with("choose: Empirical", "torno.downtime.mttr")).unwrap();
    assert!(matches!(
        dados::get_dist(&m, "torno.downtime.mttr").unwrap(),
        Some(Dist::Empirical(_))
    ));
    let m = Model::parse(&with("choose: Family(\"Weibull\")", "torno.setup")).unwrap();
    assert!(matches!(
        dados::get_dist(&m, "torno.setup").unwrap(),
        Some(Dist::Weibull { .. })
    ));

    for (extra, field, expected) in [
        ("", "torno.capacity", "não é um campo de distribuição"),
        ("", "fresa.process", "objeto 'fresa' não existe"),
        ("", "torno.tempo", "não tem o campo 'tempo'"),
        (
            "choose: Family(\"Cauchy\")",
            "torno.process",
            "desconhecida",
        ),
    ] {
        let err = Model::parse(&with(extra, field)).unwrap_err().to_string();
        assert!(err.contains(expected), "{field}: {err}");
    }
    let twice = line_with(
        "data: [(field: \"torno.process\", values: [1.0, 2.0]), (field: \"torno.process\", values: [1.0, 2.0])]",
    );
    assert!(
        Model::parse(&twice)
            .unwrap_err()
            .to_string()
            .contains("mais de uma vez")
    );
    let neither = line_with("data: [(field: \"torno.process\")]");
    assert!(
        Model::parse(&neither)
            .unwrap_err()
            .to_string()
            .contains("informe `source`")
    );
}

#[test]
fn reports_are_written_per_field_with_an_index() {
    let dir = scratch("relatorios");
    let values = sample(
        &Dist::Weibull {
            shape: 2.0,
            scale: 5.0,
        },
        120,
        27,
    );
    let text = line_with(&format!(
        "data: [(field: \"torno.process\", values: {v}, {q}), (field: \"chegada.labels.peso\", values: {v}, {q}, criterion: Ad)]",
        v = ron_values(&values),
        q = quick()
    ));
    let m = Model::parse(&text).unwrap();
    let (written, failed) = dados::write_reports(&m, None, &dir).unwrap();
    assert_eq!(failed, 0);
    assert_eq!(written.len(), 5);
    let index = std::fs::read_to_string(dir.join("index.html")).unwrap();
    assert!(index.contains("torno.process.html") && index.contains("chegada.labels.peso.html"));
    let report = std::fs::read_to_string(dir.join("torno.process.html")).unwrap();
    assert!(report.contains("Escolha para o campo") && report.contains("<svg"));
    // The report's choice is the field's distribution.
    let fit = auto::auto_fit(
        "torno.process",
        &values,
        &AutoConfig {
            choose: Choice::Best,
            criterion: Criterion::Aic,
            ..m.data[0].config()
        },
    )
    .unwrap();
    assert_eq!(process(&m), fit.dist);
}

#[test]
fn inputs_follow_renames_and_removals() {
    let mut m = Model::parse(&line_with(
        "data: [(field: \"torno.process\", values: [1.0, 2.0, 3.0], choose: Empirical)]",
    ))
    .unwrap();
    dados::rename_object(&mut m, "torno", "fresa");
    assert_eq!(m.data[0].field, "fresa.process");
    dados::remove_object(&mut m, "fresa");
    assert!(m.data.is_empty());
    let input = DataInput::with_source("x.y", "a.csv", Some("tempo"));
    assert_eq!(input.source_label(), "a.csv, coluna tempo");
}
