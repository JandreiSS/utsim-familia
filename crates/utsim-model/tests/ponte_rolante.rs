//! Ponte rolante (`Crane`): tempos dos três eixos conferidos à mão nas três ordens
//! (Safe, Sequential, Parallel), engate e desengate, peça acima da capacidade recusada,
//! validação da área de cobertura, avisos e o exemplo pronto.

use std::collections::BTreeMap;
use std::path::Path;

use utsim_model::Model;

fn metrics(m: &Model) -> BTreeMap<String, f64> {
    m.build(0).run().expect("a rodada termina").metrics()
}

fn close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-9 * want.abs().max(1.0),
        "{what}: obtido {got}, esperado {want}"
    );
}

/// Uma peça de `kg` chega em t = 0 ao recebimento (5, 9, 0); a ponte, parada no centro
/// (20, 9) com o gancho a 6 m, leva a peça até a saída em (35, 15, 1).
fn bay(motion: &str, kg: f64, extra: &str) -> String {
    format!(
        r#"Model(
    name: "baia",
    run: (stop_time: 2000.0, seed: 1),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(10000.0), first_at_zero: true,
            max_arrivals: Some(1), labels: {{"weight": Const({kg:?})}}), position: (2.0, 9.0, 0.0)),
        (name: "recebimento", kind: Queue(), transport: "ponte", position: (5.0, 9.0, 0.0)),
        (name: "saida", kind: Sink, position: (35.0, 15.0, 1.0)),
        (name: "ponte", kind: Crane(
            bridge: (speed: 0.5), trolley: (speed: 0.25), hoist: (speed: 0.1),
            hook_height: 6.0, attach: Const(30.0), detach: Const(20.0),
            capacity_t: 10.0, motion: {motion},
        ), position: (20.0, 9.0, 0.0), size: Some((40.0, 18.0, 8.0))),
        {extra}
    ],
    connections: [("chegada", "recebimento"), ("recebimento", "saida")],
)"#
    )
}

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("o modelo é lido");
    m.validate().expect("o modelo é válido");
    m
}

#[test]
fn safe_motion_by_hand() {
    // Vazia até a peça: a ponte anda 15 m a 0,5 m/s (30 s). Desce 6 m a 0,1 m/s (60 s) e
    // engata (30 s): a peça sai do recebimento em 120 s. Sobe 6 m (60 s). Até a saída: a
    // ponte anda 30 m (60 s) e o carro 6 m (24 s) juntos, 60 s. Desce 5 m (50 s), solta
    // (20 s): a peça chega em 310 s. Sobe 5 m (50 s).
    let x = metrics(&model(&bay("Safe", 5000.0, "")));
    close(x["saida.input"], 1.0, "peças entregues");
    close(x["saida.time_in_system.mean"], 310.0, "tempo da peça");
    close(x["ponte.crane.trips"], 1.0, "viagens");
    close(x["ponte.crane.refused"], 0.0, "recusas");
    // Pendurada da subida (120 s) até tocar a saída (290 s).
    close(x["ponte.crane.loaded_time"], 170.0, "com carga suspensa");
    close(x["ponte.crane.bridge_time"], 90.0, "ponte");
    close(x["ponte.crane.trolley_time"], 24.0, "carro");
    close(x["ponte.crane.hoist_time"], 220.0, "gancho");
}

#[test]
fn sequential_and_parallel_by_hand() {
    // Em sequência, a ponte e depois o carro: o trecho carregado leva 60 + 24 s.
    let x = metrics(&model(&bay("Sequential", 5000.0, "")));
    close(x["saida.time_in_system.mean"], 334.0, "sequencial");
    // Em paralelo, o gancho desce enquanto anda: 60 s até a peça (o gancho manda),
    // engata (90 s), anda 60 s subindo 1 m e solta (20 s): 170 s.
    let x = metrics(&model(&bay("Parallel", 5000.0, "")));
    close(x["saida.time_in_system.mean"], 170.0, "paralelo");
    close(
        x["ponte.crane.loaded_time"],
        60.0,
        "carga suspensa em paralelo",
    );
    // Os nomes em português também valem.
    let pt = bay("Paralela", 5000.0, "").replace("Crane(", "PonteRolante(");
    let pt = pt
        .replace("bridge:", "ponte:")
        .replace("trolley:", "carro:")
        .replace("hoist:", "talha:")
        .replace("hook_height:", "altura_gancho:")
        .replace("capacity_t:", "capacidade_t:")
        .replace("motion:", "movimento:");
    assert_eq!(metrics(&model(&pt))["saida.time_in_system.mean"], 170.0);
}

#[test]
fn a_part_above_the_capacity_is_refused_and_counted() {
    let m = model(&bay("Safe", 12000.0, ""));
    assert!(
        m.warnings()
            .iter()
            .any(|w| w.contains("ponte: há peças de até 12000 kg") && w.contains("(10 t)")),
        "{:?}",
        m.warnings()
    );
    let x = metrics(&m);
    close(x["ponte.crane.refused"], 1.0, "recusas");
    close(x["ponte.crane.trips"], 0.0, "viagens");
    close(x["saida.input"], 0.0, "nada entregue");
    // A peça fica onde está.
    close(x["recebimento.input"], 1.0, "a peça entrou no recebimento");
    assert!(model(&bay("Safe", 5000.0, "")).warnings().is_empty());
}

#[test]
fn the_geometry_and_the_reach_are_checked() {
    let err = |text: String| {
        Model::parse(&text)
            .expect("o modelo é lido")
            .validate()
            .unwrap_err()
            .to_string()
    };
    let e = err(bay("Safe", 1.0, "").replace(", size: Some((40.0, 18.0, 8.0))", ""));
    assert!(e.contains("ponte: a ponte rolante precisa de size"), "{e}");
    let e = err(bay("Safe", 1.0, "").replace("(35.0, 15.0, 1.0)", "(35.0, 25.0, 1.0)"));
    assert!(
        e.contains("ponte: fora da área da ponte") && e.contains("saida"),
        "{e}"
    );
    let e = err(bay("Safe", 1.0, "").replace("hook_height: 6.0", "hook_height: 9.0"));
    assert!(
        e.contains("hook_height deve ficar entre 0 e a altura útil (8 m)"),
        "{e}"
    );
    let e = err(bay("Safe", 1.0, "").replace("hoist: (speed: 0.1)", "hoist: (speed: 0.0)"));
    assert!(
        e.contains("ponte: hoist.speed deve ser finito e > 0"),
        "{e}"
    );
    // A ponte só transporta: não opera postos.
    let e = err(bay(
        "Safe",
        1.0,
        r#"(name: "torno", kind: Processor(process: Const(5.0), operators: Some(OperatorUse(team: "ponte"))), position: (20.0, 9.0, 0.0)),"#,
    ));
    assert!(e.contains("é uma ponte rolante, que só transporta"), "{e}");
}

#[test]
fn overlapping_cranes_are_warned_and_a_team_of_cranes_works() {
    // Uma segunda ponte cuja área cobre parte da primeira.
    let other = r#"(name: "ponte2", kind: Crane(bridge: (speed: 0.5), trolley: (speed: 0.25),
            hoist: (speed: 0.1), hook_height: 6.0, capacity_t: 5.0),
            position: (50.0, 9.0, 0.0), size: Some((30.0, 18.0, 8.0))),"#;
    let w = model(&bay("Safe", 1.0, other)).warnings();
    assert!(
        w.iter()
            .any(|x| x.contains("ponte e ponte2") && x.contains("se sobrepõem")),
        "{w:?}"
    );
    // Lado a lado, sem sobrepor: nenhum aviso.
    let apart = other.replace("(50.0, 9.0, 0.0)", "(55.0, 9.0, 0.0)");
    assert!(model(&bay("Safe", 1.0, &apart)).warnings().is_empty());
    // Uma equipe de pontes, pelo despachante.
    let team = format!(
        "{apart}\n        (name: \"pontes\", kind: Dispatcher(team: [\"ponte\", \"ponte2\"]), position: (20.0, 0.0, 0.0)),"
    );
    let text = bay("Safe", 1.0, &team).replace("transport: \"ponte\"", "transport: \"pontes\"");
    let m = model(&text);
    let x = metrics(&m);
    close(x["saida.input"], 1.0, "entregue pela equipe");
    close(
        x["ponte.crane.trips"] + x["ponte2.crane.trips"],
        1.0,
        "uma viagem",
    );
    // A ponte2 não alcança o recebimento: o modelo avisa.
    assert!(
        m.warnings()
            .iter()
            .any(|w| w.contains("ponte2: a equipe 'pontes' pode lhe dar pedidos fora da sua área")),
        "{:?}",
        m.warnings()
    );
}

#[test]
fn the_example_runs() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ponte_rolante.ron");
    let m = model(&std::fs::read_to_string(path).unwrap());
    assert!(m.warnings().is_empty(), "{:?}", m.warnings());
    let x = metrics(&m);
    assert!(x["ponte.crane.trips"] > 10.0, "{}", x["ponte.crane.trips"]);
    assert!(x["ponte.crane.loaded_time"] > 0.0);
    assert_eq!(x["ponte.crane.refused"], 0.0);
}
