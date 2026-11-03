//! Indicators on small cases with answers known by hand.

use super::*;
use crate::registro::{LEAN_OFFICE, find};

fn run(text: &str) -> Indicadores {
    let m = Model::parse(text).unwrap();
    Indicadores::rodar(&m, find(LEAN_OFFICE).as_ref()).unwrap()
}

/// A deterministic step worked by a person standing at it: every case takes 4 min.
const ONE: &str = r#"Model(name: "um", line: "lean_office", run: (stop_time: 10000.0, warmup: 1000.0, replications: 2), objects: [
    (name: "pedidos", kind: Source(interarrival: Const(10.0), labels: {"sla": Const(SLA)}), preset: "lean_office/entrada"),
    (name: "posto", kind: Processor(process: Const(4.0), operators: Some(OperatorUse(team: "ana"))),
        position: (1.0, 0.0, 0.0), preset: "lean_office/posto"),
    (name: "fim", kind: Sink, preset: "lean_office/conclusao"),
    (name: "ana", kind: Transporter(speed: 72.0), position: (1.0, 0.0, 0.0), preset: "lean_office/pessoa"),
], connections: [("pedidos", "posto"), ("posto", "fim")])"#;

#[test]
fn one_deterministic_step_lead_time_equals_process_time() {
    let k = run(&ONE.replace("SLA", "5.0"));
    let r = &k.reps[0];
    assert!(r.concluidos >= 890, "{}", r.concluidos);
    assert!((r.lead_time - 4.0).abs() < 1e-9);
    assert!((r.processo - 4.0).abs() < 1e-9);
    assert!((r.touch - 4.0).abs() < 1e-9);
    assert!((r.razao_atividade - 1.0).abs() < 1e-9);
    assert_eq!(r.fpy, 1.0);
    assert_eq!(r.voltas, 0.0);
    assert_eq!(r.sla, Some(1.0));
    // Little: WIP = arrival rate x lead time.
    assert!((r.wip - 0.4).abs() < 0.01, "{}", r.wip);
    assert!((r.por_dia - 144.0).abs() < 1.0, "{}", r.por_dia);
    assert!(r.espera.iter().all(|e| e.abs() < 1e-9));
    let ana = &r.ocupacao["ana"];
    assert!((ana.trabalhando - 0.4).abs() < 0.01, "{ana:?}");
    // Every hour of the day has the same occupancy.
    let hours = &k.ocupacao_hora["ana"];
    assert!(
        hours
            .iter()
            .all(|h| h.is_some_and(|v| (v - 0.4).abs() < 0.05))
    );
    assert_eq!(run(&ONE.replace("SLA", "3.0")).reps[0].sla, Some(0.0));
    assert!(k.serie_wip.iter().all(|(_, n)| *n <= 1.0));
}

#[test]
fn handover_is_waiting_not_process_time() {
    let k = run(
        r#"Model(name: "dois", run: (stop_time: 5000.0, replications: 2), objects: [
            (name: "pedidos", kind: Source(interarrival: Const(10.0))),
            (name: "a", kind: Processor(process: Const(2.0))),
            (name: "malote", kind: Processor(capacity: 1000, process: Const(3.0)), preset: "lean_office/tramitacao"),
            (name: "b", kind: Processor(process: Const(4.0))),
            (name: "fim", kind: Sink),
        ], connections: [("pedidos", "a"), ("a", "malote"), ("malote", "b"), ("b", "fim")])"#,
    );
    let r = &k.reps[0];
    assert!((r.lead_time - 9.0).abs() < 1e-9, "{}", r.lead_time);
    assert!((r.processo - 6.0).abs() < 1e-9, "{}", r.processo);
    // No people: no touch time.
    assert_eq!(r.touch, 0.0);
    assert!((r.espera[3] - 3.0).abs() < 1e-9, "{:?}", r.espera);
    let total: f64 = r.espera.iter().sum();
    assert!((r.processo + total - r.lead_time).abs() < 1e-9);
    assert_eq!(r.pt_etapa["a"], (2.0, 0.0));
    assert!(!r.pt_etapa.contains_key("malote"));
}

#[test]
fn rolled_ca_of_two_steps_at_90_percent_is_81_percent() {
    let posto = |name: &str| {
        format!(
            r#"(name: "{name}", kind: Processor(process: Exp(mean: 1.0)), preset: "lean_office/posto",
                on_entry: "if random() > 0.9 {{ item.erro = 1.0; item.erros_{name} = label(item, \"erros_{name}\", 0.0) + 1.0; }}")"#
        )
    };
    let text = format!(
        r#"Model(name: "ca", run: (stop_time: 100000.0, replications: 5, seed: 4), objects: [
            (name: "pedidos", kind: Source(interarrival: Exp(mean: 5.0))),
            {}, {},
            (name: "fim", kind: Sink),
        ], connections: [("pedidos", "a"), ("a", "b"), ("b", "fim")])"#,
        posto("a"),
        posto("b")
    );
    let k = run(&text);
    let rolled = k.resumo_opt(|r| r.rolled_ca).unwrap();
    assert!(
        (rolled.mean - 0.81).abs() <= rolled.half_width.max(0.01),
        "{rolled:?}"
    );
    for step in ["a", "b"] {
        let s = k.resumo(|r| r.ca[step]);
        assert!((s.mean - 0.9).abs() < 0.01, "{step}: {s:?}");
    }
    // Without a check the errors stay in the cases: FPY is the rolled %C&A.
    let fpy = k.resumo(|r| r.fpy);
    assert!((fpy.mean - rolled.mean).abs() < 0.01, "{fpy:?}");
    assert_eq!(k.etapas_ca(), ["a", "b"]);
}

#[test]
fn the_ready_model_has_every_indicator() {
    let m = Model::load(format!(
        "{}/../../examples/aprovacao_de_compras.ron",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let mut m = m;
    m.run.replications = 2;
    let k = Indicadores::rodar(&m, find(LEAN_OFFICE).as_ref()).unwrap();
    let r = &k.reps[0];
    assert!(r.concluidos > 100);
    assert!(r.lead_time > r.processo && r.processo >= r.touch && r.touch > 0.0);
    assert!(r.fpy > 0.5 && r.fpy < 1.0, "{}", r.fpy);
    assert!(r.sla.is_some());
    assert!(r.voltas > 0.0);
    assert!(r.ca.contains_key("cotacao"));
    assert!(r.espera[4] > 0.0, "comitê: {:?}", r.espera);
    assert!(r.espera[2] > 0.0, "solicitante: {:?}", r.espera);
    assert_eq!(r.ocupacao.len(), 6);
    // Conservation: lead time = process time + every wait.
    let total: f64 = r.espera.iter().sum();
    assert!((r.processo + total - r.lead_time).abs() < 1e-6 * r.lead_time);
    assert!(k.tabela().len() >= 11);
}

#[test]
fn report_speaks_the_line_vocabulary() {
    let pack = find(LEAN_OFFICE).unwrap();
    let k = run(&ONE.replace("SLA", "5.0"));
    let page = relatorio::html(&k, &pack.vocabulary, "");
    for want in [
        "Lead time",
        "Touch time",
        "FPY",
        "SLA",
        "WIP de casos",
        "<svg",
    ] {
        assert!(page.contains(want), "{want}");
    }
    // No word of the kernel is left in the page.
    for core in [
        "time_in_system",
        "waiting_operator",
        "off_shift",
        "utilize",
        "Processor",
        "Queue",
        "Source",
        " item ",
        " itens ",
    ] {
        assert!(!page.contains(core), "{core}");
    }
    let text = relatorio::texto(&k);
    assert!(text.contains("Lead time (L/T)"), "{text}");
}

#[test]
fn office_map_and_lanes_of_the_ready_model() {
    let pack = find(LEAN_OFFICE).unwrap();
    let mut m = Model::load(format!(
        "{}/../../examples/aprovacao_de_compras.ron",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    m.run.replications = 2;
    let k = Indicadores::rodar(&m, Some(&pack)).unwrap();
    let v = diagramas::montar(&m, Some(&pack), &k);
    let names: Vec<&str> = v
        .caixas
        .iter()
        .map(|c| match c {
            diagramas::Caixa::Processo { nome, .. } | diagramas::Caixa::Espera { nome, .. } => {
                nome.as_str()
            }
        })
        .collect();
    assert_eq!(
        names,
        [
            "caixa_compras",
            "cotacao",
            "conferencia",
            "alcadas",
            "caixa_gerente",
            "aprov_gerente",
            "caixa_emissao",
            "emissao"
        ]
    );
    let svg = v.svg("Estado atual");
    assert!(svg.starts_with("<svg") && svg.contains("Escada do lead time"));
    let (lanes, handovers) = diagramas::raias(&m);
    for area in ["Solicitante", "Compras", "Gerência", "Diretoria", "Comitê"] {
        assert!(lanes.contains(area), "{area}");
    }
    assert!(
        handovers
            .iter()
            .any(|(a, b, _)| a == "Compras" && b == "Gerência")
    );
    let page = diagramas::secoes(&m, Some(&pack), &k, Some((&m, &k)));
    assert!(page.contains("Estado futuro"));
}
