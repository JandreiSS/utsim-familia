use super::*;
use crate::programa::testkit::*;

fn table(csv: &str) -> Table {
    Table::parse(csv)
}

#[test]
fn descriptive_matches_the_crate() {
    let t = table("x\n1\n2\n3\n4\n10\n");
    let o = run(&Recipe::Descriptive { col: 0 }, &t).unwrap();
    let d = utsim_stats::descriptive::describe(
        &[1.0, 2.0, 3.0, 4.0, 10.0],
        LEVEL,
        QuantileMethod::default(),
    )
    .unwrap();
    assert_eq!(o.value("média"), Some(d.mean));
    assert_eq!(o.value("desvio-padrão"), Some(d.sd));
    assert_eq!(o.series.len(), 5);
}

#[test]
fn capability_and_chart_on_decimal_comma_data() {
    let mut csv = String::from("ordem;medida\n");
    let mut rng = crate::formacao::exams::Rng::new(9);
    for i in 0..50 {
        let v = format!("{:.3}", 10.0 + 0.1 * rng.normal()).replace('.', ",");
        csv.push_str(&format!("{i};{v}\n"));
    }
    let t = table(&csv);
    let o = run(
        &Recipe::Capability {
            col: 1,
            lsl: Some(9.5),
            usl: Some(10.5),
            target: None,
            subgroup: 1,
        },
        &t,
    )
    .unwrap();
    assert!(o.value("Cpk").unwrap() > 1.0, "{o:?}");
    let c = run(
        &Recipe::Chart {
            kind: ChartKind::IMr,
            col: 1,
            subgroup: 1,
            size_col: None,
            nelson: false,
        },
        &t,
    )
    .unwrap();
    let (cl, l, u) = c.limits.unwrap();
    assert!(l < cl && cl < u);
    assert_eq!(c.series.len(), 50);
}

#[test]
fn tests_give_the_crates_p_values() {
    let t = table("a,b,g\n1,2,x\n2,3,x\n3,5,y\n4,6,y\n5,8,z\n6,9,z\n");
    let o = run(
        &Recipe::TTwo {
            a: 0,
            b: 1,
            pooled: false,
        },
        &t,
    )
    .unwrap();
    let r = hypothesis::t_two_sample(
        &[1., 2., 3., 4., 5., 6.],
        &[2., 3., 5., 6., 8., 9.],
        0.0,
        false,
        LEVEL,
        Alternative::TwoSided,
    )
    .unwrap();
    assert_eq!(o.value("valor-p"), Some(r.p_value));
    let p = run(&Recipe::TPaired { a: 0, b: 1 }, &t).unwrap();
    assert!(p.value("valor-p").unwrap() < 0.01);
    let an = run(&Recipe::Anova { col: 0, group: 2 }, &t).unwrap();
    assert_eq!(an.value("grupos"), Some(3.0));
    let chi = run(&Recipe::ChiSquare { a: 2, b: 2 }, &t).unwrap();
    assert!(chi.value("X²").unwrap() > 0.0);
    assert!(
        run(
            &Recipe::Proportion {
                x: 5,
                n: 0,
                p0: 0.5
            },
            &t
        )
        .is_err()
    );
    let mw = run(&Recipe::MannWhitney { a: 0, b: 1 }, &t).unwrap();
    assert!(mw.value("valor-p").is_some());
    let w = run(&Recipe::Wilcoxon { a: 0, b: 1 }, &t).unwrap();
    assert!(w.value("valor-p").is_some());
    assert!(
        run(&Recipe::TOne { col: 0, mu0: 3.5 }, &t)
            .unwrap()
            .value("valor-p")
            .unwrap()
            > 0.5
    );
}

#[test]
fn type1_study_by_the_formula() {
    let x: Vec<f64> = (0..20)
        .map(|i| 10.0 + if i % 2 == 0 { 0.01 } else { -0.01 })
        .collect();
    let r = type1(&x, 10.0, 1.0).unwrap();
    let sd = utsim_stats::descriptive::std_dev(&x);
    assert!((r.cg - 0.2 / (6.0 * sd)).abs() < 1e-12);
    assert!((r.cgk - (0.1 - r.bias.abs()) / (3.0 * sd)).abs() < 1e-12);
    assert!(type1(&x, 10.0, 0.0).is_err());
    let csv: String = std::iter::once("v".to_string())
        .chain(x.iter().map(|v| v.to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let o = run(
        &Recipe::Type1 {
            col: 0,
            reference: 10.0,
            tolerance: 1.0,
        },
        &table(&csv),
    )
    .unwrap();
    assert!(o.verdict.contains("capaz"));
}

fn dataset(t: &mut T, origin: DataOrigin) -> Dataset {
    let p = t.program;
    let id = t.id();
    t.commit(dados::register(
        p,
        id,
        &dados::DatasetInput {
            name: "d".into(),
            origin,
            file: "f.csv".into(),
            hash: "h".into(),
            ..dados::DatasetInput::default()
        },
    ));
    dados::datasets(&t.s, p)
        .into_iter()
        .find(|d| d.id == id)
        .unwrap()
}

#[test]
fn medir_checks_from_the_saved_analyses() {
    let mut t = T::new();
    let p = t.program;
    assert!(!msa_approved(&t.s, p));
    assert!(!baseline_real(&t.s, p));
    // A simulated baseline does not count.
    let sim = dataset(&mut t, DataOrigin::Simulated);
    let cap = Recipe::Capability {
        col: 0,
        lsl: Some(1.0),
        usl: Some(2.0),
        target: None,
        subgroup: 1,
    };
    let id = t.id();
    t.commit(record(
        p,
        id,
        "",
        &sim,
        &cap,
        &Outcome::default(),
        d(2026, 11, 3),
    ));
    assert!(!baseline_real(&t.s, p));
    let real = dataset(&mut t, DataOrigin::Real);
    let id = t.id();
    t.commit(record(
        p,
        id,
        "",
        &real,
        &cap,
        &Outcome::default(),
        d(2026, 11, 3),
    ));
    assert!(baseline_real(&t.s, p));
    // A Gage R&R of 35 %: not approved until justified; 30 % exactly is approved.
    let g = Recipe::GageRr {
        part: 0,
        operator: 1,
        value: 2,
        tolerance: None,
        anova: true,
    };
    let mut o = Outcome::default();
    o.put(&format!("%VE {}", msa::GRR), 35.0);
    let gid = t.id();
    t.commit(record(p, gid, "", &real, &g, &o, d(2026, 11, 3)));
    assert!(!msa_approved(&t.s, p));
    t.set(
        Kind::Analysis,
        gid,
        fields::JUSTIFICATION,
        "característica de baixa criticidade",
    );
    assert!(msa_approved(&t.s, p));
    let mut o30 = Outcome::default();
    o30.put(&format!("%VE {}", msa::GRR), 30.0);
    let mut t2 = T::new();
    let p2 = t2.program;
    let r2 = dataset(&mut t2, DataOrigin::Real);
    let id = t2.id();
    t2.commit(record(p2, id, "", &r2, &g, &o30, d(2026, 11, 3)));
    assert!(msa_approved(&t2.s, p2));
    // The saved analysis reads back with its recipe and origin.
    let a = analyses(&t.s, p);
    assert_eq!(a.len(), 3);
    assert_eq!(a[2].recipe, Some(g));
    assert_eq!(a[1].origin, DataOrigin::Real);
}

#[test]
fn rerun_gives_the_same_numbers() {
    let csv = "x\n1,5\n2,5\n3\n";
    let mut t = T::new();
    let p = t.program;
    let ds = dataset(&mut t, DataOrigin::Real);
    let r = Recipe::Descriptive { col: 0 };
    let o = run(&r, &table(csv)).unwrap();
    let id = t.id();
    t.commit(record(p, id, "media", &ds, &r, &o, d(2026, 11, 3)));
    let a = &analyses(&t.s, p)[0];
    assert_eq!(
        format!("{:?}", rerun(a, csv).unwrap().lines),
        format!("{:?}", o.lines)
    );
    assert_eq!(a.recipe.as_ref().unwrap().key(), "descritiva");
}

#[test]
fn bias_and_linearity_match_the_crate() {
    // Twelve readings of a reference of 5: a bias of about +0,05.
    let x: Vec<f64> = (0..12)
        .map(|i| 5.05 + if i % 2 == 0 { 0.01 } else { -0.01 })
        .collect();
    let csv: String = std::iter::once("v".to_string())
        .chain(x.iter().map(|v| v.to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let o = run(
        &Recipe::Bias {
            col: 0,
            reference: 5.0,
            tolerance: Some(1.0),
        },
        &table(&csv),
    )
    .unwrap();
    let b = msa::bias(5.0, &x, LEVEL).unwrap();
    assert_eq!(o.value("viés"), Some(b.bias));
    assert_eq!(o.value("valor-p"), Some(b.p_value));
    assert!((o.value("%viés da tolerância").unwrap() - 5.0).abs() < 1e-6);
    assert!(o.verdict.contains("significativo"), "{}", o.verdict);
    assert!(
        run(
            &Recipe::Bias {
                col: 0,
                reference: 5.0,
                tolerance: None
            },
            &table("v\n1\n2\n")
        )
        .is_err()
    );
    // Linearity: the bias grows with the reference (0 at 2, 0,1 at 4, 0,2 at 6...).
    let mut csv = String::from("ref;medida\n");
    let mut pairs = Vec::new();
    for r in [2.0, 4.0, 6.0, 8.0, 10.0] {
        for k in 0..3 {
            let m = r + 0.05 * (r - 2.0) + 0.001 * f64::from(k);
            pairs.push((r, m));
            csv.push_str(&format!("{};{}\n", r, m.to_string().replace('.', ",")));
        }
    }
    let o = run(
        &Recipe::Linearity {
            reference: 0,
            value: 1,
            variation: Some(6.0),
        },
        &table(&csv),
    )
    .unwrap();
    let l = msa::linearity(&pairs, Some(6.0), LEVEL).unwrap();
    assert!((o.value("inclinação").unwrap() - l.slope).abs() < 1e-12);
    assert!((l.slope - 0.05).abs() < 1e-3);
    assert_eq!(o.series.len(), 5);
    assert!(o.verdict.contains("linearidade"), "{}", o.verdict);
}

#[test]
fn attribute_agreement_and_the_medir_check() {
    // Three appraisers, ten items, two trials; C misses item 3 once.
    let mut csv = String::from("item,avaliador,resposta,padrao\n");
    for i in 0..10 {
        let std = if i % 3 == 0 { "ruim" } else { "bom" };
        for a in ["A", "B", "C"] {
            for trial in 0..2 {
                let resp = if a == "C" && i == 3 && trial == 1 {
                    "bom"
                } else {
                    std
                };
                csv.push_str(&format!("p{i},{a},{resp},{std}\n"));
            }
        }
    }
    let rec = Recipe::Attribute {
        item: 0,
        appraiser: 1,
        response: 2,
        standard: Some(3),
    };
    let o = run(&rec, &table(&csv)).unwrap();
    assert_eq!(o.value("itens"), Some(10.0));
    assert_eq!(o.value("avaliadores"), Some(3.0));
    assert_eq!(o.value("repetições"), Some(2.0));
    assert!((o.value("% entre avaliadores").unwrap() - 90.0).abs() < 1e-9);
    let k = o.value(KAPPA).unwrap();
    assert!(k > KAPPA_LIMIT && k < 1.0, "{k}");
    assert_eq!(o.value("% A contra o padrão"), Some(100.0));
    assert!(o.verdict.contains("aceito"));
    // Saved, it approves the MSA of Medir.
    let mut t = T::new();
    let p = t.program;
    let ds = dataset(&mut t, DataOrigin::Real);
    assert!(!msa_approved(&t.s, p));
    let id = t.id();
    t.commit(record(p, id, "", &ds, &rec, &o, d(2026, 11, 6)));
    assert!(msa_approved(&t.s, p));
    assert_eq!(analyses(&t.s, p)[0].kappa(), Some(k));
    assert_eq!(rec.key(), "msa_atributos");
}

#[test]
fn extended_statistics_match_the_crate() {
    // Three machines (groups), four shifts (blocks), a second factor (tool) and a predictor.
    let mut csv = String::from("valor;maquina;turno;ferramenta;temperatura\n");
    let vals = [
        [10.1, 11.9, 13.2],
        [9.8, 12.4, 12.9],
        [10.4, 12.1, 13.8],
        [10.0, 12.8, 13.1],
    ];
    for (b, row) in vals.iter().enumerate() {
        for (g, v) in row.iter().enumerate() {
            let tool = if b % 2 == 0 { "nova" } else { "velha" };
            let temp = 20.0 + g as f64 * 2.0 + b as f64 * 0.5;
            csv.push_str(&format!(
                "{};M{};T{};{tool};{}\n",
                v.to_string().replace('.', ","),
                g + 1,
                b + 1,
                temp.to_string().replace('.', ",")
            ));
        }
    }
    let t = table(&csv);
    let groups: Vec<Vec<f64>> = (0..3)
        .map(|g| vals.iter().map(|r| r[g]).collect())
        .collect();
    let refs: Vec<&[f64]> = groups.iter().map(Vec::as_slice).collect();
    let o = run(&Recipe::Variances { col: 0, group: 1 }, &t).unwrap();
    assert_eq!(
        o.value("valor-p Levene"),
        Some(utsim_stats::variances::levene(&refs).unwrap().p_value)
    );
    assert!(o.value("Bartlett").is_some());
    let o = run(&Recipe::KruskalWallis { col: 0, group: 1 }, &t).unwrap();
    assert_eq!(
        o.value("H"),
        Some(utsim_stats::ranks::kruskal_wallis(&refs).unwrap().statistic)
    );
    assert!(o.value("valor-p").unwrap() < 0.05);
    let o = run(&Recipe::MoodMedian { col: 0, group: 1 }, &t).unwrap();
    assert_eq!(o.value("mediana de M2"), Some(12.25));
    let o = run(
        &Recipe::Friedman {
            value: 0,
            treatment: 1,
            block: 2,
        },
        &t,
    )
    .unwrap();
    // Machine 3 best in every shift, 1 worst: Q = 8 with 4 blocks.
    assert!((o.value("Q").unwrap() - 8.0).abs() < 1e-9);
    let o = run(
        &Recipe::TwoWayAnova {
            value: 0,
            a: 1,
            b: 3,
        },
        &t,
    )
    .unwrap();
    assert!(o.value("valor-p maquina").unwrap() < 0.001);
    assert!(o.value("valor-p maquina × ferramenta").is_some());
    assert_eq!(o.plots.len(), 1);
    let o = run(
        &Recipe::Regression {
            response: 0,
            predictors: vec![4],
        },
        &t,
    )
    .unwrap();
    assert!(o.value("R²").unwrap() > 0.8);
    assert!(o.value("coeficiente temperatura").unwrap() > 0.0);
    assert_eq!(o.series.len(), 12);
    assert!(
        run(
            &Recipe::Regression {
                response: 0,
                predictors: vec![]
            },
            &t
        )
        .is_err()
    );
    let o = run(
        &Recipe::SampleSize {
            kind: 1,
            a: 0.5,
            b: 1.0,
            target: 0.8,
            alpha: 0.05,
        },
        &t,
    )
    .unwrap();
    let n = utsim_stats::power::sample_size(
        utsim_stats::power::Design::TwoMeans {
            delta: 0.5,
            sd: 1.0,
        },
        0.8,
        0.05,
    )
    .unwrap();
    assert_eq!(o.value("n"), Some(n as f64));
    assert_eq!(Recipe::Variances { col: 0, group: 1 }.key(), "variancias");
}
