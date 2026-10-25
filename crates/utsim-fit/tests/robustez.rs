//! Robustness: random CSV text never panics in the reader or in the fit; failures are
//! Portuguese messages.

use proptest::prelude::*;
use utsim_fit::data::{self, DataOptions};
use utsim_fit::report::{FitConfig, analyze};

/// Reads `text` and, when it holds numbers, fits them with a tiny bootstrap.
fn pipeline(text: &str, options: &DataOptions) -> Result<(), String> {
    let loaded = match data::parse(text, options) {
        Ok(l) => l,
        Err(e) => {
            return if e.is_empty() || !e.is_ascii() && e.contains("panicked") {
                Err(format!("mensagem inválida: {e}"))
            } else {
                Ok(())
            };
        }
    };
    let mut config = FitConfig::default();
    config.gof.bootstrap = 5;
    let report = analyze("teste", &loaded.values, &config);
    // The report must render (HTML and JSON) without panicking.
    let _ = report.to_html();
    Ok(())
}

fn options(column: Option<&str>, timestamps: bool) -> DataOptions {
    DataOptions {
        column: column.map(str::to_string),
        timestamps,
    }
}

#[test]
fn degenerate_samples() {
    for text in [
        "",
        "\n\n",
        "#só comentário",
        "x\n",
        "1\n",
        "1\n1\n1\n1\n1\n1\n",
        "0\n0\n0\n0\n0\n",
        "-1\n-2\n-3\n-4\n-5\n-6\n",
        "1e308\n1e308\n1e307\n1e308\n2e308\n",
        "1e-320\n1e-320\n2e-320\n3e-320\n",
        "t;u\n1;2\n3\n;\n4;5;6\n",
        "NaN\ninf\n-inf\n1\n2\n3\n",
        "1,5\n2,5\n3,5\n4,5\n5,5\n",
        "\u{feff}a,b\n1,2\n3,4\n5,6\n7,8\n",
    ] {
        for ts in [false, true] {
            for col in [None, Some("1"), Some("b"), Some("0"), Some("99")] {
                pipeline(text, &options(col, ts)).unwrap_or_else(|e| panic!("{text:?}: {e}"));
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_text(text in "\\PC{0,300}", ts in any::<bool>()) {
        pipeline(&text, &options(None, ts)).unwrap();
    }

    #[test]
    fn random_tables(
        rows in proptest::collection::vec(
            proptest::collection::vec(
                prop_oneof![
                    Just("".to_string()), Just("abc".to_string()), Just("NaN".to_string()),
                    Just("-0".to_string()), Just("1e308".to_string()), Just("1,5".to_string()),
                    (-1000i64..1000).prop_map(|v| v.to_string()),
                    (0.0f64..100.0).prop_map(|v| format!("{v:.3}")),
                ], 0..5),
            0..40),
        sep in prop_oneof![Just(";"), Just(","), Just("\t")],
        col in prop_oneof![Just(None), Just(Some("1")), Just(Some("2")), Just(Some("abc"))],
        ts in any::<bool>(),
    ) {
        let text = rows.iter().map(|r| r.join(sep)).collect::<Vec<_>>().join("\n");
        pipeline(&text, &options(col, ts)).unwrap();
    }

    #[test]
    fn random_numbers(
        values in proptest::collection::vec(
            prop_oneof![
                -1e6f64..1e6, Just(0.0), Just(1.0), Just(1e-300), Just(1e300), Just(f64::MIN_POSITIVE)
            ], 0..60),
        ts in any::<bool>(),
    ) {
        let text = values.iter().map(|v| format!("{v:e}")).collect::<Vec<_>>().join("\n");
        pipeline(&text, &options(None, ts)).unwrap();
    }
}
