use std::path::PathBuf;

use utsim_core::summarize;
use utsim_model::Model;
use utsim_model::experiment::Experiment;

const MODEL: &str = r#"Model(
    name: "M/M/1",
    run: (stop_time: 2000.0, warmup: 100.0, seed: 7, replications: 4),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
        (name: "fila", kind: Queue()),
        (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
        (name: "saida", kind: Sink),
    ],
    connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
)"#;

const EXPERIMENT: &str = r#"Experiment(
    name: "teste",
    model: "m.ron",
    scenarios: [
        (name: "base"),
        (name: "rapida", set: { "maquina.process": "Exp(mean: 0.5)" }),
    ],
    measures: ["fila.content.mean", "maquina.output"],
)"#;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("utsim-exp-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn faster_machine_lowers_queue_and_base_matches_direct_run() {
    let dir = temp_dir("a");
    std::fs::write(dir.join("m.ron"), MODEL).unwrap();
    std::fs::write(dir.join("e.ron"), EXPERIMENT).unwrap();

    let result = Experiment::load(dir.join("e.ron")).unwrap().run().unwrap();
    let key = "fila.content.mean";
    let base = &result.scenarios[0];
    let fast = &result.scenarios[1];
    assert_eq!(base.summary.len(), 2, "only the requested measures are kept");
    assert!(fast.summary[key].mean < base.summary[key].mean);

    let direct = summarize(&Model::parse(MODEL).unwrap().run_replications().unwrap());
    for (k, s) in &base.summary {
        assert_eq!(s.mean, direct[k].mean, "{k}");
        assert_eq!(s.std_dev, direct[k].std_dev, "{k}");
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn replications_override_and_error_names_scenario() {
    let dir = temp_dir("b");
    std::fs::write(dir.join("m.ron"), MODEL).unwrap();
    let text = r#"Experiment(name: "e", model: "m.ron", replications: 2,
        scenarios: [(name: "ruim", set: {"maquina.nada": "1"})])"#;
    let err = Experiment::parse(text, &dir).unwrap().run().unwrap_err().to_string();
    assert!(err.contains("cenário 'ruim'") && err.contains("nada"), "{err}");

    let ok = text.replace(r#"set: {"maquina.nada": "1"}"#, "");
    let result = Experiment::parse(&ok, &dir).unwrap().run().unwrap();
    assert_eq!(result.scenarios[0].summary["fila.content.mean"].n, 2);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn patches_phase1_block_fields() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let exp = utsim_model::experiment::Experiment::parse(
        r#"Experiment(
            name: "paletes",
            model: "montagem_paletes.ron",
            replications: 1,
            scenarios: [
                (name: "base"),
                (name: "lote", set: { "montagem.components": "[2]", "montagem.mode": "Join" }),
            ],
            measures: [],
        )"#,
        &dir,
    )
    .unwrap();
    exp.run().expect("cenários com campos do Combiner rodam");
}
