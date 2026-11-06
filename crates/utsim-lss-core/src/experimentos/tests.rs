use super::*;
use crate::analises::{self, Recipe, Table};

fn spec(k: usize, fraction: usize) -> ExperimentSpec {
    let names = [
        "Temperatura",
        "Pressão",
        "Tempo",
        "Catalisador",
        "Velocidade",
    ];
    ExperimentSpec {
        name: "Rendimento".into(),
        factors: (0..k)
            .map(|i| FactorSpec {
                name: names[i].into(),
                low: format!("{}", 10 * (i + 1)),
                high: format!("{}", 10 * (i + 1) + 4),
                ..FactorSpec::default()
            })
            .collect(),
        fraction,
        replicates: 1,
        center_points: 0,
        seed: 2026,
        response: "rendimento".into(),
    }
}

/// Fills the response of a worksheet with `f(coded levels)`.
fn fill(csv: &str, k: usize, f: impl Fn(&[f64]) -> f64) -> String {
    let mut out = String::new();
    for (i, line) in csv.lines().enumerate() {
        if i == 0 {
            out.push_str(line);
        } else {
            let cells: Vec<&str> = line.split(';').collect();
            let x: Vec<f64> = cells[3..3 + k].iter().map(|c| c.parse().unwrap()).collect();
            let mut c: Vec<String> = cells.iter().map(|s| s.to_string()).collect();
            let last = c.len() - 1;
            c[last] = f(&x).to_string().replace('.', ",");
            out.push_str(&c.join(";"));
        }
        out.push('\n');
    }
    out
}

#[test]
fn worksheet_filled_and_analysed() {
    let s = spec(3, 0);
    let d = s.design().unwrap();
    let ws = worksheet(&s, &d);
    let t = Table::parse(&ws);
    assert_eq!(
        t.header,
        [
            "ordem",
            "ordem_padrao",
            "centro",
            "A",
            "B",
            "C",
            "Temperatura",
            "Pressão",
            "Tempo",
            "rendimento"
        ]
    );
    assert_eq!(t.rows.len(), 8);
    // The actual levels follow the coded ones.
    for r in &t.rows {
        assert_eq!(r[6], if r[3] == "-1" { "10" } else { "14" });
    }
    let (f, resp) = columns(&t.header);
    assert_eq!((f.clone(), resp), (vec![3, 4, 5], Some(9)));
    // y = 60 + 5A − 2B + 1,5AC: effects 10, −4 and 3.
    let filled = fill(&ws, 3, |x| {
        60.0 + 5.0 * x[0] - 2.0 * x[1] + 1.5 * x[0] * x[2]
    });
    let r = Recipe::Factorial {
        factors: f,
        response: 9,
        order: 2,
    };
    let o = analises::run(&r, &Table::parse(&filled)).unwrap();
    assert!((o.value("efeito A").unwrap() - 10.0).abs() < 1e-9);
    assert!((o.value("efeito B").unwrap() + 4.0).abs() < 1e-9);
    assert!((o.value("efeito AC").unwrap() - 3.0).abs() < 1e-9);
    assert!(o.value("efeito BC").unwrap().abs() < 1e-9);
    assert!(o.notes[0].contains("A = Temperatura"));
    assert_eq!(o.plots[0].labels[0], "A");
    assert!(o.plots[0].bars);
    assert_eq!(o.plots[1].series.len(), 3);
    assert_eq!(o.plots[1].series[0].1, vec![55.0, 65.0]);
    assert_eq!(o.plots.len(), 2 + 3);
    assert_eq!(r.key(), "experimento_fatorial");
    // The same analysis on the actual levels (coded by the app).
    let r2 = Recipe::Factorial {
        factors: vec![6, 7, 8],
        response: 9,
        order: 2,
    };
    let o2 = analises::run(&r2, &Table::parse(&filled)).unwrap();
    assert_eq!(o2.value("efeito A"), o.value("efeito A"));
}

#[test]
fn fraction_aliases_and_center_points() {
    let mut s = spec(4, 1);
    s.center_points = 3;
    let d = s.design().unwrap();
    assert_eq!(d.runs.len(), 8 + 3);
    let lines = alias_lines(&d);
    assert_eq!(lines[0], "I = ABCD");
    assert_eq!(lines[1], "resolução IV");
    assert!(lines.contains(&"A = BCD".to_string()), "{lines:?}");
    assert!(lines.contains(&"AB = CD".to_string()), "{lines:?}");
    let ws = worksheet(&s, &d);
    let centers: Vec<&str> = ws.lines().filter(|l| l.contains(";sim;")).collect();
    assert_eq!(centers.len(), 3);
    assert!(centers[0].contains(";12;22;32;42;"), "{}", centers[0]);
    assert_eq!(ExperimentSpec::fractions(4), vec![0, 1]);
    assert_eq!(ExperimentSpec::fractions(7), vec![0, 1, 2, 3, 4]);
    s.fraction = 2;
    assert_eq!(s.design(), Err(ExpError::Fraction(4, 2)));
    // Text levels need the center.
    let mut t = spec(2, 0);
    t.center_points = 1;
    t.factors[0].low = "Exp(mean: 1.0)".into();
    t.factors[0].high = "Exp(mean: 0.5)".into();
    assert!(matches!(t.design(), Err(ExpError::Center(_))));
    t.factors[1].high = t.factors[1].low.clone();
    t.center_points = 0;
    assert!(matches!(t.design(), Err(ExpError::Levels(_))));
    assert_eq!(full().on_model(), Ok(()));
    assert!(matches!(spec(2, 0).on_model(), Err(ExpError::NoKey(_))));
}

const MM1: &str = r#"Model(
    name: "M/M/1 curta",
    run: (stop_time: 2000.0, warmup: 100.0, seed: 7, replications: 4),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0)), position: (0.0, 0.0, 0.0)),
        (name: "fila",    kind: Queue(),                                position: (3.0, 0.0, 0.0)),
        (name: "maquina", kind: Processor(process: Exp(mean: 0.8)),     position: (6.0, 0.0, 0.0)),
        (name: "saida",   kind: Sink,                                   position: (9.0, 0.0, 0.0)),
    ],
    connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
)"#;

fn full() -> ExperimentSpec {
    ExperimentSpec {
        name: "Máquina e chegada".into(),
        factors: vec![
            FactorSpec {
                name: "Processo".into(),
                low: "Exp(mean: 0.8)".into(),
                high: "Exp(mean: 0.5)".into(),
                key: "maquina.process".into(),
                ..FactorSpec::default()
            },
            FactorSpec {
                name: "Chegadas".into(),
                low: "Exp(mean: 1.0)".into(),
                high: "Exp(mean: 1.2)".into(),
                key: "chegada.interarrival".into(),
                ..FactorSpec::default()
            },
        ],
        replicates: 1,
        seed: 1,
        ..ExperimentSpec::default()
    }
}

#[test]
fn the_model_runs_the_experiment() {
    let m = crate::simulacao::parse(MM1, None).unwrap();
    let s = full();
    let d = s.design().unwrap();
    let pts = scenarios(&s, &d).unwrap();
    assert_eq!(pts.len(), 4);
    assert_eq!(pts[0].1.name, "P1");
    assert_eq!(pts[0].1.set["maquina.process"], "Exp(mean: 0.8)");
    assert_eq!(pts[1].1.set["maquina.process"], "Exp(mean: 0.5)");
    let scen: Vec<_> = pts.iter().map(|(_, s)| s.clone()).collect();
    let res = utsim_model::experiment::run_scenarios(&m, &scen, &[], &|_, _| true).unwrap();
    let measure = res[0]
        .summary
        .keys()
        .find(|k| k.contains("maquina") && k.contains("util"))
        .cloned()
        .unwrap();
    let csv = simulated_csv(&s, &pts, &res, std::slice::from_ref(&measure));
    let t = Table::parse(&csv);
    assert_eq!(t.rows.len(), 4 * 4);
    let (f, resp) = columns(&t.header);
    assert_eq!(f, vec![2, 3]);
    let o = analises::run(
        &Recipe::Factorial {
            factors: f,
            response: resp.unwrap(),
            order: 2,
        },
        &t,
    )
    .unwrap();
    // A faster machine lowers its utilization.
    assert!(o.value("efeito A").unwrap() < -0.1, "{o:?}");
    assert!(o.value("valor-p A").unwrap() < 0.05);
}
