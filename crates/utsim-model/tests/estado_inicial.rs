//! Simular a partir de um estado dado (`InitialState`): casos determinísticos exatos,
//! validação com mensagens úteis, encadeamento de rodadas (gravar em t e retomar) e o
//! M/M/1 que parte cheio e converge para a teoria.

use utsim_core::{RunReport, Simulation, TraceEvent};
use utsim_model::{InitialState, Model};

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("o modelo é lido");
    m.validate().expect("o modelo é válido");
    m
}

fn state(text: &str) -> InitialState {
    InitialState::parse(text).expect("o estado é lido")
}

/// Instantes em que itens entram no objeto `to`, numa replicação que parte de `st`.
fn entries(m: &Model, st: &InitialState, to: &str) -> Vec<f64> {
    m.check_state(st).expect("o estado é válido");
    let mut sim = m.build_from(0, Some(st));
    let id = sim.block_id(to).unwrap().0;
    sim.enable_trace();
    sim.run().expect("a rodada termina");
    sim.take_trace()
        .into_iter()
        .filter_map(|r| match r.event {
            TraceEvent::Moved { to, .. } if to == id => Some(r.t),
            _ => None,
        })
        .collect()
}

fn assert_times(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "obtido {got:?}, esperado {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-9, "obtido {got:?}, esperado {want:?}");
    }
}

fn err_text(m: &Model, st: &InitialState) -> String {
    m.check_state(st)
        .expect_err("o estado deveria ser recusado")
        .to_string()
}

const LINE: &str = r#"Model(
    name: "linha",
    run: (stop_time: 100.0),
    objects: [
        (name: "fila", kind: Queue()),
        (name: "M1", kind: Processor(process: Const(2.0))),
        (name: "saida", kind: Sink),
    ],
    connections: [("fila", "M1"), ("M1", "saida")],
)"#;

#[test]
fn counts_and_elapsed_times_give_exact_departures() {
    let m = model(LINE);
    // Três itens na fila e um em processamento há 0.5 de 2.0: sai em 1.5, depois de 2 em 2.
    let st = state(
        r#"InitialState(objects: {
            "fila": (count: 3),
            "M1": (items: [(phase: Processing, elapsed: 0.5)]),
        })"#,
    );
    assert_times(&entries(&m, &st, "saida"), &[1.5, 3.5, 5.5, 7.5]);
    let r = m.run_replications_from(&st).unwrap().remove(0);
    let sink = r.block("saida").unwrap();
    // Tempo no sistema: o item da máquina tem idade 0.5 (2.0 ao sair); os da fila, 0.
    let tis = &sink.extra["time_in_system"];
    assert_eq!(tis.count, 4);
    assert!((tis.mean - (2.0 + 3.5 + 5.5 + 7.5) / 4.0).abs() < 1e-9);
    // Os itens pré-carregados contam no conteúdo desde t = 0, não como entradas.
    let fila = r.block("fila").unwrap();
    assert_eq!(fila.input, 0);
    assert_eq!(fila.output, 3);
    assert!(m.state_warnings(&st).iter().any(|w| w.contains("count")));
}

#[test]
fn items_keep_their_labels_and_age() {
    let m = model(LINE);
    let st = state(
        r#"InitialState(objects: {
            "fila": (items: [(elapsed: 10.0, age: 30.0, labels: {"tipo": 2.0, "cliente": "A"})]),
        })"#,
    );
    let r = m.run_replications_from(&st).unwrap().remove(0);
    // Sai da máquina em 2.0: tempo no sistema 30 + 2; permanência na fila 10.
    assert!((r.block("saida").unwrap().extra["time_in_system"].mean - 32.0).abs() < 1e-9);
    assert!((r.block("fila").unwrap().staytime.mean - 10.0).abs() < 1e-9);
}

#[test]
fn a_breakdown_in_progress_repairs_with_the_residual_time() {
    let m = model(
        r#"Model(name: "quebra", run: (stop_time: 100.0), objects: [
            (name: "M1", kind: Processor(process: Const(3.0)),
             downtime: (mtbf: Const(1000.0), mttr: Const(10.0))),
            (name: "saida", kind: Sink),
        ], connections: [("M1", "saida")])"#,
    );
    // Quebrada há 4 de 10 (volta em 6) com um item que já tinha 1 de 3: sai em 8.
    let st = state(
        r#"InitialState(objects: {"M1": (down: 4.0, items: [(phase: Processing, elapsed: 1.0)])})"#,
    );
    assert_times(&entries(&m, &st, "saida"), &[8.0]);
    let r = m.run_replications_from(&st).unwrap().remove(0);
    let states = &r.block("M1").unwrap().states;
    assert!((states["down"] - 6.0 / 100.0).abs() < 1e-9, "{states:?}");
    // Funcionando há 990 de 1000: a próxima quebra vem em 10.
    let st = state(r#"InitialState(objects: {"M1": (up: 990.0)})"#);
    let r = m.run_replications_from(&st).unwrap().remove(0);
    let down = r.block("M1").unwrap().states["down"];
    assert!((down - 10.0 / 100.0).abs() < 1e-9, "{down}");
}

#[test]
fn the_clock_starts_shifts_at_the_right_point_of_the_cycle() {
    let m = model(
        r#"Model(name: "turno", run: (stop_time: 100.0), objects: [
            (name: "M1", kind: Processor(process: Const(5.0)),
             shifts: (period: 100.0, off: [(start: 50.0, duration: 30.0)])),
            (name: "saida", kind: Sink),
        ], connections: [("M1", "saida")])"#,
    );
    // Relógio 60: fora do turno até 80, isto é, até t = 20; o item termina em 25.
    let st = state(r#"InitialState(clock: 60.0, objects: {"M1": (count: 1, off_shift: true)})"#);
    assert_times(&entries(&m, &st, "saida"), &[25.0]);
    // A janela seguinte abre em t = 90 (relógio 150 = 50 do ciclo seguinte).
    let r = m.run_replications_from(&st).unwrap().remove(0);
    let off = r.block("M1").unwrap().states["off_shift"];
    assert!((off - 30.0 / 100.0).abs() < 1e-9, "{off}");
    let wrong = state(r#"InitialState(clock: 60.0, objects: {"M1": (off_shift: false)})"#);
    assert!(err_text(&m, &wrong).contains("off_shift"));
}

#[test]
fn the_clock_starts_arrival_schedules_at_the_right_period() {
    let m = model(
        r#"Model(name: "chegadas", run: (stop_time: 100.0), objects: [
            (name: "fonte", kind: Source(interarrival: Const(10.0),
                schedule: (cycle: 100.0, periods: [(start: 0.0, end: 50.0)]))),
            (name: "saida", kind: Sink),
        ], connections: [("fonte", "saida")])"#,
    );
    // Relógio 35: chega em t = 10 (45 no ciclo); fecha em t = 15; reabre em t = 65.
    let st = state("InitialState(clock: 35.0)");
    assert_times(&entries(&m, &st, "saida"), &[10.0, 75.0, 85.0, 95.0]);
    // Com o relógio de chegadas já correndo há 4 de 10, a primeira chegada vem em 6.
    let st = state(r#"InitialState(clock: 35.0, objects: {"fonte": (elapsed: 4.0)})"#);
    assert_times(&entries(&m, &st, "saida"), &[6.0, 75.0, 85.0, 95.0]);
}

#[test]
fn conveyor_rack_and_transporter_states_are_exact() {
    let m = model(
        r#"Model(name: "materiais", run: (stop_time: 100.0), objects: [
            (name: "esteira", kind: Conveyor(length: 10.0, speed: 1.0, item_length: 1.0)),
            (name: "saida", kind: Sink),
            (name: "estoque", kind: Rack(bays: 2, levels: 2, dwell: Const(10.0))),
            (name: "saida2", kind: Sink),
            (name: "empilhadeira", kind: Transporter(speed: 1.0, home: true)),
        ], connections: [("esteira", "saida"), ("estoque", "saida2")])"#,
    );
    let st = state(
        r#"InitialState(objects: {
            "esteira": (items: [(at: 9.0), (at: 5.0)]),
            "estoque": (items: [(elapsed: 4.0, slot: 3), (phase: Done)]),
            "empilhadeira": (position: (10.0, 0.0, 0.0)),
        })"#,
    );
    assert_times(&entries(&m, &st, "saida"), &[1.0, 5.0]);
    assert_times(&entries(&m, &st, "saida2"), &[0.0, 6.0]);
    // A empilhadeira, ociosa a 10 de casa, volta para casa em 10.
    m.check_state(&st).unwrap();
    let mut sim = m.build_from(0, Some(&st));
    let id = sim.block_id("empilhadeira").unwrap().0;
    sim.enable_trace();
    sim.run().unwrap();
    let travel = sim.take_trace().into_iter().find_map(|r| match r.event {
        TraceEvent::Travel {
            carrier, duration, ..
        } if carrier == id => Some(duration),
        _ => None,
    });
    assert_eq!(travel, Some(10.0));
}

#[test]
fn an_empty_state_changes_nothing() {
    // Sem aquecimento no modelo, o estado vazio dá o mesmo relatório, número a número.
    let m = model(
        r#"Model(name: "mm1", run: (stop_time: 500.0, seed: 9, replications: 2), objects: [
            (name: "chegada", kind: Source(interarrival: Exp(mean: 1.25))),
            (name: "fila", kind: Queue()),
            (name: "M1", kind: Processor(process: Exp(mean: 1.0)),
             downtime: (mtbf: Exp(mean: 100.0), mttr: Exp(mean: 5.0)),
             shifts: (period: 100.0, off: [(start: 40.0, duration: 10.0)])),
            (name: "saida", kind: Sink),
        ], connections: [("chegada", "fila"), ("fila", "M1"), ("M1", "saida")])"#,
    );
    let plain = m.run_replications().unwrap();
    let from = m.run_replications_from(&InitialState::default()).unwrap();
    let json = |r: &[RunReport]| serde_json::to_string(r).unwrap();
    assert_eq!(json(&plain), json(&from));
}

#[test]
fn invalid_states_are_refused_with_useful_messages() {
    let m = model(
        r#"Model(name: "v", run: (stop_time: 100.0), objects: [
            (name: "fila", kind: Queue(capacity: 2)),
            (name: "M1", kind: Processor(process: Const(2.0))),
            (name: "esteira", kind: Conveyor(length: 4.0, speed: 1.0, item_length: 1.0)),
            (name: "separa", kind: Separator(process: Const(1.0))),
            (name: "saida", kind: Sink),
        ], connections: [("fila", "M1"), ("M1", "esteira"), ("esteira", "separa"),
                         ("separa", "saida")],
        fluids: (nodes: [(name: "tanque", kind: Tank(capacity: 10.0))], links: []))"#,
    );
    let cases = [
        (r#"{"nada": (count: 1)}"#, "'nada' não existe"),
        (r#"{"fila": (count: 3)}"#, "capacidade é 2"),
        (r#"{"M1": (count: 2)}"#, "capacidade é 1"),
        (r#"{"M1": (items: [(phase: Setup)])}"#, "não tem setup"),
        (r#"{"M1": (down: 3.0)}"#, "downtime"),
        (r#"{"M1": (items: [(elapsed: -1.0)])}"#, ">= 0"),
        (r#"{"fila": (count: 1, items: [()])}"#, "não os dois"),
        (r#"{"fila": (items: [(at: 1.0)])}"#, "'at'"),
        (
            r#"{"esteira": (items: [(at: 3.0), (at: 2.5)])}"#,
            "item_length",
        ),
        (r#"{"esteira": (items: [(at: 7.0)])}"#, "fora da esteira"),
        (r#"{"separa": (count: 1)}"#, "ainda não aceitam"),
        (
            r#"{"saida": (position: (1.0, 0.0, 0.0))}"#,
            "ainda não aceitam",
        ),
        (
            r#"{"fluidos": (levels: {"tanque": 12.0})}"#,
            "capacidade (10)",
        ),
        (r#"{"fluidos": (levels: {"cano": 1.0})}"#, "não é um tanque"),
        (r#"{"fila": (levels: {"tanque": 1.0})}"#, "rede de fluidos"),
    ];
    for (objects, expected) in cases {
        let st = state(&format!("InitialState(objects: {objects})"));
        let text = err_text(&m, &st);
        assert!(text.contains(expected), "{objects}: {text}");
    }
    let st = state("InitialState(warmup: 200.0)");
    assert!(err_text(&m, &st).contains("warmup"));
    assert!(
        InitialState::parse("InitialState(objetos: {\"fila\": (cont: 1)})")
            .unwrap_err()
            .to_string()
            .contains("estado inicial")
    );
    // Tanque dentro da capacidade é aceito e vira o nível inicial.
    let st = state(r#"InitialState(objects: {"fluidos": (levels: {"tanque": 7.0})})"#);
    let r = m.run_replications_from(&st).unwrap().remove(0);
    let final_level = r.block("fluidos").unwrap().custom["tanque.nivel_final"];
    assert!((final_level - 7.0).abs() < 1e-9);
}

#[test]
fn elapsed_past_the_support_warns_and_gives_zero() {
    let m = model(LINE);
    let st = state(r#"InitialState(objects: {"M1": (items: [(elapsed: 5.0)])})"#);
    let warnings = m.check_state(&st).unwrap();
    assert!(
        warnings.iter().any(|w| w.contains("além do máximo")),
        "{warnings:?}"
    );
    assert_times(&entries(&m, &st, "saida"), &[0.0]);
}

/// Linha determinística com tudo que guarda estado: fonte, fila, máquina com setup,
/// quebras e turnos, esteira, porta-paletes e uma rede de fluidos.
const DETERMINISTIC: &str = r#"Model(
    name: "encadeada",
    run: (stop_time: 400.0),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const(1.3141592), labels: {"lote": Const(7.0)})),
        (name: "fila", kind: Queue()),
        (name: "M1", kind: Processor(setup: Const(0.2377), process: Const(1.0931)),
         downtime: (mtbf: Const(37.1713), mttr: Const(4.4319)),
         shifts: (period: 60.0, off: [(start: 41.37, duration: 6.2113)])),
        (name: "esteira", kind: Conveyor(length: 6.0, speed: 0.8317, item_length: 1.0)),
        (name: "estoque", kind: Rack(bays: 3, levels: 2, dwell: Const(9.4719))),
        (name: "saida", kind: Sink),
    ],
    connections: [("fonte", "fila"), ("fila", "M1"), ("M1", "esteira"), ("esteira", "estoque"),
                  ("estoque", "saida")],
    fluids: (nodes: [
        (name: "rede", kind: Source(rate: 0.5)),
        (name: "tanque", kind: Tank(capacity: 1000.0, initial: 3.0)),
        (name: "uso", kind: Sink(max_rate: 0.2)),
    ], links: [("rede", "tanque"), ("tanque", "uso")]),
)"#;

/// Rodada contínua com aquecimento `t` (estatísticas de t até o fim) e a rodada que parte
/// do estado gravado em `t`, com horizonte igual ao que faltava.
fn continuous_and_resumed(text: &str, t: f64, r: u32) -> (RunReport, RunReport, InitialState) {
    let m = model(text);
    let mut cont = m.clone();
    cont.run.warmup = t;
    let mut sim = cont.build(r);
    let continuous = sim.run().unwrap();
    let (st, _notes) = m.state_at(t, None, r).unwrap();
    let mut rest = m.clone();
    rest.run.stop_time = m.run.stop_time - t;
    rest.check_state(&st).unwrap();
    let mut sim: Simulation = rest.build_from(r, Some(&st));
    let resumed = sim.run().unwrap();
    (continuous, resumed, st)
}

#[test]
fn a_deterministic_run_resumes_exactly_from_a_saved_state() {
    for t in [57.3, 150.0, 203.65] {
        let (continuous, resumed, st) = continuous_and_resumed(DETERMINISTIC, t, 0);
        // O arquivo vai e volta sem perda.
        assert_eq!(InitialState::parse(&st.to_ron()).unwrap(), st);
        assert!((st.clock - t).abs() < 1e-12);
        for name in ["fila", "M1", "esteira", "estoque", "saida"] {
            let a = continuous.block(name).unwrap();
            let b = resumed.block(name).unwrap();
            assert_eq!(a.output, b.output, "{name} em t={t}: saídas");
            assert!(
                (a.content_mean - b.content_mean).abs() < 1e-6,
                "{name} em t={t}: conteúdo {} vs {}",
                a.content_mean,
                b.content_mean
            );
            for (state, frac) in &a.states {
                let other = b.states.get(state).copied().unwrap_or(0.0);
                assert!(
                    (frac - other).abs() < 1e-6,
                    "{name} em t={t}: estado {state} {frac} vs {other}"
                );
            }
        }
        let tis = |r: &RunReport| r.block("saida").unwrap().extra["time_in_system"].mean;
        assert!((tis(&continuous) - tis(&resumed)).abs() < 1e-6);
        let level = |r: &RunReport| r.block("fluidos").unwrap().custom["tanque.nivel_final"];
        assert!((level(&continuous) - level(&resumed)).abs() < 1e-6);
    }
}

#[test]
fn saving_twice_chains_runs() {
    // 0 → 100 → 250 → 400 em três pernas dá o mesmo que a rodada contínua.
    let m = model(DETERMINISTIC);
    let (s1, _) = m.state_at(100.0, None, 0).unwrap();
    let mut leg = m.clone();
    leg.run.stop_time = 300.0;
    let (s2, _) = leg.state_at(150.0, Some(&s1), 0).unwrap();
    assert!((s2.clock - 250.0).abs() < 1e-12);
    let mut last = m.clone();
    last.run.stop_time = 150.0;
    let resumed = last.run_replications_from(&s2).unwrap().remove(0);
    let mut cont = m.clone();
    cont.run.warmup = 250.0;
    let continuous = cont.run_replications().unwrap().remove(0);
    assert_eq!(
        continuous.block("saida").unwrap().input,
        resumed.block("saida").unwrap().input
    );
}

/// M/M/1 com quebras exponenciais e um tempo de setup Gamma (que não é sem memória).
const RANDOM: &str = r#"Model(
    name: "aleatoria",
    run: (stop_time: 3000.0, seed: 5),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.4))),
        (name: "fila", kind: Queue()),
        (name: "M1", kind: Processor(setup: Gamma(shape: 2.0, scale: 0.1),
                                      process: Weibull(shape: 2.0, scale: 1.0)),
         downtime: (mtbf: Weibull(shape: 1.5, scale: 60.0), mttr: Triangular(min: 1.0, mode: 2.0, max: 6.0))),
        (name: "saida", kind: Sink),
    ],
    connections: [("chegada", "fila"), ("fila", "M1"), ("M1", "saida")],
)"#;

#[test]
fn resumed_random_runs_are_statistically_compatible() {
    // Não são idênticas (os residuais são novos sorteios), mas as médias entre
    // replicações das duas formas ficam dentro do erro amostral.
    let t = 1000.0;
    let n = 40;
    let mut a = Vec::new();
    let mut b = Vec::new();
    for r in 0..n {
        let (continuous, resumed, _) = continuous_and_resumed(RANDOM, t, r);
        let pick = |rep: &RunReport| {
            let m1 = rep.block("M1").unwrap();
            [
                rep.block("fila").unwrap().content_mean,
                m1.states.get("processing").copied().unwrap_or(0.0),
                m1.states.get("down").copied().unwrap_or(0.0),
                rep.block("saida").unwrap().input as f64,
            ]
        };
        a.push(pick(&continuous));
        b.push(pick(&resumed));
    }
    let names = ["conteúdo da fila", "processando", "quebrada", "saídas"];
    for k in 0..4 {
        let xs: Vec<f64> = a.iter().map(|v| v[k]).collect();
        let ys: Vec<f64> = b.iter().map(|v| v[k]).collect();
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let var = |v: &[f64]| {
            let m = mean(v);
            v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64
        };
        let se = ((var(&xs) + var(&ys)) / n as f64).sqrt();
        let diff = (mean(&xs) - mean(&ys)).abs();
        assert!(
            diff <= 4.0 * se + 1e-9,
            "{}: contínua {} vs retomada {} (erro padrão {se})",
            names[k],
            mean(&xs),
            mean(&ys)
        );
    }
}

#[test]
fn mm1_started_full_converges_to_theory() {
    // λ = 0.8, μ = 1: Lq = ρ²/(1-ρ) = 3.2 e utilização 0.8. Parte com 60 na fila e a
    // máquina ocupada; o aquecimento de 2000 (configurável no estado) apaga o transiente.
    let m = model(
        r#"Model(name: "mm1", run: (stop_time: 42000.0, seed: 3, replications: 8), objects: [
            (name: "chegada", kind: Source(interarrival: Exp(mean: 1.25))),
            (name: "fila", kind: Queue()),
            (name: "M1", kind: Processor(process: Exp(mean: 1.0))),
            (name: "saida", kind: Sink),
        ], connections: [("chegada", "fila"), ("fila", "M1"), ("M1", "saida")])"#,
    );
    let st = state(
        r#"InitialState(warmup: 2000.0, objects: {
            "fila": (count: 60),
            "M1": (items: [(elapsed: 0.7)]),
        })"#,
    );
    let reports = m.run_replications_from(&st).unwrap();
    let n = reports.len() as f64;
    let lq = reports
        .iter()
        .map(|r| r.block("fila").unwrap().content_mean)
        .sum::<f64>()
        / n;
    let rho = reports
        .iter()
        .map(|r| r.block("M1").unwrap().states["processing"])
        .sum::<f64>()
        / n;
    assert!((lq - 3.2).abs() / 3.2 < 0.08, "Lq = {lq}");
    assert!((rho - 0.8).abs() < 0.01, "ρ = {rho}");
    // Sem aquecimento, o começo cheio pesa na média de uma rodada curta.
    let mut short = m.clone();
    short.run.stop_time = 300.0;
    let st0 = state(r#"InitialState(objects: {"fila": (count: 60)})"#);
    let early = short.run_replications_from(&st0).unwrap();
    let lq0 = early
        .iter()
        .map(|r| r.block("fila").unwrap().content_mean)
        .sum::<f64>()
        / early.len() as f64;
    assert!(lq0 > 10.0, "Lq inicial = {lq0}");
}
