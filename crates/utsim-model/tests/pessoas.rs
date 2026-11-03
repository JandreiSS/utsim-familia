//! Características físicas de pessoas (`person` num Transporter): perfil vazio = resultado
//! idêntico, tolerância exata, fadiga dinâmica e pausa conferidas à mão, curva de Wright,
//! caminhada com carga, habilidades no despachante, ritmo individual em stream próprio,
//! determinismo, avisos de carga manual e estado gravado e retomado.

use std::collections::BTreeMap;
use std::path::Path;

use utsim_model::{InitialState, Model};

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("o modelo é lido");
    m.validate().expect("o modelo é válido");
    m
}

fn metrics(m: &Model, r: u32) -> BTreeMap<String, f64> {
    m.build(r).run().expect("a rodada termina").metrics()
}

fn close(got: f64, want: f64, tol: f64, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: obtido {got}, esperado {want} ± {tol}"
    );
}

fn example(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    std::fs::read_to_string(path).expect("o exemplo existe")
}

/// Uma pessoa opera uma máquina na mesma posição (sem caminhada). `n` peças chegam a cada
/// `gap`; a máquina leva `process` com o operador o tempo todo.
fn station(person: &str, n: u32, gap: f64, process: &str, stop: f64) -> String {
    format!(
        r#"Model(
    name: "m",
    run: (stop_time: {stop}, seed: 3),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const({gap}), first_at_zero: true, max_arrivals: Some({n}))),
        (name: "fila", kind: Queue()),
        (name: "maq", kind: Processor(process: {process},
            operators: Some(OperatorUse(team: "op")))),
        (name: "saida", kind: Sink),
        (name: "op", kind: Transporter(speed: 1.0), person: {person}),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
)"#
    )
}

#[test]
fn empty_profile_changes_nothing() {
    // Os exemplos com pessoas, com um perfil vazio em cada Transporter: as métricas são as
    // mesmas, só aparecem as de pessoa.
    for file in [
        "celula_com_operadores.ron",
        "pessoas_carregando.ron",
        "tutorial_03_operadores.ron",
    ] {
        let mut base = Model::parse(&example(file)).unwrap();
        base.run.stop_time = base.run.stop_time.min(20_000.0);
        let mut with = base.clone();
        for o in &mut with.objects {
            if matches!(o.kind, utsim_model::ObjectKind::Transporter { .. }) {
                o.person = Some(Box::default());
            }
        }
        with.validate().unwrap();
        for r in 0..2 {
            let a = metrics(&base, r);
            let mut b = metrics(&with, r);
            b.retain(|k, _| !k.contains(".person."));
            assert_eq!(a, b, "{file}, replicação {r}");
        }
    }
}

#[test]
fn allowance_multiplies_the_time_exactly() {
    let m = model(&station(
        "Some((fatigue: Some(Allowance(0.15))))",
        3,
        100.0,
        "Const(10.0)",
        1000.0,
    ));
    let x = metrics(&m, 0);
    close(x["maq.staytime.mean"], 11.5, 1e-12, "tempo com 15 %");
    // A tolerância não tem estado: nenhuma métrica de nível de fadiga.
    assert!(!x.contains_key("op.person.fatigue_mean"));
}

#[test]
fn dynamic_fatigue_by_hand() {
    // λ = 0,01 no posto, μ = 0,02 parado, tempos × (1 + 0,5·F). Peça 1 em t = 0 (F = 0):
    // 100 s; F(100) = 1 − e^−1. Parado até 300: F(300) = F(100)·e^−4. Peça 2 leva
    // 100·(1 + 0,5·F(300)), e depois a pessoa descansa até 1000.
    let m = model(&station(
        "Some((fatigue: Some(Dynamic(work: 0.01, recovery: 0.02, time_factor: 0.5, speed_factor: 0.0))))",
        2,
        300.0,
        "Const(100.0)",
        1000.0,
    ));
    let x = metrics(&m, 0);
    let f100 = 1.0 - (-1.0f64).exp();
    let f300 = f100 * (-4.0f64).exp();
    let t2 = 100.0 * (1.0 + 0.5 * f300);
    close(x["maq.staytime.mean"], (100.0 + t2) / 2.0, 1e-9, "tempos");
    let f_end_work = 1.0 - (1.0 - f300) * (-0.01 * t2).exp();
    let f_end = f_end_work * (-0.02 * (1000.0 - 300.0 - t2)).exp();
    close(x["op.person.fatigue_end"], f_end, 1e-12, "F no fim");
    close(x["op.person.fatigue_max"], f_end_work, 1e-12, "F máxima");
    // Média: integrais fechadas de cada trecho.
    let work = |f0: f64, l: f64, t: f64| t - (1.0 - f0) * (1.0 - (-l * t).exp()) / l;
    let rest = |f0: f64, m: f64, t: f64| f0 * (1.0 - (-m * t).exp()) / m;
    let area = work(0.0, 0.01, 100.0)
        + rest(f100, 0.02, 200.0)
        + work(f300, 0.01, t2)
        + rest(f_end_work, 0.02, 1000.0 - 300.0 - t2);
    close(x["op.person.fatigue_mean"], area / 1000.0, 1e-9, "F média");
}

#[test]
fn pause_when_the_limit_is_reached() {
    // F(100) = 1 − e^−1 ≈ 0,632 ≥ 0,5: pausa até 0,2, que dura ln(F/0,2)/μ.
    let m = model(&station(
        "Some((fatigue: Some(Dynamic(work: 0.01, recovery: 0.02, limit: Some(0.5), target: 0.2))))",
        1,
        100.0,
        "Const(100.0)",
        1000.0,
    ));
    let x = metrics(&m, 0);
    let f = 1.0 - (-1.0f64).exp();
    let pause = (f / 0.2).ln() / 0.02;
    close(x["op.person.rests"], 1.0, 0.0, "pausas");
    close(x["op.person.rest_time"], pause, 1e-9, "tempo em pausa");
    close(
        x["op.state.resting"],
        pause / 1000.0,
        1e-12,
        "fração em pausa",
    );
    // Tempo acima do limite: de quando F cruza 0,5 trabalhando até quando cai de volta.
    let up = 100.0 - (1.0f64 / 0.5).ln() / 0.01;
    let down = (f / 0.5).ln() / 0.02;
    close(
        x["op.person.time_above_limit"],
        up + down,
        1e-9,
        "acima do limite",
    );
}

#[test]
fn shift_break_recovers() {
    // Com intervalo de turno a pessoa fica `off_shift` e recupera: o tempo de intervalo
    // vira métrica e a fadiga média é menor que sem o intervalo.
    let text = |shifts: &str| {
        format!(
            r#"Model(
    name: "m",
    run: (stop_time: 4000.0, seed: 3),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const(60.0), first_at_zero: true)),
        (name: "fila", kind: Queue()),
        (name: "maq", kind: Processor(process: Const(50.0), operators: Some(OperatorUse(team: "op")))),
        (name: "saida", kind: Sink),
        (name: "op", kind: Transporter(speed: 1.0){shifts},
            person: Some((fatigue: Some(Dynamic(work: 0.001, recovery: 0.005))))),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
)"#
        )
    };
    let without = metrics(&model(&text("")), 0);
    let with = metrics(
        &model(&text(
            ", shifts: (period: 2000.0, off: [(start: 1000.0, duration: 500.0)])",
        )),
        0,
    );
    close(with["op.person.break_time"], 1000.0, 1e-9, "intervalos");
    assert!(with["op.person.fatigue_mean"] < without["op.person.fatigue_mean"]);
}

#[test]
fn wright_learning_curve() {
    // r = 0,85, 1.ª repetição 3× o padrão de 10 s: T_n = 30·n^(−b), b = −log2(0,85).
    // T_8 = T_1·0,85³.
    let m = model(&station(
        "Some((learning: Some((rate: 0.85, first: 3.0))))",
        8,
        100.0,
        "Const(10.0)",
        1000.0,
    ));
    let x = metrics(&m, 0);
    let b = -(0.85f64).log2();
    let times: Vec<f64> = (1..=8).map(|n| 30.0 * (n as f64).powf(-b)).collect();
    close(times[7], times[0] * 0.85f64.powi(3), 1e-12, "T8 = T1·r³");
    let mean = times.iter().sum::<f64>() / 8.0;
    close(x["maq.staytime.mean"], mean, 1e-9, "tempo médio na máquina");
    close(x["op.person.repetitions"], 8.0, 0.0, "repetições");
    close(
        x["op.person.learning_factor"],
        mean / 10.0,
        1e-12,
        "fator médio",
    );
    close(
        x["op.person.learning_gain"],
        1.0 - mean / 30.0,
        1e-12,
        "ganho de aprendizado",
    );
}

#[test]
fn loaded_walk_is_slower() {
    // Caixa de 25 kg por 10 m a 1 m/s: −1 %/kg acima de 5 kg → 0,8 m/s, 12,5 s carregado.
    let text = |person: &str| {
        format!(
            r#"Model(
    name: "m",
    run: (stop_time: 100.0, seed: 3),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const(1000.0), first_at_zero: true,
            labels: {{"weight": Const(25.0)}}), position: (0.0, 0.0, 0.0)),
        (name: "fila", kind: Queue(), transport: "p", position: (0.0, 0.0, 0.0)),
        (name: "saida", kind: Sink, position: (10.0, 0.0, 0.0)),
        (name: "p", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0), person: {person}),
    ],
    connections: [("fonte", "fila"), ("fila", "saida")],
)"#
        )
    };
    let plain = metrics(&model(&text("None")), 0);
    close(
        plain["p.state.travel_loaded"] * 100.0,
        10.0,
        1e-9,
        "sem perfil",
    );
    let loaded = metrics(&model(&text("Some((walk_speed_loaded: Some(())))")), 0);
    close(
        loaded["p.state.travel_loaded"] * 100.0,
        12.5,
        1e-9,
        "com carga",
    );
}

/// Equipe "eq" com `a` (sem a habilidade, primeira da equipe) e `b` (com "torno").
fn skills_model(b_skills: &str) -> String {
    format!(
        r#"Model(
    name: "m",
    run: (stop_time: 2000.0, seed: 3),
    objects: [
        (name: "fonte", kind: Source(interarrival: Exp(mean: 30.0)), position: (0.0, 0.0, 0.0)),
        (name: "fila", kind: Queue(), position: (2.0, 0.0, 0.0)),
        (name: "maq", kind: Processor(process: Const(10.0),
            operators: Some(OperatorUse(team: "eq", requires: "torno"))), position: (5.0, 0.0, 0.0)),
        (name: "saida", kind: Sink, position: (8.0, 0.0, 0.0)),
        (name: "a", kind: Transporter(speed: 1.0), position: (5.0, 2.0, 0.0),
            person: Some((skills: ["solda"]))),
        (name: "b", kind: Transporter(speed: 1.0), position: (5.0, -4.0, 0.0),
            person: Some((skills: {b_skills}))),
        (name: "eq", kind: Dispatcher(team: ["a", "b"], rule: FirstAvailable), position: (5.0, 0.0, 0.0)),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
)"#
    )
}

#[test]
fn skills_restrict_who_operates() {
    let m = model(&skills_model(r#"["torno"]"#));
    assert!(m.warnings().iter().all(|w| !w.contains("habilidade")));
    let x = metrics(&m, 0);
    assert_eq!(x.get("a.state.utilize"), None, "a não sabe operar o torno");
    assert!(x["b.state.utilize"] > 0.0);
    assert!(x["saida.input"] > 10.0);
    // Sem a exigência, a primeira livre (a, mais perto) também opera.
    let open = model(&skills_model(r#"["torno"]"#).replace(r#", requires: "torno""#, ""));
    assert!(metrics(&open, 0)["a.state.utilize"] > 0.0);
    // Ninguém habilitado: aviso.
    let none = model(&skills_model("[]"));
    assert!(
        none.warnings()
            .iter()
            .any(|w| w.contains("ninguém da equipe 'eq' tem a habilidade 'torno'")),
        "{:?}",
        none.warnings()
    );
}

#[test]
fn variation_has_its_own_stream() {
    // Ligar o ritmo individual não muda as chegadas (stream da fonte) e sorteia ritmos
    // diferentes por replicação, dentro de [0,7; 1,3].
    let base = Model::parse(&example("pessoas_carregando.ron")).unwrap();
    let mut with = base.clone();
    for o in &mut with.objects {
        if matches!(o.kind, utsim_model::ObjectKind::Transporter { .. }) {
            o.person = Some(Box::new(utsim_core::Person {
                variation: Some(Default::default()),
                ..Default::default()
            }));
        }
    }
    with.validate().unwrap();
    let mut paces = Vec::new();
    for r in 0..3 {
        let a = metrics(&base, r);
        let b = metrics(&with, r);
        assert_eq!(a["chegada.output"], b["chegada.output"], "replicação {r}");
        let pace = b["ana.person.pace"];
        assert!((0.7..=1.3).contains(&pace));
        paces.push(pace);
    }
    assert!(paces[0] != paces[1] || paces[1] != paces[2]);
}

#[test]
fn example_is_deterministic_and_shows_the_metrics() {
    let m = model(&example("pessoas_fadiga.ron"));
    let a = m.run_replications().unwrap();
    let b = m.run_replications().unwrap();
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.metrics(), y.metrics());
    }
    let x = a[0].metrics();
    for key in [
        "ana.person.fatigue_mean",
        "ana.person.fatigue_max",
        "ana.person.rest_time",
        "ana.person.break_time",
        "ana.person.learning_gain",
        "bruno.person.pace",
    ] {
        assert!(x.contains_key(key), "falta {key}");
    }
    // Bruno não sabe operar o torno.
    assert_eq!(x.get("bruno.state.utilize"), None);
    assert!(x["ana.state.utilize"] + x["carla.state.utilize"] > 0.0);
    // Uma replicação isolada dá o mesmo que dentro de run_replications (paralelo).
    assert_eq!(metrics(&m, 1), a[1].metrics());
    // Os exemplos não geram avisos: com a geometria padrão o limite é 23 kg.
    assert!(m.warnings().is_empty(), "{:?}", m.warnings());
    assert!(
        m.person_lines()
            .iter()
            .any(|l| l.starts_with("ana: fadiga dinâmica"))
    );
}

#[test]
fn saved_state_keeps_fatigue_and_experience() {
    let m = model(&example("pessoas_fadiga.ron"));
    let (saved, _notes) = m.state_at(3000.0, None, 0).unwrap();
    let ana = &saved.objects["ana"];
    let f = ana.fatigue.expect("a fadiga é gravada");
    assert!(f > 0.0 && f < 1.0);
    let done = ana.experience.get("torno").copied().unwrap_or(0)
        + saved.objects["carla"]
            .experience
            .get("torno")
            .copied()
            .unwrap_or(0);
    assert!(done > 0, "repetições do torno gravadas");
    // Texto, leitura e retomada: no instante 0 o estado é o mesmo.
    let back = InitialState::parse(&saved.to_ron()).unwrap();
    assert_eq!(back, saved);
    m.check_state(&back).unwrap();
    let mut again = m.build_from(0, Some(&back));
    again.begin().unwrap();
    let (resaved, _) = m.save_state(&mut again);
    close(
        resaved.objects["ana"].fatigue.unwrap(),
        f,
        1e-12,
        "fadiga retomada",
    );
    assert_eq!(resaved.objects["ana"].experience, ana.experience);
    // A rodada retomada termina.
    again.run().unwrap();
    // Fadiga num objeto sem perfil: erro.
    let mut bad = back.clone();
    bad.objects.insert(
        "estoque".into(),
        utsim_core::BlockState {
            fatigue: Some(0.3),
            ..Default::default()
        },
    );
    let mut sim = m.build_from(0, Some(&bad));
    assert!(sim.begin().is_err());
}

#[test]
fn invalid_profiles_are_reported() {
    let bad = |person: &str, kind: &str| {
        let text = format!(
            r#"Model(name: "m", run: (stop_time: 10.0), objects: [
                (name: "f", kind: Source(interarrival: Const(1.0))),
                (name: "x", kind: {kind}, person: {person}),
                (name: "s", kind: Sink)],
                connections: [("f", "s")])"#
        );
        Model::parse(&text)
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string()
    };
    assert!(bad("Some(())", "Queue()").contains("só vale para um Transporter"));
    let t = "Transporter(speed: 1.0)";
    assert!(bad("Some((fatigue: Some(Allowance(-0.1))))", t).contains("Allowance"));
    assert!(bad("Some((learning: Some((rate: 1.5))))", t).contains("rate"));
    assert!(
        bad(
            "Some((fatigue: Some(Dynamic(limit: Some(0.3), target: 0.5))))",
            t
        )
        .contains("target")
    );
    assert!(bad(r#"Some((skills: [""]))"#, t).contains("skills"));
}

#[test]
fn manual_lifting_warnings() {
    // H = 40, V = 30, D = 50: RWL = 23·0,625·0,865·0,91 ≈ 11,3 kg; 18 kg dá LI ≈ 1,59.
    let mut m = model(&example("pessoas_fadiga.ron"));
    let lift = utsim_core::Lift {
        horizontal: 40.0,
        vertical: 30.0,
        distance: 50.0,
        ..Default::default()
    };
    for o in &mut m.objects {
        if let Some(p) = o.person.as_mut() {
            p.lift = Some(lift);
        }
    }
    let w = m.warnings();
    assert_eq!(w.len(), 3, "{w:?}");
    assert!(w.iter().all(|x| x.contains("NIOSH")
        && x.contains("11.3 kg")
        && x.contains("1.59 > 1")
        && x.contains("[conferir no texto vigente]")));
    // Acima de 3: alerta; acima de 60 kg: referência da CLT.
    if let utsim_model::ObjectKind::Source { labels, .. } = &mut m.objects[0].kind {
        labels.insert("weight".into(), utsim_core::Dist::Const(70.0));
    }
    let w = m.warnings();
    assert!(w.iter().any(|x| x.contains("ALERTA")), "{w:?}");
    assert!(w.iter().any(|x| x.contains("art. 198")), "{w:?}");
}
