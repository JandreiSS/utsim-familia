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
