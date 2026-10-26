//! Tutorial 05 do manual: os dois arquivos de dados dão as distribuições citadas no texto.

use std::path::Path;

use utsim_fit::data::{DataOptions, load};
use utsim_fit::report::{FitConfig, analyze};

fn best_family(file: &str, options: &DataOptions) -> (String, Vec<(String, f64)>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/dados")
        .join(file);
    let loaded = load(&path, options).unwrap();
    let mut config = FitConfig::default();
    config.gof.bootstrap = 100;
    let report = analyze(file, &loaded.values, &config);
    let best = &report.ranking.candidates[report.ranking.best.expect("há uma escolhida")];
    (best.family.clone(), best.params.clone())
}

#[test]
fn service_times_are_gamma() {
    let (family, params) = best_family("tutorial_05_atendimento.csv", &DataOptions::default());
    assert_eq!(family, "Gama");
    let value = |name: &str| params.iter().find(|(n, _)| n.contains(name)).unwrap().1;
    let (shape, scale) = (value("forma"), value("escala"));
    // O texto cita forma 3,506 e escala 1,459 (média 5,115).
    assert!((shape * scale - 5.115).abs() < 0.01, "{shape} x {scale}");
    assert!((2.5..4.5).contains(&shape), "{shape}");
}

#[test]
fn arrival_intervals_are_exponential_by_parsimony() {
    let options = DataOptions {
        column: Some("instante_min".into()),
        timestamps: true,
    };
    let (family, params) = best_family("tutorial_05_chegadas.csv", &options);
    assert_eq!(family, "Exponencial");
    let mean = params[0].1;
    assert!((mean - 2.896).abs() < 0.01, "{mean}");
}
