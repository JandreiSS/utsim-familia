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
        screening: false,
        surface: None,
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
            let x: Vec<f64> = cells[3..3 + k]
                .iter()
                .map(|c| c.replace(',', ".").parse().unwrap())
                .collect();
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
    let ws = worksheet(&s, &s.plan().unwrap());
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
    let lines = alias_lines(&s, &d);
    assert_eq!(lines[0], "I = ABCD");
    assert_eq!(lines[1], "resolução IV");
    assert!(lines.contains(&"A = BCD".to_string()), "{lines:?}");
    assert!(lines.contains(&"AB = CD".to_string()), "{lines:?}");
    let ws = worksheet(&s, &s.plan().unwrap());
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

#[test]
fn plackett_burman_screening() {
    let mut s = ExperimentSpec {
        name: "Triagem".into(),
        factors: (0..9)
            .map(|i| FactorSpec {
                name: format!("X{}", i + 1),
                low: "0".into(),
                high: "1".into(),
                ..FactorSpec::default()
            })
            .collect(),
        screening: true,
        replicates: 1,
        seed: 3,
        response: "y".into(),
        ..ExperimentSpec::default()
    };
    assert_eq!(ExperimentSpec::screening_runs(9), Some(12));
    assert_eq!(ExperimentSpec::screening_runs(15), Some(16));
    assert_eq!(ExperimentSpec::screening_runs(16), None);
    let d = s.design().unwrap();
    assert_eq!(d.runs.len(), 12);
    // Balanced, orthogonal columns.
    for i in 0..9 {
        assert_eq!(d.runs.iter().map(|r| r.levels[i] as i32).sum::<i32>(), 0);
        for j in 0..i {
            let dot: i32 = d
                .runs
                .iter()
                .map(|r| (r.levels[i] * r.levels[j]) as i32)
                .sum();
            assert_eq!(dot, 0);
        }
    }
    let lines = alias_lines(&s, &d);
    assert!(lines[0].contains("12 corridas para 9 fatores"), "{lines:?}");
    assert!(lines[3].starts_with("2 coluna(s)"), "{lines:?}");
    // y = 50 + 4A − 3E + 2I: effects 8, −6 and 4; asking for interactions falls back to
    // the main effects.
    let ws = worksheet(&s, &s.plan().unwrap());
    let filled = fill(&ws, 9, |x| 50.0 + 4.0 * x[0] - 3.0 * x[4] + 2.0 * x[8]);
    let t = Table::parse(&filled);
    let (f, resp) = columns(&t.header);
    assert_eq!(f.len(), 9);
    let r = Recipe::Factorial {
        factors: f,
        response: resp.unwrap(),
        order: 2,
    };
    let o = analises::run(&r, &t).unwrap();
    assert!((o.value("efeito A").unwrap() - 8.0).abs() < 1e-9);
    assert!((o.value("efeito E").unwrap() + 6.0).abs() < 1e-9);
    assert!((o.value("efeito I").unwrap() - 4.0).abs() < 1e-9);
    assert!(o.value("efeito B").unwrap().abs() < 1e-9);
    assert!(o.value("efeito AB").is_none());
    assert!(o.notes.iter().any(|n| n.contains("plano de triagem")));
    // More than 8 factors only as a screening.
    s.screening = false;
    assert_eq!(s.design(), Err(ExpError::Factors));
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
    let pts = scenarios(&s, &s.plan().unwrap()).unwrap();
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

#[test]
fn central_composite_surface_finds_the_maximum() {
    let s = ExperimentSpec {
        name: "Superfície".into(),
        factors: vec![
            FactorSpec {
                name: "Temperatura".into(),
                low: "100".into(),
                high: "120".into(),
                ..FactorSpec::default()
            },
            FactorSpec {
                name: "Tempo".into(),
                low: "10".into(),
                high: "20".into(),
                ..FactorSpec::default()
            },
        ],
        surface: Some(Surface::CentralComposite),
        center_points: 5,
        seed: 4,
        response: "y".into(),
        ..ExperimentSpec::default()
    };
    let plan = s.plan().unwrap();
    assert_eq!(plan.runs.len(), 4 + 4 + 5);
    assert!(plan.lines[0].contains("α = 1,414214"), "{:?}", plan.lines);
    let ws = worksheet(&s, &plan);
    let axial: Vec<&str> = ws.lines().filter(|l| l.contains(";axial;")).collect();
    assert_eq!(axial.len(), 4);
    // An axial point of Temperatura at +α: 110 + 1,414214 × 10.
    assert!(ws.contains(";1,414214;0;124,142136;15;"), "{ws}");
    // y = 80 + 4A + 2B − 2A² − B²: maximum at A = 1, B = 1 (120 and 20), y = 83.
    let filled = fill(&ws, 2, |x| {
        80.0 + 4.0 * x[0] + 2.0 * x[1] - 2.0 * x[0] * x[0] - x[1] * x[1]
    });
    let t = Table::parse(&filled);
    let (f, resp) = columns(&t.header);
    let r = Recipe::Surface {
        factors: f,
        response: resp.unwrap(),
    };
    let o = analises::run(&r, &t).unwrap();
    assert!((o.value("coef A²").unwrap() + 2.0).abs() < 1e-6);
    assert!((o.value("ponto estacionário A").unwrap() - 1.0).abs() < 1e-6);
    assert!((o.value("ponto estacionário B").unwrap() - 1.0).abs() < 1e-6);
    assert!((o.value("ponto estacionário Temperatura (real)").unwrap() - 120.0).abs() < 1e-4);
    assert!((o.value("ponto estacionário Tempo (real)").unwrap() - 20.0).abs() < 1e-4);
    assert!((o.value("resposta prevista no ponto estacionário").unwrap() - 83.0).abs() < 1e-6);
    assert!(
        o.verdict.starts_with("o ponto estacionário é um máximo"),
        "{}",
        o.verdict
    );
    assert_eq!(o.plots.len(), 2);
    assert_eq!(r.key(), "superficie_resposta");
    // Box-Behnken needs center points and 3 to 5 factors.
    let mut b = s.clone();
    b.surface = Some(Surface::BoxBehnken);
    assert!(matches!(b.plan(), Err(ExpError::Design(_))));
    b.factors.push(FactorSpec {
        name: "Pressão".into(),
        low: "1".into(),
        high: "2".into(),
        ..FactorSpec::default()
    });
    assert_eq!(b.plan().unwrap().runs.len(), 12 + 5);
    b.center_points = 0;
    assert!(b.plan().is_err());
    // Text levels have no axial points.
    let mut c = s.clone();
    c.factors[0].low = "X".into();
    c.factors[0].high = "Y".into();
    c.factors[0].center = "Z".into();
    assert_eq!(c.plan(), Err(ExpError::Axial("Temperatura".into())));
}

#[test]
fn desirability_of_two_responses() {
    use utsim_doe::Goal;
    let s = ExperimentSpec {
        factors: vec![
            FactorSpec {
                name: "Temperatura".into(),
                low: "100".into(),
                high: "120".into(),
                ..FactorSpec::default()
            },
            FactorSpec {
                name: "Tempo".into(),
                low: "10".into(),
                high: "20".into(),
                ..FactorSpec::default()
            },
        ],
        surface: Some(Surface::CentralComposite),
        center_points: 5,
        seed: 8,
        ..ExperimentSpec::default()
    };
    let plan = s.plan().unwrap();
    // y1 = 80 + 4A + 2B − 2A² − B² (yield, maximum 83 at A = B = 1); y2 = 10 + 2A + B (cost).
    let mut csv = String::from("A;B;rendimento;custo\n");
    for r in &plan.runs {
        let (a, b) = (r.coded[0], r.coded[1]);
        let y1 = 80.0 + 4.0 * a + 2.0 * b - 2.0 * a * a - b * b;
        let y2 = 10.0 + 2.0 * a + b;
        csv.push_str(&format!("{a};{b};{y1};{y2}\n").replace('.', ","));
    }
    let t = Table::parse(&csv);
    // Only the yield: the best point is its maximum.
    let only = Recipe::Desirability {
        factors: vec![0, 1],
        responses: vec![analises::ResponseGoal {
            col: 2,
            goal: Goal::Maximize {
                low: 70.0,
                high: 83.0,
            },
            importance: 1.0,
        }],
    };
    let o = analises::run(&only, &t).unwrap();
    assert!((o.value("desejabilidade geral").unwrap() - 1.0).abs() < 1e-6);
    assert!(
        (o.value("melhor ponto A").unwrap() - 1.0).abs() < 1e-3,
        "{:?}",
        o.lines
    );
    assert!((o.value("prevista rendimento").unwrap() - 83.0).abs() < 1e-3);
    assert_eq!(only.key(), "desejabilidade");
    // With the cost to keep low, the best point gives up some yield.
    let both = Recipe::Desirability {
        factors: vec![0, 1],
        responses: vec![
            analises::ResponseGoal {
                col: 2,
                goal: Goal::Maximize {
                    low: 70.0,
                    high: 83.0,
                },
                importance: 1.0,
            },
            analises::ResponseGoal {
                col: 3,
                goal: Goal::Minimize {
                    low: 8.0,
                    high: 14.0,
                },
                importance: 1.0,
            },
        ],
    };
    let o = analises::run(&both, &t).unwrap();
    let d = o.value("desejabilidade geral").unwrap();
    assert!(d > 0.0 && d < 1.0, "{d}");
    assert!(o.value("prevista rendimento").unwrap() < 83.0);
    assert!(o.value("prevista custo").unwrap() < 13.0);
    assert_eq!(o.plots[0].labels.len(), 3);
    // A response that cannot be met.
    let never = Recipe::Desirability {
        factors: vec![0, 1],
        responses: vec![analises::ResponseGoal {
            col: 2,
            goal: Goal::Maximize {
                low: 90.0,
                high: 95.0,
            },
            importance: 1.0,
        }],
    };
    let o = analises::run(&never, &t).unwrap();
    assert_eq!(o.value("desejabilidade geral"), Some(0.0));
    assert!(o.verdict.starts_with("nenhum ponto"), "{}", o.verdict);
    // A factor column cannot also be a response.
    let bad = Recipe::Desirability {
        factors: vec![0, 1],
        responses: vec![analises::ResponseGoal {
            col: 1,
            goal: Goal::Minimize {
                low: 0.0,
                high: 1.0,
            },
            importance: 1.0,
        }],
    };
    assert!(analises::run(&bad, &t).is_err());
}
