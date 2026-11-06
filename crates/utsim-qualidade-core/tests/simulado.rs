//! Quality indicators of the simulation against the hand calculation and against
//! `utsim-quality` on the same values.

use std::path::Path;

use utsim_model::Model;
use utsim_qualidade_core::relatorio;
use utsim_qualidade_core::simulado::{coluna, rodar};
use utsim_quality::Spec;
use utsim_quality::capability;

const INSPECTION: &str = r#"
Model(
    name: "inspecao",
    run: (stop_time: 2000.0, seed: 11, replications: 30),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
        (name: "torno", kind: Processor(process: Const(0.2)),
         quality: Some((defects: [(name: "risco", probability: 0.2)]))),
        (name: "fila", kind: Queue()),
        (name: "inspecao", kind: Processor(process: Const(0.1)), output: ByLabel("inspecao"),
         inspection: Some((detect: 0.75, on_reject: Rework, max_rework: Some(1)))),
        (name: "cliente", kind: Sink, sink_role: Some(Cliente)),
        (name: "retrabalho", kind: Processor(process: Const(0.1)), rework: Some((fix: 0.5))),
        (name: "refugo", kind: Sink, sink_role: Some(Refugo)),
    ],
    connections: [
        ("chegada", "torno"), ("torno", "fila"), ("fila", "inspecao"),
        ("inspecao", "cliente"), ("inspecao", "retrabalho"), ("inspecao", "refugo"),
        ("retrabalho", "fila"),
    ],
)
"#;

fn media(r: &utsim_qualidade_core::RelatorioQualidade, nome: &str) -> f64 {
    r.resumo.iter().find(|(n, _)| n == nome).unwrap().1.media
}

#[test]
fn indicators_match_the_hand_calculation() {
    let r = rodar(&Model::parse(INSPECTION).unwrap(), None).unwrap();
    // See utsim-model/tests/qualidade.rs: escape 0.06875, scrap 0.05625, FPY 0.85.
    assert!((media(&r, "% refugo") - 0.05625).abs() < 0.005);
    assert!((media(&r, "Rendimento de primeira passagem") - 0.85).abs() < 0.01);
    let escape = 0.06875 / (1.0 - 0.05625);
    assert!((media(&r, "% escape (defeitos no cliente)") - escape).abs() < 0.01);
    // One opportunity: DPMO = DPU * 1e6, DPU = 0.2.
    assert!((media(&r, "Defeitos por unidade") - 0.2).abs() < 0.01);
    assert!((media(&r, "DPMO") - 200_000.0).abs() < 10_000.0);
    // Defective items caught: 1 - first-pass escapes / defective = 1 - 0.05 / 0.2.
    assert!((media(&r, "Eficácia da inspeção") - 0.75).abs() < 0.02);
    assert_eq!(r.pareto_tipo[0].0, "risco");
    assert_eq!(r.pareto_posto[0].0, "torno");
}

#[test]
fn capability_is_the_one_of_utsim_quality_on_the_csv() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/inspecao_e_retrabalho.ron");
    let mut model = Model::load(&path).unwrap();
    model.run.replications = 2;
    let r = rodar(&model, None).unwrap();
    let c = &r.capacidades[0];
    let k = c.capacidade.as_ref().unwrap();
    // Same values from the CSV of the long output.
    let csv = relatorio::csv(&r.medicoes);
    let valores: Vec<Vec<f64>> = csv
        .lines()
        .skip(1)
        .filter(|l| l.split(',').nth(3) == Some("diametro"))
        .map(|l| vec![l.split(',').nth(5).unwrap().parse::<f64>().unwrap()])
        .collect();
    let spec = Spec {
        lsl: Some(24.95),
        usl: Some(25.05),
        target: None,
    };
    let direto = capability::capability(&valores, &spec, &capability::Options::default()).unwrap();
    assert_eq!(direto.cpk.value, k.cpk.value);
    assert_eq!(direto.ppk.value, k.ppk.value);
    assert!(!coluna(&r.medicoes, "diametro", 0).is_empty());
    let html = relatorio::html(&r);
    assert!(html.contains("Pareto por tipo de defeito"));
    assert!(html.contains("Simulado"));
    assert!(
        !html.contains("http"),
        "o relatório não pode depender da rede"
    );
}
