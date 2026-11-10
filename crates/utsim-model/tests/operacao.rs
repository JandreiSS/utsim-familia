//! Regra de operação de um posto (`operation`): tempo para o operador entrar e sair (estados
//! `entering` e `leaving` do posto, conferidos à mão), habilidades e EPI exigidos (do posto e
//! das zonas de risco em que ele está), avisos e regra vazia sem efeito.

use std::collections::BTreeMap;

use utsim_model::Model;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("o modelo é lido");
    m.validate().expect("o modelo é válido");
    m
}

fn metrics(m: &Model, r: u32) -> BTreeMap<String, f64> {
    m.build(r).run().expect("a rodada termina").metrics()
}

fn close(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() <= 1e-9, "{what}: obtido {got}, esperado {want}");
}

/// Um operador no próprio posto (sem caminhada); 5 peças, uma a cada 100 s; processo de 10 s.
fn station(rule: &str) -> String {
    format!(
        r#"Model(
    name: "m",
    run: (stop_time: 1000.0, seed: 3),
    objects: [
        (name: "fonte", kind: Source(interarrival: Const(100.0), first_at_zero: true, max_arrivals: Some(5))),
        (name: "fila", kind: Queue()),
        (name: "maq", kind: Processor(process: Const(10.0),
            operators: Some(OperatorUse(team: "op"))){rule}),
        (name: "saida", kind: Sink),
        (name: "op", kind: Transporter(speed: 1.0)),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
)"#
    )
}

#[test]
fn empty_rule_changes_nothing() {
    let base = model(&station(""));
    let empty = model(&station(", operation: Some((require_ppe: true))"));
    for r in 0..2 {
        assert_eq!(metrics(&base, r), metrics(&empty, r), "replicação {r}");
    }
}

#[test]
fn entering_and_leaving_by_hand() {
    // Com os nomes em português: 2 s para entrar e 3 s para sair em cada peça.
    let m = model(&station(
        ", operacao: Some((entrar: Const(2.0), sair: Const(3.0)))",
    ));
    let x = metrics(&m, 0);
    close(x["maq.state.entering"], 5.0 * 2.0 / 1000.0, "entrando");
    close(x["maq.state.leaving"], 5.0 * 3.0 / 1000.0, "saindo");
    close(x["maq.state.processing"], 5.0 * 10.0 / 1000.0, "processando");
    // O operador fica no posto do começo da entrada ao fim da saída.
    close(x["op.state.utilize"], 5.0 * 15.0 / 1000.0, "operador");
    assert_eq!(x["saida.input"], 5.0);
}

#[test]
fn entering_with_a_setup_and_the_same_crew_happens_once() {
    let m = model(
        &station(", operation: Some((enter: Const(2.0), leave: Const(3.0)))")
            .replace("Processor(process", "Processor(setup: Some(Const(4.0)), process"),
    );
    let x = metrics(&m, 0);
    close(x["maq.state.entering"], 5.0 * 2.0 / 1000.0, "entrando");
    close(x["maq.state.setup"], 5.0 * 4.0 / 1000.0, "preparação");
    close(x["maq.state.leaving"], 5.0 * 3.0 / 1000.0, "saindo");
    close(x["op.state.utilize"], 5.0 * 19.0 / 1000.0, "operador");
}

/// Equipe "eq": `a` (primeira, mais perto) e `b`. O posto pede capacete; a zona em volta dele
/// pede óculos.
fn ppe_model(a_ppe: &str, b_ppe: &str, require: bool) -> String {
    format!(
        r#"Model(
    name: "m",
    run: (stop_time: 2000.0, seed: 3),
    objects: [
        (name: "fonte", kind: Source(interarrival: Exp(mean: 30.0)), position: (0.0, 0.0, 0.0)),
        (name: "fila", kind: Queue(), position: (2.0, 0.0, 0.0)),
        (name: "maq", kind: Processor(process: Const(10.0),
            operators: Some(OperatorUse(team: "eq"))), position: (5.0, 0.0, 0.0),
            ppe: ["capacete"], operation: Some((require_ppe: {require}))),
        (name: "saida", kind: Sink, position: (8.0, 0.0, 0.0)),
        (name: "a", kind: Transporter(speed: 1.0), position: (5.0, 2.0, 0.0), ppe: {a_ppe}),
        (name: "b", kind: Transporter(speed: 1.0), position: (5.0, -4.0, 0.0), ppe: {b_ppe}),
        (name: "eq", kind: Dispatcher(team: ["a", "b"], rule: FirstAvailable), position: (5.0, 0.0, 0.0)),
    ],
    connections: [("fonte", "fila"), ("fila", "maq"), ("maq", "saida")],
    zones: [(name: "z", polygon: [(4.0, -1.0), (6.0, -1.0), (6.0, 1.0), (4.0, 1.0)], ppe: ["oculos"])],
)"#
    )
}

#[test]
fn ppe_of_the_station_and_its_zone_restricts_who_operates() {
    let m = model(&ppe_model(
        r#"["capacete"]"#,
        r#"["capacete", "oculos"]"#,
        true,
    ));
    assert_eq!(
        m.required_ppe(m.objects.iter().find(|o| o.name == "maq").unwrap()),
        vec!["capacete".to_string(), "oculos".to_string()]
    );
    assert!(m.warnings().iter().all(|w| !w.contains("regra de operação")));
    let x = metrics(&m, 0);
    assert_eq!(x.get("a.state.utilize"), None, "a está sem óculos");
    assert!(x["b.state.utilize"] > 0.0);
    assert!(x["saida.input"] > 10.0);
    // Sem exigir o EPI, a primeira livre (a, mais perto) também opera.
    let open = model(&ppe_model(
        r#"["capacete"]"#,
        r#"["capacete", "oculos"]"#,
        false,
    ));
    assert!(metrics(&open, 0)["a.state.utilize"] > 0.0);
    // Ninguém com todo o EPI: aviso.
    let none = model(&ppe_model("[]", r#"["oculos"]"#, true));
    assert!(
        none.warnings().iter().any(|w| w.contains(
            "ninguém da equipe 'eq' atende à regra de operação (EPI 'capacete', EPI 'oculos')"
        )),
        "{:?}",
        none.warnings()
    );
}

#[test]
fn skills_of_the_rule_restrict_who_operates() {
    let text = ppe_model("[]", "[]", false)
        .replace("(require_ppe: false)", r#"(habilidades: ["torno"])"#)
        .replace(r#"ppe: [], "#, "")
        .replace(
            r#"position: (5.0, 2.0, 0.0), ppe: []"#,
            r#"position: (5.0, 2.0, 0.0), person: Some((skills: ["solda"]))"#,
        )
        .replace(
            r#"position: (5.0, -4.0, 0.0), ppe: []"#,
            r#"position: (5.0, -4.0, 0.0), person: Some((skills: ["torno"]))"#,
        );
    let m = model(&text);
    let x = metrics(&m, 0);
    assert_eq!(x.get("a.state.utilize"), None, "a não sabe operar o torno");
    assert!(x["b.state.utilize"] > 0.0);
}

#[test]
fn the_rule_is_only_for_processors() {
    let text = station("").replace(
        r#"(name: "fila", kind: Queue())"#,
        r#"(name: "fila", kind: Queue(), operation: Some((enter: Const(1.0))))"#,
    );
    let m = Model::parse(&text).unwrap();
    let err = format!("{:?}", m.validate().unwrap_err());
    assert!(err.contains("fila: a regra de operação"), "{err}");
    let bad = station(", operation: Some((enter: Exp(mean: 0.0)))");
    let err = format!("{:?}", Model::parse(&bad).unwrap().validate().unwrap_err());
    assert!(err.contains("operation.enter"), "{err}");
}
