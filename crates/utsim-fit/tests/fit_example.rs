//! End to end: example CSV -> analysis -> text/HTML. Asserts structure only, so it holds
//! whichever families and tests are implemented.

use utsim_fit::data::{DataOptions, load};
use utsim_fit::rank::RULE;
use utsim_fit::report::{FitConfig, analyze};

#[test]
fn example_csv_goes_through_the_whole_pipeline() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/dados/tempos_atendimento.csv");
    let loaded = load(&path, &DataOptions::default()).unwrap();
    assert_eq!(loaded.values.len(), 300);
    assert_eq!(loaded.column, "tempo_atendimento");
    assert!(loaded.values.iter().all(|v| *v > 0.0));

    let mut config = FitConfig::default();
    config.gof.bootstrap = 20;
    let report = analyze("tempos_atendimento.csv", &loaded.values, &config);
    assert!(!report.ranking.candidates.is_empty());

    let text = report.to_text();
    assert!(text.contains(RULE));
    let html = report.to_html();
    assert!(html.contains("<svg") && html.contains("</html>"));
    for c in &report.ranking.candidates {
        assert!(text.contains(&c.family) && html.contains(&c.family));
    }
    let by_column = load(
        &path,
        &DataOptions {
            column: Some("2".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(by_column.values, loaded.values);
}
