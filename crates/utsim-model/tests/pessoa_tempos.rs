//! Tempos próprios de cada pessoa (`person.times`): o tempo da pessoa substitui o do
//! posto, a fadiga e a tolerância continuam valendo, o ritmo individual sai, os sorteios
//! dos outros blocos não mudam, o ajuste das cronometragens grava o resultado no perfil e
//! o modelo pronto de duas pessoas no mesmo torno.

use std::collections::BTreeMap;
use std::path::Path;

use utsim_core::rng::stream;
use utsim_core::{Dist, Reading, TimeStudy};
use utsim_fit::auto::AutoConfig;
use utsim_fit::fatigue::{DEFAULT_RECOVERY_PER_HOUR, fit_person, level_at};
use utsim_model::Model;

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

/// Uma pessoa opera uma máquina na mesma posição (sem caminhada). `n` peças chegam a cada
/// `gap`; a máquina leva `process` com o operador o tempo todo.
fn station(person: &str, n: u32, gap: &str, process: &str, stop: f64) -> String {
    format!(
        r#"Model(
    name: "m",
    run: (stop_time: {stop}, seed: 3),
    objects: [
        (name: "fonte", kind: Source(interarrival: {gap}, first_at_zero: true, max_arrivals: Some({n}))),
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
fn the_own_time_replaces_the_station_time() {
    for person in [
        r#"Some((times: {"maq": (dist: Const(30.0))}))"#,
        r#"Some((tempos: {"maq": (distribuicao: Const(30.0), metodo: "A")}))"#,
    ] {
        let x = metrics(
            &model(&station(person, 3, "Const(100.0)", "Const(60.0)", 1000.0)),
            0,
        );
        close(x["maq.staytime.mean"], 30.0, 1e-12, person);
        close(
            x["op.person.own_times"],
            3.0,
            0.0,
            "operações com tempo próprio",
        );
    }
    // Um tempo próprio de outra tarefa não muda nada neste posto.
    let x = metrics(
        &model(&station(
            r#"Some((times: {"carga": (dist: Const(30.0))}))"#,
            3,
            "Const(100.0)",
            "Const(60.0)",
            1000.0,
        )),
        0,
    );
    close(x["maq.staytime.mean"], 60.0, 1e-12, "outra tarefa");
    close(x["op.person.own_times"], 0.0, 0.0, "nenhuma operação");
    // Sem tempos próprios, a métrica não aparece.
    let x = metrics(
        &model(&station(
            "Some(())",
            3,
            "Const(100.0)",
            "Const(60.0)",
            1000.0,
        )),
        0,
    );
    assert!(!x.contains_key("op.person.own_times"));
}

#[test]
fn allowance_and_fatigue_apply_and_the_pace_does_not() {
    // Tolerância de 10 % sobre o tempo próprio; o ritmo individual (aqui sempre 1,25) fica
    // de fora, porque o tempo medido da pessoa já é o ritmo dela.
    let person = r#"Some((
        fatigue: Some(Allowance(0.1)),
        variation: Some((dist: Const(1.25), min: 0.5, max: 1.5)),
        times: {"maq": (dist: Const(30.0))}))"#;
    let x = metrics(
        &model(&station(person, 3, "Const(100.0)", "Const(60.0)", 1000.0)),
        0,
    );
    close(x["maq.staytime.mean"], 33.0, 1e-12, "30 s com 10 %");
    close(x["op.person.pace"], 1.25, 0.0, "ritmo sorteado");

    // Fadiga dinâmica conferida à mão, como no teste do tempo padrão: λ = 0,01, μ = 0,02,
    // tempos × (1 + 0,5·F). A primeira peça leva 100 s; a segunda, 100·(1 + 0,5·F(300)).
    let person = r#"Some((
        fatigue: Some(Dynamic(work: 0.01, recovery: 0.02, time_factor: 0.5, speed_factor: 0.0)),
        times: {"maq": (dist: Const(100.0))}))"#;
    let x = metrics(
        &model(&station(person, 2, "Const(300.0)", "Const(10.0)", 1000.0)),
        0,
    );
    let f300 = (1.0 - (-1.0f64).exp()) * (-4.0f64).exp();
    let t2 = 100.0 * (1.0 + 0.5 * f300);
    close(x["maq.staytime.mean"], (100.0 + t2) / 2.0, 1e-9, "tempos");
}

#[test]
fn the_other_draws_do_not_move() {
    // O tempo próprio vem do stream da pessoa e o posto ainda sorteia o seu: as chegadas
    // são as mesmas, peça por peça.
    let text = |person: &str| station(person, 200, "Exp(mean: 60.0)", "Exp(mean: 40.0)", 1e6);
    let a = metrics(&model(&text("Some(())")), 0);
    let b = metrics(
        &model(&text(
            r#"Some((times: {"maq": (dist: Uniform(min: 20.0, max: 40.0))}))"#,
        )),
        0,
    );
    for (k, v) in &a {
        if k.starts_with("fonte.") {
            assert_eq!(Some(v), b.get(k), "{k}");
        }
    }
    close(b["op.person.own_times"], 200.0, 0.0, "todas as peças");
    close(b["maq.staytime.mean"], 30.0, 2.0, "média do tempo próprio");
    // E a mesma semente dá o mesmo resultado.
    assert_eq!(
        b,
        metrics(
            &model(&text(
                r#"Some((times: {"maq": (dist: Uniform(min: 20.0, max: 40.0))}))"#,
            )),
            0,
        )
    );
}

#[test]
fn the_slowest_of_a_crew_sets_the_time() {
    // Dois operadores por peça: um com tempo próprio de 80 s, outro sem (tempo do posto,
    // 50 s). Quem demora mais manda.
    let text = r#"Model(
    name: "m",
    run: (stop_time: 1000.0, seed: 3),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const(200.0), first_at_zero: true, max_arrivals: Some(3))),
        (name: "fila", kind: Queue()),
        (name: "maq", kind: Processor(process: Const(50.0),
            operators: Some(OperatorUse(team: "equipe", count: 2)))),
        (name: "saida", kind: Sink),
        (name: "a", kind: Transporter(speed: 1.0), person: Some((times: {"maq": (dist: Const(80.0))}))),
        (name: "b", kind: Transporter(speed: 1.0)),
        (name: "equipe", kind: Dispatcher(team: ["a", "b"], rule: FirstAvailable)),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
)"#;
    let x = metrics(&model(text), 0);
    close(x["maq.staytime.mean"], 80.0, 1e-9, "o mais lento");
}

#[test]
fn bad_or_unused_own_times_are_reported() {
    let e = Model::parse(&station(
        r#"Some((times: {"maq": (dist: Uniform(min: 5.0, max: 1.0))}))"#,
        1,
        "Const(10.0)",
        "Const(5.0)",
        100.0,
    ))
    .unwrap()
    .validate()
    .unwrap_err()
    .to_string();
    assert!(e.contains("op.person.times[\"maq\"]"), "{e}");
    let w = model(&station(
        r#"Some((times: {"maqq": (dist: Const(5.0)), "maq:setup": (dist: Const(5.0))}))"#,
        1,
        "Const(10.0)",
        "Const(5.0)",
        100.0,
    ))
    .warnings();
    assert!(
        w.iter()
            .any(|x| x.contains("tarefa 'maqq'") && x.contains("nunca é usado")),
        "{w:?}"
    );
    assert!(!w.iter().any(|x| x.contains("'maq:setup'")), "{w:?}");
}

#[test]
fn the_fit_of_the_study_goes_into_the_run() {
    // Cronometragens sintéticas de uma pessoa (t0 = 40 s, β = 0,25, λ = 0,3 por hora) em
    // quatro horários; o ajuste grava o tempo descansado e a fadiga no perfil, e a rodada
    // passa a usar esse tempo no lugar do tempo do posto (60 s).
    let (t0, beta, work) = (40.0, 0.25, 0.3);
    let mut rng = stream(7, 0, 0);
    let base = Dist::Normal { mean: t0, sd: 2.0 };
    let readings = [0.5, 2.5, 5.0, 7.5]
        .iter()
        .map(|&h| {
            let k = 1.0 + beta * level_at(h, work, DEFAULT_RECOVERY_PER_HOUR, &[]);
            Reading {
                task: "maq".into(),
                method: String::new(),
                hour: h,
                times: (0..40).map(|_| base.sample(&mut rng) * k).collect(),
            }
        })
        .collect();
    let mut m = model(&station("Some(())", 50, "Const(100.0)", "Const(60.0)", 1e5));
    let ob = m.objects.iter_mut().find(|o| o.name == "op").unwrap();
    let person = ob.person.as_deref_mut().unwrap();
    person.study = Some(TimeStudy {
        readings,
        pauses: Vec::new(),
    });
    let fit = fit_person(person, "maq", "", &AutoConfig::default()).unwrap();
    fit.apply(person, 3600.0);
    assert!(person.times.contains_key("maq"));
    match person.fatigue {
        Some(utsim_core::Fatigue::Dynamic {
            work: w,
            time_factor,
            ..
        }) => {
            close(w * 3600.0, fit.curve.work_per_hour, 1e-9, "λ por segundo");
            close(time_factor, fit.curve.beta, 1e-12, "β");
        }
        ref other => panic!("{other:?}"),
    }
    m.validate().unwrap();
    let x = metrics(&m, 0);
    close(x["op.person.own_times"], 50.0, 0.0, "todas as peças");
    // 50 peças de cerca de 40 s, com fadiga pequena (o posto fica parado 60 % do tempo).
    let mean = x["maq.staytime.mean"];
    assert!(mean > 38.0 && mean < 46.0, "{mean}");
}

#[test]
fn two_people_with_different_curves_at_the_same_lathe() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/pessoas_tempos_proprios.ron");
    let m = model(&std::fs::read_to_string(path).unwrap());
    let lines = m.person_lines().join("\n");
    assert!(
        lines.contains("tempos próprios: torno (média 40.0, método A)"),
        "{lines}"
    );
    assert!(
        lines.contains("tempos próprios: torno (média 48.0, método A)"),
        "{lines}"
    );
    assert!(m.warnings().is_empty(), "{:?}", m.warnings());
    let x = metrics(&m, 0);
    let (a, b) = (x["ana.person.own_times"], x["bruno.person.own_times"]);
    assert!(a > 100.0 && b > 100.0, "ana {a}, bruno {b}");
    // Todas as peças do torno saíram do tempo de uma das duas pessoas.
    close(a + b, x["torno.output"], 2.0, "peças do torno");
    // ana cansa mais que bruno.
    assert!(x["ana.person.fatigue_mean"] > 2.0 * x["bruno.person.fatigue_mean"]);
}
