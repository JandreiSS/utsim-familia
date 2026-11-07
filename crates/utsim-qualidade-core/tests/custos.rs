//! Cost of quality: the sum per category is the counts times the unit costs.

use utsim_model::Model;
use utsim_qualidade_core::custos::{Categoria, Lancamento, comparar, custos, html, rodar};

const MODELO: &str = r#"
Model(
    name: "custos",
    run: (stop_time: 1000.0, seed: 11, replications: 10),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
        (name: "torno", kind: Processor(process: Const(0.2)),
         quality: Some((defects: [(name: "risco", probability: 0.2)], unit_cost: Some(10.0), escape_cost: Some(100.0)))),
        (name: "fila", kind: Queue()),
        (name: "inspecao", kind: Processor(process: Const(0.1)), output: ByLabel("inspecao"),
         inspection: Some((detect: DETECT, on_reject: Rework, max_rework: Some(1), cost: Some(2.0)))),
        (name: "cliente", kind: Sink, sink_role: Some(Customer)),
        (name: "retrabalho", kind: Processor(process: Const(0.1)), rework: Some((fix: 0.5, cost: Some(5.0)))),
        (name: "refugo", kind: Sink, sink_role: Some(Scrap)),
    ],
    connections: [
        ("chegada", "torno"), ("torno", "fila"), ("fila", "inspecao"),
        ("inspecao", "cliente"), ("inspecao", "retrabalho"), ("inspecao", "refugo"),
        ("retrabalho", "fila"),
    ],
)
"#;

#[test]
fn category_sums_are_counts_times_unit_costs() {
    let model = Model::parse(&MODELO.replace("DETECT", "0.75")).unwrap();
    let (_, items) = model.run_item_log(0).unwrap();
    let c = custos(&model, &items);
    // Independent counts from the item log.
    let visitas = |b: usize| {
        items
            .iter()
            .map(|i| f64::from(*i.visits.get(&b).unwrap_or(&0)))
            .sum::<f64>()
    };
    let fim = |b: usize| {
        items
            .iter()
            .filter(|i| i.ended_at.is_some() && i.block == Some(b))
            .count() as f64
    };
    let escapados = items
        .iter()
        .filter(|i| i.ended_at.is_some() && i.block == Some(4))
        .filter(|i| {
            i.labels
                .get("defeitos")
                .and_then(|v| v.as_num())
                .unwrap_or(0.0)
                > 0.0
        })
        .count() as f64;
    assert_eq!(c.inspecoes, visitas(3));
    assert_eq!(c.passagens_retrabalho, visitas(5));
    assert_eq!(c.refugados, fim(6));
    let get = |k: Categoria| c.por_categoria.get(&k).copied().unwrap_or(0.0);
    assert!((get(Categoria::Avaliacao) - 2.0 * visitas(3)).abs() < 1e-9);
    // Rework passes plus the value of the lathe lost on every scrapped item.
    assert!((get(Categoria::FalhaInterna) - (5.0 * visitas(5) + 10.0 * fim(6))).abs() < 1e-9);
    // One defect type: one escaped defect per escaped item.
    assert!((get(Categoria::FalhaExterna) - 100.0 * escapados).abs() < 1e-9);
    assert_eq!(get(Categoria::Prevencao), 0.0);
    assert!((c.total() - c.por_posto.values().sum::<f64>()).abs() < 1e-9);
}

#[test]
fn entries_and_scenarios() {
    let a = Model::parse(&MODELO.replace("DETECT", "0.5")).unwrap();
    let b = Model::parse(&MODELO.replace("DETECT", "0.99")).unwrap();
    let treino = [Lancamento {
        categoria: Some(Categoria::Prevencao),
        descricao: "treinamento dos inspetores".into(),
        valor: 300.0,
        ..Lancamento::default()
    }];
    let r = rodar(&a, None, &treino).unwrap();
    let modelo: f64 = r.por_categoria.iter().map(|(_, i)| i.media).sum();
    assert!((r.total - modelo - 300.0).abs() < 1e-9);
    // Better inspection: far fewer escapes, so a much lower total cost.
    let c = comparar(&a, &b, None, &[], &treino).unwrap();
    assert!(c.diferenca.media < 0.0, "{:?}", c.diferenca);
    assert!(c.significativa);
    let h = html(&c.b, Some(&c));
    assert!(h.contains("Falha externa") && h.contains("Comparação de cenários"));
}

#[test]
fn flat_keys_for_the_missions() {
    let model = Model::parse(&MODELO.replace("DETECT", "0.75")).unwrap();
    let (_, items) = model.run_item_log(0).unwrap();
    let m = utsim_qualidade_core::simulado::metricas(&model, &items);
    let c = custos(&model, &items);
    assert_eq!(m["custo_qualidade.total"], c.total());
    assert!(m["qualidade.pct_escape"] > 0.0 && m["qualidade.pct_escape"] < 0.2);
    assert!(m.contains_key("qualidade.defeitos.risco"));
}

#[test]
fn the_report_metrics_carry_the_quality_counts() {
    let model = Model::parse(&MODELO.replace("DETECT", "0.75")).unwrap();
    let (report, items) = model.run_item_log(2).unwrap();
    let m = report.metrics();
    let k = utsim_qualidade_core::simulado::de_itens(&model, &items);
    // Warm-up is zero here, so the sink counts match the per-item log.
    assert_eq!(m["cliente.input"], k.entregues);
    assert_eq!(m["cliente.pecas_defeituosas"], k.escapados);
    assert!((m["cliente.pct_defeituosas"] - k.pct_escape).abs() < 1e-12);
    assert_eq!(m["refugo.input"], k.refugados);
    assert!(m["cliente.retrabalhadas"] > 0.0);
    assert!(m.contains_key("cliente.defeitos.risco"));
}

#[test]
fn an_8d_becomes_an_idea_in_the_seis_sigma_portfolio() {
    use utsim_lss_core::portfolio;
    use utsim_lss_store::{Clock, Identity, ProjectFolder, SystemClock};
    use utsim_qualidade_core::oito_d::{OitoD, Problema};
    let dir = tempfile::tempdir().unwrap();
    let me = Identity {
        person: SystemClock.new_id(),
        name: "Ana".into(),
        email: "ana@exemplo.org".into(),
    };
    let caminho = dir.path().join("Carteira.lss");
    ProjectFolder::create_portfolio(
        &caminho,
        "Carteira",
        "Planta 1",
        "Programa",
        &me,
        &SystemClock,
        None,
    )
    .unwrap();
    let d = OitoD {
        titulo: "Trinca na flange".into(),
        problema: Problema {
            o_que: "Trinca na flange".into(),
            quanto: "3 %".into(),
            ..Problema::default()
        },
        ..OitoD::default()
    };
    let envio =
        utsim_qualidade_core::seis_sigma::enviar(&caminho, &me, &SystemClock, &d, "torno 2")
            .unwrap();
    // Someone else opens the portfolio: the idea is there with the attachment and its hash.
    let pasta = ProjectFolder::open(&caminho, &me, None).unwrap();
    let ideias = portfolio::ideas(pasta.state(), pasta.project_id());
    let i = ideias.iter().find(|i| i.id == envio.ideia).unwrap();
    assert_eq!(i.title, "8D: Trinca na flange");
    assert!(i.data.contains(&envio.hash));
    let bytes = std::fs::read(caminho.join("anexos").join(&envio.anexo)).unwrap();
    let volta: OitoD = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(volta, d);
    // A project folder (not a portfolio) is refused.
    assert!(
        utsim_qualidade_core::seis_sigma::enviar(dir.path(), &me, &SystemClock, &d, "x").is_err()
    );
}
