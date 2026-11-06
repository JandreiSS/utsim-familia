use super::*;
use crate::programa::testkit::*;

const MM1: &str = r#"Model(
    name: "M/M/1 curta",
    run: (stop_time: 3000.0, warmup: 200.0, seed: 7, replications: 6),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0)), position: (0.0, 0.0, 0.0)),
        (name: "fila",    kind: Queue(),                                position: (3.0, 0.0, 0.0)),
        (name: "maquina", kind: Processor(process: Exp(mean: 0.8)),     position: (6.0, 0.0, 0.0)),
        (name: "saida",   kind: Sink,                                   position: (9.0, 0.0, 0.0)),
    ],
    connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
)"#;

fn model() -> Model {
    parse(MM1, None).unwrap()
}

#[test]
fn link_hash_and_stale_dependents() {
    let mut t = T::new();
    let p = t.program;
    let m = model();
    let l = t.id();
    t.commit(link_edits(
        p,
        l,
        "modelos/mm1.ron",
        MM1,
        &m,
        true,
        d(2026, 11, 3),
    ));
    let link = link(&t.s, p).unwrap();
    assert_eq!(link.hash, hash_text(MM1));
    assert_eq!(link.hash.len(), 64);
    assert_eq!(link.name, "M/M/1 curta");
    assert!(link.copied);
    // A data set made from this version.
    let ds = t.id();
    let csv = "replicacao,x\n1,2\n";
    t.commit(crate::dados::register(
        p,
        ds,
        &simulated_dataset(
            "Vazão",
            &m,
            &link.hash,
            "",
            "a.csv",
            "h",
            csv,
            d(2026, 11, 3),
        ),
    ));
    assert!(stale(&t.s, p, &link.hash).is_empty());
    // The file changed: the data set depends on the old version.
    let other = format!("{MM1}\n// novo");
    let st = stale(&t.s, p, &hash_text(&other));
    assert_eq!(st.len(), 1);
    assert_eq!((st[0].kind, st[0].id), (Kind::Dataset, ds));
}

#[test]
fn sipoc_generated_from_the_model() {
    let mut t = T::new();
    let p = t.program;
    let m = model();
    let cols = sipoc_columns(&m).unwrap();
    assert!(cols.iter().all(|c| !c.is_empty()), "{cols:?}");
    let dl = t.id();
    let mut n = 50_000;
    t.commit(sipoc_edits(&t.s, p, dl, "abc", &cols, || {
        n += 1;
        Id::v7(n, [9; 10])
    }));
    let s = sipoc::of_deliverable(&t.s, dl);
    for (i, c) in cols.iter().enumerate() {
        assert_eq!(
            s.columns[i]
                .iter()
                .map(|(_, x)| x.clone())
                .collect::<Vec<_>>(),
            *c
        );
    }
    let e = t.s.entity(dl).unwrap();
    assert_eq!(e.text(fields::GENERATED), "modelo");
    assert_eq!(e.text(fields::MODEL_HASH), "abc");
    // CSV quoting.
    let q = sipoc_from_csv("a,b,c,d,e\n\"x, y\",,\"z \"\"w\"\"\",,\n");
    assert_eq!(q[0], vec!["x, y"]);
    assert_eq!(q[2], vec!["z \"w\""]);
    assert!(q[1].is_empty());
}

#[test]
fn carta_and_diagrams() {
    let m = model();
    let c = carta(&m);
    assert!(
        c.rows
            .iter()
            .any(|r| r.objeto.as_deref() == Some("maquina"))
    );
    let html = diagram_html(&m, utsim_diagram::TipoDiagrama::Blocos).unwrap();
    assert!(html.contains("<svg"));
}

#[test]
fn simulated_dataset_and_study_with_paired_difference() {
    let m = model();
    let reports = m.run_replications().unwrap();
    let per = utsim_model::analysis::per_replication(&reports);
    let metric = per
        .keys()
        .find(|k| k.contains("fila") && k.contains("conteudo") || k.ends_with("content"))
        .cloned()
        .unwrap_or_else(|| per.keys().next().unwrap().clone());
    let csv = replication_csv(&per, std::slice::from_ref(&metric));
    let (cols, rows) = crate::dados::shape(&csv);
    assert_eq!(cols, vec!["replicacao".to_string(), metric.clone()]);
    assert_eq!(rows, 6);
    let ds = simulated_dataset("Fila", &m, "h", "", "f.csv", "fh", &csv, d(2026, 11, 3));
    assert_eq!(ds.origin, DataOrigin::Simulated);
    assert_eq!(ds.seed, Some(7));
    assert_eq!(ds.replications, Some(6));

    // A faster machine: the utilization of the machine goes down, significantly.
    let scenarios = vec![
        Scenario {
            name: "atual".into(),
            set: BTreeMap::new(),
        },
        Scenario {
            name: "maquina rapida".into(),
            set: [("maquina.process".to_string(), "Exp(mean: 0.5)".to_string())]
                .into_iter()
                .collect(),
        },
    ];
    let results =
        utsim_model::experiment::run_scenarios(&m, &scenarios, &[], &|_, _| true).unwrap();
    let all: Vec<String> = results[0].summary.keys().cloned().collect();
    let measure = all
        .iter()
        .find(|k| k.contains("maquina") && k.contains("util"))
        .cloned()
        .unwrap_or_else(|| all[0].clone());
    let mins: BTreeMap<String, f64> = [(measure.clone(), 0.05)].into_iter().collect();
    let st = build_study(
        "Máquina mais rápida",
        &m,
        "h",
        &scenarios,
        std::slice::from_ref(&measure),
        &results,
        &mins,
    )
    .unwrap();
    assert_eq!(st.schema, STUDY_SCHEMA);
    assert_eq!(st.differences.len(), 1);
    let dif = &st.differences[0];
    let a = &results[1].per_replication[&measure];
    let b = &results[0].per_replication[&measure];
    let s = utsim_model::analysis::paired_difference_at(a, b, LEVEL);
    assert_eq!((dif.mean, dif.half_width), (s.mean, s.half_width));
    if measure.contains("util") {
        assert!(dif.significant && dif.relevant && dif.mean < 0.0, "{dif:?}");
    }
    // The package reads back the same.
    let back = StudyFile::from_json(&st.to_json()).unwrap();
    assert_eq!(
        (&back.scenarios, &back.measures, back.seed),
        (&st.scenarios, &st.measures, st.seed)
    );
    for (a, b) in back.differences.iter().zip(&st.differences) {
        assert!((a.mean - b.mean).abs() < 1e-12 && a.relevant == b.relevant);
    }
    let mut t = T::new();
    let p = t.program;
    let sid = t.id();
    t.commit(record_study(
        p,
        sid,
        &st,
        "estudo.json",
        "x",
        d(2026, 11, 3),
    ));
    let got = &studies(&t.s, p)[0];
    assert_eq!(got.name, "Máquina mais rápida");
    assert!(!got.conclusion.is_empty());
    assert_eq!(
        build_study(
            "x",
            &m,
            "h",
            &scenarios[..1],
            &[],
            &results[..1],
            &BTreeMap::new()
        ),
        Err(SimError::NoScenario)
    );
}

#[test]
fn decision_limits() {
    // Interval (0.5, 1.5) with a minimum of 0.5: significant and relevant (limit included).
    assert_eq!(decide(1.0, 0.5, 0.5), (true, true));
    // Interval (0.4, 1.6): significant, but it may be only 0.4.
    assert_eq!(decide(1.0, 0.6, 0.5), (true, false));
    // Contains zero.
    assert_eq!(decide(0.1, 0.2, 0.0), (false, false));
    // Negative differences count by their size.
    assert_eq!(decide(-2.0, 0.5, 1.0), (true, true));
    assert_eq!(decide(f64::NAN, 0.1, 0.0), (false, false));
}

#[test]
fn bottleneck_and_observed_path() {
    let m = model();
    let reports = m.run_replications().unwrap();
    let stats = utsim_model::analysis::object_stats(&m, &reports);
    let b = bottleneck(&stats);
    assert_eq!(b.bottleneck.as_deref(), Some("maquina"));
    assert_eq!(b.rows[0].name, "maquina");
    assert!((b.rows[0].utilization - 0.8).abs() < 0.1, "{:?}", b.rows[0]);
    let o = observed_pert(&m, "fila", "maquina", Some(3)).unwrap();
    assert!(o.units > 0);
    assert!(o.total > 0.0);
    assert!(o.activities.iter().any(|a| a.0 == "maquina"));
    let mut t = T::new();
    let p = t.program;
    let id = t.id();
    t.commit(record_analysis(
        p,
        id,
        AnalysisKind::ObservedPert,
        "fila → máquina",
        "h",
        serde_json::to_value(&o).unwrap(),
        3,
        d(2026, 11, 3),
    ));
    let a = &model_analyses(&t.s, p)[0];
    assert_eq!(a.kind, AnalysisKind::ObservedPert);
    let back: Observed = serde_json::from_value(a.result.clone()).unwrap();
    assert_eq!(back, o);
}

fn study_file(mean: f64, hw: f64) -> StudyFile {
    let (significant, relevant) = decide(mean, hw, 0.0);
    StudyFile {
        schema: STUDY_SCHEMA,
        name: "Segunda máquina".into(),
        model: "Linha".into(),
        model_hash: "ab".repeat(32),
        version: "x".into(),
        replications: 10,
        seed: 42,
        level: LEVEL,
        scenarios: Vec::new(),
        measures: vec!["vazao".into()],
        results: Vec::new(),
        per_replication: BTreeMap::new(),
        differences: vec![Difference {
            scenario: "duas maquinas".into(),
            metric: "vazao".into(),
            mean,
            half_width: hw,
            significant,
            minimum: 0.0,
            relevant,
        }],
    }
}

#[test]
fn gain_from_a_difference_of_the_study() {
    let mut t = T::new();
    let p = t.program;
    let file = study_file(40.0, 10.0);
    let sid = t.id();
    t.commit(record_study(
        p,
        sid,
        &file,
        "e.json",
        &"cd".repeat(32),
        d(2026, 11, 6),
    ));
    let study = studies(&t.s, p)[0].clone();
    let dif = &file.differences[0];
    let mut g = GainInput {
        description: "Peças a mais com a segunda máquina".into(),
        category: crate::domain::SavingCategory::Hard,
        value_per_unit: 2.5,
        periods_per_year: 50.0,
        higher_is_better: true,
        conservative: true,
    };
    // Conservative: the end of the interval nearest to zero, 30.
    let (v, why) = gain_value(&study, &file, dif, &g).unwrap();
    assert!((v - 30.0 * 2.5 * 50.0).abs() < 1e-9);
    assert!(why.contains("Segunda máquina") && why.contains("semente 42") && why.contains("3750"));
    g.conservative = false;
    assert!((gain_value(&study, &file, dif, &g).unwrap().0 - 5000.0).abs() < 1e-9);
    // A decrease in the chosen sense is a loss, not a gain.
    g.higher_is_better = false;
    assert_eq!(gain_value(&study, &file, dif, &g), Err(GainError::Worse));
    g.higher_is_better = true;
    g.periods_per_year = 0.0;
    assert_eq!(
        gain_value(&study, &file, dif, &g),
        Err(GainError::BadFactor)
    );
    g.periods_per_year = 50.0;
    let flat = study_file(1.0, 5.0);
    assert_eq!(
        gain_value(&study, &flat, &flat.differences[0], &g),
        Err(GainError::NotSignificant)
    );
    // The saving: estimated, source simulation, with the study and the rationale.
    g.conservative = true;
    let id = t.id();
    t.commit(gain_edits(p, id, &study, &file, dif, &g).unwrap());
    let plan = &crate::savings::plans(&t.s, p)[0];
    assert_eq!(plan.source, crate::savings::Source::Simulation);
    assert_eq!(plan.study, Some(study.id));
    assert_eq!(plan.saving.status, crate::domain::SavingStatus::Estimated);
    assert!((plan.saving.value - 3750.0).abs() < 1e-9);
    assert!(plan.saving.rationale.contains("duas maquinas"));
}

#[test]
fn vsm_and_spaghetti_from_the_model() {
    let m = model();
    let (html, rows) = vsm_from_model(&m).unwrap();
    assert!(html.contains("<svg"));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "maquina");
    assert!((rows[0].cycle - 0.8).abs() < 1e-9);
    assert!(rows[0].value_added);
    assert!(rows[0].inventory.is_some_and(|i| i > 0.0));
    let (page, travels) = spaghetti(&m).unwrap();
    assert!(page.contains("<html") || page.contains("<!DOCTYPE") || page.contains("<!doctype"));
    assert_eq!(travels.last().map(|t| t.resource.as_str()), Some("itens"));
    assert!(travels.last().unwrap().trips > 0);
}
