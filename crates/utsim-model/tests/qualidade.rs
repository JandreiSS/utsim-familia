//! Quality blocks (`docs/formato_qualidade_seguranca.md`, stages QS0 to QS2 of
//! `docs/plano_qualidade_e_seguranca.md`): defects, characteristics, inspection, rework
//! and scrap, checked against probabilities worked out by hand.

use std::path::Path;

use utsim_core::ItemRecord;
use utsim_model::Model;
use utsim_model::quality::present_in;

/// Records of the items that ended in the sink at index `sink` (object order).
fn ended_in(items: &[ItemRecord], sink: usize) -> Vec<&ItemRecord> {
    items
        .iter()
        .filter(|r| r.ended_at.is_some() && r.block == Some(sink))
        .collect()
}

fn num(r: &ItemRecord, label: &str) -> f64 {
    r.labels.get(label).and_then(|v| v.as_num()).unwrap_or(0.0)
}

/// `|p_hat - p|` below 4 standard errors of a proportion over `n` items.
fn close(p_hat: f64, p: f64, n: usize, what: &str) {
    let se = (p * (1.0 - p) / n as f64).sqrt();
    assert!(
        (p_hat - p).abs() < 4.0 * se,
        "{what}: {p_hat:.5} longe de {p:.5} (n = {n}, ep = {se:.5})"
    );
}

const PROCESS: &str = r#"
Model(
    name: "defeito",
    run: (stop_time: 1000.0, seed: 7, replications: 30),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
        (name: "torno", kind: Processor(process: Const(0.5)),
         quality: Some((
            defects: [
                (name: "risco", probability: 0.1),
                (name: "rebarba", probability: 0.1, factors: [Cycle(every: 10, first: 2, times: 5.0)]),
            ],
            characteristics: [
                (name: "diametro", value: Normal(mean: 10.0, sd: 1.0), lsl: Some(8.0), usl: Some(12.0)),
            ],
         ))),
        (name: "saida", kind: Sink),
    ],
    connections: [("chegada", "torno"), ("torno", "saida")],
)
"#;

#[test]
fn defect_rates_match_the_probabilities_over_30_replications() {
    let model = Model::parse(PROCESS).unwrap();
    let mut n = 0;
    let (mut risco, mut rebarba, mut fora) = (0, 0, 0);
    for r in 0..model.run.replications {
        let (_, items) = model.run_item_log(r).unwrap();
        for item in ended_in(&items, 2) {
            n += 1;
            risco += usize::from(num(item, "defeito.risco") > 0.0);
            rebarba += usize::from(num(item, "defeito.rebarba") > 0.0);
            fora += usize::from(num(item, "defeito.diametro") > 0.0);
            let d = num(item, "diametro");
            assert_eq!(
                num(item, "defeito.diametro") > 0.0,
                !(8.0..=12.0).contains(&d)
            );
            assert_eq!(
                num(item, "defeitos") as usize,
                present_in(&item.labels).len()
            );
        }
    }
    assert!(n > 29_000);
    close(risco as f64 / n as f64, 0.1, n, "risco");
    // Two items in every ten with 5 times the probability: 0.8 * 0.1 + 0.2 * 0.5.
    close(rebarba as f64 / n as f64, 0.18, n, "rebarba");
    // Outside mean ± 2 sd of a normal.
    close(fora as f64 / n as f64, 0.0455, n, "diametro fora");
}

#[test]
fn same_seed_same_labels() {
    let model = Model::parse(PROCESS).unwrap();
    let (_, a) = model.run_item_log(3).unwrap();
    let (_, b) = model.run_item_log(3).unwrap();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.labels, y.labels);
    }
    let (_, c) = model.run_item_log(4).unwrap();
    assert_ne!(
        a.iter().map(|r| num(r, "diametro")).collect::<Vec<_>>(),
        c.iter().map(|r| num(r, "diametro")).collect::<Vec<_>>()
    );
}

#[test]
fn quality_block_does_not_shift_process_times() {
    let with = PROCESS.replace("Const(0.5)", "Exp(mean: 0.5)");
    let without = r#"
Model(
    name: "defeito",
    run: (stop_time: 1000.0, seed: 7, replications: 30),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
        (name: "torno", kind: Processor(process: Exp(mean: 0.5))),
        (name: "saida", kind: Sink),
    ],
    connections: [("chegada", "torno"), ("torno", "saida")],
)
"#;
    let a = Model::parse(&with).unwrap().run_replications().unwrap();
    let b = Model::parse(without).unwrap().run_replications().unwrap();
    let wait = |r: &utsim_core::RunReport| {
        let b = r.blocks.iter().find(|b| b.name == "torno").unwrap();
        (b.output, b.staytime.mean)
    };
    assert_eq!(wait(&a[0]), wait(&b[0]));
}

const INSPECTION: &str = r#"
Model(
    name: "inspecao",
    run: (stop_time: 2000.0, seed: 11, replications: 30),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
        (name: "torno", kind: Processor(process: Const(0.2)),
         quality: Some((defects: [(name: "risco", probability: 0.2)]))),
        (name: "fila", kind: Queue()),
        (name: "inspecao", kind: Processor(process: Const(0.1)), output: ByLabel("inspecao"),
         inspection: Some((detect: 0.75, on_reject: Rework, max_rework: Some(1)))),
        (name: "cliente", kind: Sink, sink_role: Some(Cliente)),
        (name: "retrabalho", kind: Processor(process: Const(0.1)), rework: Some((fix: 0.5))),
        (name: "refugo", kind: Sink, sink_role: Some(Refugo)),
    ],
    connections: [
        ("chegada", "torno"), ("torno", "fila"), ("fila", "inspecao"),
        ("inspecao", "cliente"), ("inspecao", "retrabalho"), ("inspecao", "refugo"),
        ("retrabalho", "fila"),
    ],
)
"#;

#[test]
fn inspection_and_rework_match_the_hand_calculation() {
    // Defective 0.2, detected 0.75, fixed 0.5, at most one rework pass.
    //   escape on the first pass: 0.2 * 0.25 = 0.05
    //   detected, fixed, approved: 0.15 * 0.5 = 0.075
    //   detected, not fixed, detected again: scrap 0.075 * 0.75 = 0.05625
    //   detected, not fixed, missed: escape 0.075 * 0.25 = 0.01875
    let model = Model::parse(INSPECTION).unwrap();
    let (mut n, mut scrap, mut escape, mut first_pass) = (0, 0, 0, 0);
    for r in 0..model.run.replications {
        let (_, items) = model.run_item_log(r).unwrap();
        for item in &items {
            assert!(
                num(item, "passagens") <= 1.0,
                "passou do limite de retrabalho"
            );
        }
        let cliente = ended_in(&items, 4);
        let refugo = ended_in(&items, 6);
        n += cliente.len() + refugo.len();
        scrap += refugo.len();
        escape += cliente.iter().filter(|r| num(r, "defeitos") > 0.0).count();
        first_pass += cliente
            .iter()
            .filter(|r| num(r, "passagens") == 0.0)
            .count();
        for r in &refugo {
            assert_eq!(num(r, "passagens"), 1.0);
            assert!(num(r, "defeitos") > 0.0);
        }
    }
    assert!(n > 59_000);
    close(scrap as f64 / n as f64, 0.05625, n, "refugo");
    close(escape as f64 / n as f64, 0.06875, n, "escape");
    // First-pass yield: good items plus escapes on the first pass.
    close(
        first_pass as f64 / n as f64,
        0.85,
        n,
        "rendimento de primeira passagem",
    );
}

#[test]
fn measurement_error_misclassifies_near_the_limits() {
    let text = r#"
Model(
    name: "medicao",
    run: (stop_time: 5000.0, seed: 3, replications: 4),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
        (name: "torno", kind: Processor(process: Const(0.1)),
         quality: Some((characteristics: [
            (name: "d", value: Uniform(min: 9.0, max: 11.0), lsl: Some(9.5), usl: Some(10.5)),
         ]))),
        (name: "inspecao", kind: Processor(process: Const(0.1)), output: ByLabel("inspecao"),
         inspection: Some((measure: [(characteristic: "d", sigma: SIGMA)], on_reject: Scrap))),
        (name: "cliente", kind: Sink, sink_role: Some(Customer)),
        (name: "refugo", kind: Sink, sink_role: Some(Scrap)),
    ],
    connections: [
        ("chegada", "torno"), ("torno", "inspecao"),
        ("inspecao", "cliente"), ("inspecao", "refugo"),
    ],
)
"#;
    let wrong = |sigma: &str| {
        let model = Model::parse(&text.replace("SIGMA", sigma)).unwrap();
        let mut wrong = 0;
        for r in 0..model.run.replications {
            let (_, items) = model.run_item_log(r).unwrap();
            wrong += ended_in(&items, 3)
                .iter()
                .filter(|r| num(r, "defeitos") > 0.0)
                .count();
            wrong += ended_in(&items, 4)
                .iter()
                .filter(|r| num(r, "defeitos") == 0.0)
                .count();
        }
        wrong
    };
    assert_eq!(
        wrong("0.0"),
        0,
        "sem erro de medição não há erro de classificação"
    );
    assert!(wrong("0.1") > 0);
    assert!(wrong("0.2") > wrong("0.1"));
}

#[test]
fn validation_catches_wrong_inspection_wiring() {
    let bad = INSPECTION.replace("output: ByLabel(\"inspecao\"),", "output: FirstAvailable,");
    let e = Model::parse(&bad)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    assert!(e.contains("ByLabel"), "{e}");
    let few = INSPECTION.replace("        (\"inspecao\", \"refugo\"),\n", "");
    let few = few.replace(
        "(\"inspecao\", \"retrabalho\"), (\"inspecao\", \"refugo\"),",
        "(\"inspecao\", \"retrabalho\"),",
    );
    let e = Model::parse(&few)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    assert!(e.contains("3 saídas"), "{e}");
    let prob = INSPECTION.replace("probability: 0.2", "probability: 1.2");
    assert!(Model::parse(&prob).unwrap().validate().is_err());
}

#[test]
fn quality_blocks_absent_keep_old_models() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ron") {
            continue;
        }
        let Ok(model) = Model::load(&path) else {
            continue; // experiments and optimisations are other files
        };
        seen += 1;
        if model.zones.is_empty() && model.packs.is_empty() {
            let text = ron::to_string(&model).unwrap();
            for field in ["zones", "packs", "sink_role", "inspection", "rework", "ppe"] {
                assert!(
                    !text.contains(&format!("{field}:")),
                    "{}: {field}",
                    path.display()
                );
            }
        }
    }
    assert!(seen > 20);
}

#[test]
fn patch_keys_reach_the_quality_blocks() {
    use utsim_model::experiment::patch;
    let mut m = Model::parse(INSPECTION).unwrap();
    patch(&mut m, "inspecao.inspection.every", "5").unwrap();
    patch(&mut m, "inspecao.inspection.detect", "0.95").unwrap();
    patch(&mut m, "retrabalho.rework.fix", "0.8").unwrap();
    patch(
        &mut m,
        "torno.quality",
        "Some((defects: [(name: \"risco\", probability: 0.05)]))",
    )
    .unwrap();
    patch(&mut m, "torno.ppe", "[\"oculos\"]").unwrap();
    let i = m.objects[3].inspection.as_ref().unwrap();
    assert_eq!((i.every, i.detect), (5, 0.95));
    assert_eq!(m.objects[5].rework.as_ref().unwrap().fix, 0.8);
    assert_eq!(
        m.objects[1].quality.as_ref().unwrap().defects[0].probability,
        0.05
    );
    assert_eq!(m.objects[1].ppe, vec!["oculos".to_string()]);
    assert!(m.validate().is_ok());
    let e = patch(&mut m, "torno.inspection.every", "2")
        .unwrap_err()
        .to_string();
    assert!(e.contains("não tem `inspection`"), "{e}");
}
