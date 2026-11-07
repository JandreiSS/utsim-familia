//! Indicators of Serviços e atendimento on small cases with answers known by hand.

use super::*;

fn run(text: &str, meta: Meta) -> Atendimento {
    let m = Model::parse(text).unwrap();
    Atendimento::rodar(&m, None, meta).unwrap()
}

#[test]
fn erlang_c_matches_the_formula_by_hand() {
    // a = 2, c = 3: (8/6 · 3/1) / (1 + 2 + 2 + 4) = 4/9.
    assert!((erlang_c(3, 2.0) - 4.0 / 9.0).abs() < 1e-12);
    assert_eq!(erlang_c(2, 2.0), 1.0);
    // a = 2 (λ = 0.5/min, TMA 4): with no wait allowed, half served at once needs 3.
    let meta = Meta {
        wait: 0.0,
        share: 0.5,
    };
    assert_eq!(necessarios(0.5, 4.0, meta), 3);
    // More patience in the target asks for no more people.
    assert!(necessarios(0.5, 4.0, Meta::default()) <= 3);
    assert_eq!(necessarios(0.0, 4.0, meta), 0);
}

/// One desk of 10 minutes; preferencial customers at t = 0, 1, 2.
fn three(patience: &str) -> String {
    format!(
        r#"Model(name: "tres", run: (stop_time: 100.0), objects: [
    (name: "clientes", kind: Source(interarrival: Const(1.0), first_at_zero: true, max_arrivals: 3,
        labels: {{"tipo": Const(2.0)}})),
    (name: "fila", kind: Queue({patience})),
    (name: "guiche", kind: Processor(process: Const(10.0))),
    (name: "saida", kind: Sink),
], connections: [("clientes", "fila"), ("fila", "guiche"), ("guiche", "saida")])"#
    )
}

#[test]
fn waits_service_level_and_abandonment_by_hand() {
    // Patience 5: the first is served at once (wait 0), the others give up after 5.
    let k = run(&three("patience: Some(Const(5.0))"), Meta::default());
    let r = &k.reps[0];
    assert_eq!((r.atendidos, r.abandonos), (1, 2));
    assert!((r.abandono - 2.0 / 3.0).abs() < 1e-12);
    assert_eq!(r.espera, 0.0);
    assert_eq!(r.nivel, 1.0);
    assert!((r.atendimento - 10.0).abs() < 1e-9);
    assert_eq!(r.espera_abandono, Some(5.0));
    assert_eq!(r.por_tipo[&2].abandonos, 2);
    assert_eq!(r.por_fila["fila"].atendidos, 1);

    // No patience: waits 0, 9 and 18; within 15 minutes: two of three.
    let k = run(&three(""), Meta::default());
    let r = &k.reps[0];
    assert_eq!((r.atendidos, r.abandonos), (3, 0));
    assert!((r.espera - 9.0).abs() < 1e-9);
    assert!((r.nivel - 2.0 / 3.0).abs() < 1e-12);
    assert_eq!(r.espera_p90, 18.0);
    // The panel: tickets by order of arrival of their type, with the desk and the wait.
    let senhas: Vec<(&str, f64, f64)> = k
        .painel
        .iter()
        .map(|c| (c.senha.as_str(), c.instante, c.espera))
        .collect();
    assert_eq!(
        senhas,
        vec![
            ("P001", 0.0, 0.0),
            ("P002", 10.0, 9.0),
            ("P003", 20.0, 18.0)
        ]
    );
    assert!(k.painel.iter().all(|c| c.guiche == "guiche"));
    // Staffing: arrivals are in hour 0 of day 1 only.
    assert_eq!(k.horas.len(), 1);
    assert_eq!(k.horas[0].hora, 0);
    assert!((k.horas[0].chegadas - 3.0).abs() < 1e-9);
    // a = 3/60 · 10 = 0.5: one attendant serves 1 - 0.5 · e^(-0.75) = 76 % within 15
    // minutes, below 80 %; two are needed.
    assert_eq!(k.horas[0].necessarios, Some(2));
}

#[test]
fn ticket_letters() {
    assert_eq!(letra(1), "N");
    assert_eq!(letra(2), "P");
    assert_eq!(letra(3), "A");
    assert_eq!(letra(7), "T7");
}
