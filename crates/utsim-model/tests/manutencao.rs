//! Manutenção de máquinas (`downtime` com equipe, desgaste, preventiva, custos e OEE):
//! equivalência com as quebras simples, resultado de livro da manutenção por idade, fila
//! do técnico, preventiva que espera a peça, OEE calculado à mão, determinismo e estado
//! gravado e retomado.

use std::collections::BTreeMap;

use utsim_core::State;
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

/// Linha de uma máquina sempre ocupada: chega uma peça a cada 0,9 e a máquina leva 1,
/// então a fila nunca esvazia.
fn busy_line(stop: f64, downtime: &str) -> String {
    format!(
        r#"Model(
    name: "m",
    run: (stop_time: {stop}, seed: 7),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const(0.9), first_at_zero: true)),
        (name: "fila", kind: Queue()),
        (name: "maq", kind: Processor(process: Const(1.0)), downtime: {downtime}),
        (name: "saida", kind: Sink),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
)"#
    )
}

#[test]
fn weibull_shape_1_is_the_current_exponential() {
    // Mesma semente, mesmo stream: Weibull(1, 100) com reparo perfeito sorteia
    // exatamente os mesmos tempos que Exp(mean: 100), e a linha dá o mesmo resultado.
    let text = |extra: &str| {
        format!(
            r#"Model(
    name: "m",
    run: (stop_time: 20000.0, warmup: 500.0, seed: 11, replications: 2),
    objects: [
        (name: "fonte", kind: Source(interarrival: Exp(mean: 2.0))),
        (name: "fila", kind: Queue()),
        (name: "maq", kind: Processor(process: Exp(mean: 1.5)),
         downtime: (mtbf: Exp(mean: 100.0), mttr: Exp(mean: 8.0){extra})),
        (name: "saida", kind: Sink),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
)"#
        )
    };
    let plain = model(&text(""));
    let worn = model(&text(
        ", wear: Weibull(shape: 1.0, scale: 100.0), repair: AsGoodAsNew",
    ));
    for r in 0..2 {
        let a = metrics(&plain, r);
        let mut b = metrics(&worn, r);
        b.retain(|k, _| !k.starts_with("maq.oee") && !k.starts_with("maq.maintenance."));
        assert_eq!(a, b, "replicação {r}");
    }
}

#[test]
fn shape_1_minimal_repair_matches_the_exponential_rate() {
    // Reparo mínimo com forma 1 continua sem memória: falhas ≈ uso / escala.
    let m = model(&busy_line(
        200_000.0,
        "(mtbf: Exp(mean: 1.0), mttr: Const(0.0), wear: Weibull(shape: 1.0, scale: 50.0), basis: Busy)",
    ));
    let x = metrics(&m, 0);
    let failures = x["maq.maintenance.failures"];
    let expected = 200_000.0 / 50.0;
    close(failures, expected, 4.0 * expected.sqrt(), "falhas");
    // MTBF observado dentro do intervalo do teórico.
    close(
        x["maq.maintenance.mtbf"],
        50.0,
        4.0 * 50.0 / expected.sqrt(),
        "MTBF",
    );
}

#[test]
fn age_based_preventive_matches_the_textbook_cost_rate() {
    // Desgaste Weibull(β = 2, η = 100) por uso, reparo mínimo, preventiva que renova.
    // Substituição periódica com reparo mínimo (Barlow e Hunter, 1960): num ciclo de uso
    // T o número esperado de falhas é H(T) = (T/η)^β, e o custo por unidade de tempo é
    // C(T) = (c_p + c_f·H(T)) / T. Reparos e preventivas instantâneos: o relógio é o uso.
    let (eta, beta, cp, cf) = (100.0, 2.0, 40.0, 150.0);
    let (stop, reps) = (60_000.0, 3);
    let with_pm = model(&busy_line(
        stop,
        "(mtbf: Exp(mean: 1.0), mttr: Const(0.0), wear: Weibull(shape: 2.0, scale: 100.0),          basis: Busy, preventive: [(every: Hours(50.0), duration: Const(0.0))],          cost: (per_repair: 150.0, per_preventive: 40.0))",
    ));
    let (mut pms, mut failures, mut cost) = (0.0, 0.0, 0.0);
    for r in 0..reps {
        let x = metrics(&with_pm, r);
        pms += x["maq.maintenance.preventives"];
        failures += x["maq.maintenance.failures"];
        cost += x["maq.maintenance.cost"];
    }
    let total = stop * reps as f64;
    // A preventiva espera a peça em processo: o ciclo real é o uso entre duas preventivas.
    let t: f64 = total / pms;
    // (o último ciclo, incompleto, entra no tempo e não nas preventivas).
    assert!((50.0..=52.0).contains(&t), "ciclo de {t}");
    let h = (t / eta).powf(beta);
    let expected = pms * h;
    let sd = expected.sqrt();
    close(failures, expected, 4.0 * sd, "falhas com preventiva");
    // Custo: só as falhas são aleatórias (Poisson), então o desvio vem delas.
    let book = (cp + cf * h) / t;
    close(
        cost / total,
        book,
        4.0 * cf * sd / total,
        "custo por unidade de tempo",
    );

    // Sem preventiva a máquina envelhece: H(L) = (L/η)^β falhas em L de uso.
    let short = 2_000.0;
    let without = model(&busy_line(
        short,
        "(mtbf: Exp(mean: 1.0), mttr: Const(0.0), wear: Weibull(shape: 2.0, scale: 100.0), basis: Busy)",
    ));
    let y = metrics(&without, 0);
    let expected = (short / eta).powf(beta);
    close(
        y["maq.maintenance.failures"],
        expected,
        4.0 * expected.sqrt(),
        "falhas sem preventiva",
    );
    // A preventiva reduz as falhas por unidade de uso.
    assert!(failures / total < y["maq.maintenance.failures"] / short / 10.0);
}

/// Três máquinas paradas (sem peças) e um técnico na origem, em linha reta (formato 1).
fn crew_model(m3_mtbf: &str, m2_extra: &str) -> Model {
    model(&format!(
        r#"Model(
    name: "tecnico",
    run: (stop_time: 51.0),
    objects: [
        (name: "f1", kind: Source(interarrival: Const(1000.0)), position: (10.0, 5.0, 0.0)),
        (name: "M1", kind: Processor(process: Const(1.0)), position: (10.0, 0.0, 0.0),
         downtime: (mtbf: Const(10.0), mttr: Const(5.0), repair_crew: "tecnico")),
        (name: "f2", kind: Source(interarrival: Const(1000.0)), position: (-10.0, 5.0, 0.0)),
        (name: "M2", kind: Processor(process: Const(1.0)), position: (-10.0, 0.0, 0.0),
         downtime: (mtbf: Const(10.0), mttr: Const(5.0), repair_crew: "tecnico"{m2_extra})),
        (name: "f3", kind: Source(interarrival: Const(1000.0)), position: (-10.0, -5.0, 0.0)),
        (name: "M3", kind: Processor(process: Const(1.0)), position: (-10.0, 0.0, 0.0),
         downtime: (mtbf: {m3_mtbf}, mttr: Const(5.0), repair_crew: "tecnico")),
        (name: "saida", kind: Sink, position: (0.0, 10.0, 0.0)),
        (name: "tecnico", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0)),
    ],
    connections: [("f1", "M1"), ("M1", "saida"), ("f2", "M2"), ("M2", "saida"), ("f3", "M3"), ("M3", "saida")],
)"#
    ))
}

#[test]
fn one_technician_two_machines_queue() {
    // M1 e M2 quebram juntas em t = 11 (a primeira peça termina em 1 e o mtbf corre no
    // relógio desde 0: quebram em 10). M1 pede primeiro: o técnico anda 10 até M1
    // (chega em 20), repara até 25, anda 20 até M2 (chega em 45) e repara até 50.
    let m = crew_model("Const(1.0e9)", "");
    let x = metrics(&m, 0);
    close(x["M1.maintenance.wait_technician"], 10.0, 1e-9, "espera M1");
    close(x["M2.maintenance.wait_technician"], 35.0, 1e-9, "espera M2");
    close(x["M1.maintenance.mttr"], 15.0, 1e-9, "MTTR M1");
    close(x["M2.maintenance.mttr"], 40.0, 1e-9, "MTTR M2");
    close(x["M2.maintenance.repair_time"], 5.0, 1e-9, "reparo M2");
    // M1 quebrou de novo em 35 e espera o técnico (ainda não reparada).
    assert_eq!(x["M1.maintenance.failures"], 2.0);
    assert_eq!(x["M1.maintenance.repairs"], 1.0);
    assert!(x["M1.state.waiting_repair"] > 0.0);
    // A espera em aberto entra no total: 10 da primeira + 16 da segunda.
    close(
        x["M1.maintenance.wait_technician_total"],
        26.0,
        1e-9,
        "espera total M1",
    );
}

#[test]
fn corrective_goes_before_preventive() {
    // M1 quebra em 10 e ocupa o técnico até 25. A preventiva de M2 fica devida em 12 e
    // M3 quebra em 14; M3 (corretiva) vai na frente: chega em 45 (espera 31). M1 quebra
    // de novo em 35 e também passa na frente da preventiva, que em 51 ainda espera.
    let m = crew_model(
        "Const(14.0)",
        ", preventive: [(every: Clock(12.0), duration: Const(1.0))]",
    );
    let m = {
        let mut m = m;
        // M2 não quebra: só a preventiva.
        let o = m.objects.iter_mut().find(|o| o.name == "M2").unwrap();
        o.downtime.as_mut().unwrap().mtbf = utsim_core::Dist::Const(1.0e9);
        m
    };
    let x = metrics(&m, 0);
    close(x["M3.maintenance.wait_technician"], 31.0, 1e-9, "espera M3");
    close(
        x["M2.maintenance.wait_technician_total"],
        39.0,
        1e-9,
        "espera da preventiva",
    );
    assert!(x["M2.state.waiting_repair"] > 0.0);
    assert_eq!(x["M2.maintenance.preventives"], 0.0);
    assert_eq!(x["M2.maintenance.failures"], 0.0);
}

#[test]
fn preventive_waits_for_the_piece_in_process() {
    let m = model(
        r#"Model(
    name: "m",
    run: (stop_time: 100.0),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const(5.0), first_at_zero: true)),
        (name: "fila", kind: Queue()),
        (name: "maq", kind: Processor(process: Const(10.0)),
         downtime: (mtbf: Const(1.0e9), mttr: Const(1.0),
                    preventive: [(every: Clock(15.0), duration: Const(5.0))])),
        (name: "saida", kind: Sink),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
)"#,
    );
    let mut sim = m.build(0);
    let state = |sim: &utsim_core::Simulation| {
        sim.block_status()
            .into_iter()
            .find(|(n, ..)| n == "maq")
            .unwrap()
            .1
    };
    sim.run_until(17.0).unwrap();
    assert_eq!(
        state(&sim),
        Some(State::Processing),
        "a peça 10..20 continua"
    );
    sim.run_until(21.0).unwrap();
    assert_eq!(state(&sim), Some(State::Maintenance));
    sim.run_until(26.0).unwrap();
    assert_eq!(state(&sim), Some(State::Processing), "volta em 25");
    sim.run_until(f64::INFINITY).unwrap();
    let r = sim.finish();
    let b = r.block("maq").unwrap();
    // Devida 15 depois do fim da anterior, cada preventiva espera a peça em curso:
    // devida em 15, 40, 65, 90; feita em 20, 45, 70, 95.
    assert_eq!(b.custom["maintenance.preventives"], 4.0);
    assert_eq!(b.custom["maintenance.preventive_time"], 20.0);
    assert_eq!(b.custom["maintenance.failures"], 0.0);
}

#[test]
fn oee_by_hand() {
    // Ciclo ideal 1 (o mínimo de Const(1)), uma peça a cada 2: desempenho 1/2. Metade
    // das peças vai para o refugo (RoundRobin): qualidade 1/2. Uma preventiva de 10 a
    // cada 100 de relógio: tempo planejado 1000 - 100 = 900 sem quebras: disponibilidade 1.
    let m = model(
        r#"Model(
    name: "m",
    run: (stop_time: 1000.0),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const(2.0), first_at_zero: true)),
        (name: "fila", kind: Queue()),
        (name: "maq", kind: Processor(process: Const(1.0)), output: RoundRobin,
         downtime: (mtbf: Const(1.0e9), mttr: Const(1.0),
                    preventive: [(every: Clock(90.0), duration: Const(10.0))],
                    oee: (reject_to: ["refugo"]))),
        (name: "boas", kind: Sink),
        (name: "refugo", kind: Sink),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "boas"), ("maq", "refugo")],
)"#,
    );
    let x = metrics(&m, 0);
    close(x["maq.oee.availability"], 1.0, 1e-9, "disponibilidade");
    close(
        x["maq.maintenance.preventive_time"],
        100.0,
        1.0 + 1e-9,
        "tempo em preventiva",
    );
    let out = x["maq.output"];
    close(
        x["maq.oee.performance"],
        out / (1000.0 - x["maq.maintenance.preventive_time"]),
        1e-9,
        "desempenho",
    );
    close(
        x["maq.oee.performance"],
        500.0 / 900.0,
        0.01,
        "desempenho à mão",
    );
    close(x["maq.oee.quality"], 0.5, 0.002, "qualidade");
    close(
        x["maq.oee"],
        x["maq.oee.availability"] * x["maq.oee.performance"] * x["maq.oee.quality"],
        1e-12,
        "OEE = A × D × Q",
    );
}

#[test]
fn example_is_deterministic_and_shows_the_metrics() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/manutencao.ron");
    let m = Model::load(path).unwrap();
    m.validate().unwrap();
    let a = m.run_replications().unwrap();
    let b = m.run_replications().unwrap();
    let ma: Vec<_> = a.iter().map(|r| r.metrics()).collect();
    let mb: Vec<_> = b.iter().map(|r| r.metrics()).collect();
    assert_eq!(ma, mb);
    // Sequencial dá o mesmo que em paralelo.
    let seq: Vec<_> = (0..m.run.replications).map(|r| metrics(&m, r)).collect();
    assert_eq!(ma, seq);
    for key in [
        "torno.oee",
        "torno.maintenance.failures",
        "torno.maintenance.preventives",
        "torno.maintenance.wait_technician",
        "fresa.maintenance.cost",
        "fresa.oee.quality",
    ] {
        assert!(ma[0].contains_key(key), "falta {key}");
    }
    assert!(ma[0]["torno.maintenance.preventives"] > 0.0);
    assert!(ma[0]["fresa.oee.quality"] < 1.0);
    let lines = m.maintenance_lines();
    assert_eq!(lines.len(), 2, "{lines:?}");
}

#[test]
fn saved_state_keeps_age_and_next_preventive() {
    let m = model(&busy_line(
        1000.0,
        "(mtbf: Exp(mean: 1.0), mttr: Const(2.0), wear: Weibull(shape: 2.0, scale: 300.0), \
         basis: Busy, preventive: [(every: Hours(100.0), duration: Const(3.0)), \
         (every: Cycles(70), duration: Const(1.0))])",
    ));
    let mut sim = m.build(0);
    sim.run_until(237.5).unwrap();
    let (saved, _notes) = m.save_state(&mut sim);
    let bs = &saved.objects["maq"];
    let age = bs.age.expect("a idade é gravada");
    assert!(age > 0.0);
    assert_eq!(bs.preventive.len(), 2);
    // Gravar como texto, ler de novo e retomar: no instante 0 o estado é o mesmo.
    let text = saved.to_ron();
    let back = InitialState::parse(&text).unwrap();
    m.check_state(&back).unwrap();
    let mut again = m.build_from(0, Some(&back));
    again.begin().unwrap();
    let (resaved, _) = m.save_state(&mut again);
    let rb = &resaved.objects["maq"];
    close(rb.age.unwrap(), age, 1e-9, "idade retomada");
    for (a, b) in bs.preventive.iter().zip(&rb.preventive) {
        close(a.unwrap(), b.unwrap(), 1e-9, "próxima preventiva");
    }
    // A rodada retomada termina e a preventiva vem no prazo gravado.
    let r = again.run().unwrap();
    assert!(r.block("maq").unwrap().custom["maintenance.preventives"] >= 1.0);
}

#[test]
fn plain_downtime_reports_no_maintenance_metrics() {
    let m = model(&busy_line(
        100.0,
        "(mtbf: Exp(mean: 10.0), mttr: Const(1.0))",
    ));
    let x = metrics(&m, 0);
    assert!(
        !x.keys()
            .any(|k| k.contains("maintenance") || k.contains("oee"))
    );
    assert!(m.maintenance_lines().is_empty());
}

#[test]
fn invalid_maintenance_is_reported() {
    let bad = busy_line(
        10.0,
        "(mtbf: Exp(mean: 1.0), mttr: Const(1.0), repair_crew: \"ninguem\", \
         wear: Weibull(shape: 0.0, scale: 1.0), repair: Partial(2.0), \
         preventive: [(every: Calendar(\"x\"), duration: Const(1.0)), (every: Cycles(0), duration: Const(1.0))])",
    );
    let err = Model::parse(&bad)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    for part in [
        "repair_crew 'ninguem'",
        "Weibull",
        "Partial",
        "calendário 'x'",
        "Cycles(n)",
    ] {
        assert!(err.contains(part), "falta '{part}' em: {err}");
    }
}
