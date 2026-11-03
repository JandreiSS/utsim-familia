//! Escolha automática (`auto`): amostras grandes de famílias conhecidas são reconhecidas,
//! e o resultado é determinístico.

use utsim_core::Dist;
use utsim_core::rng::stream;
use utsim_fit::auto::{AutoConfig, AutoFit, Choice, Criterion, auto_fit};
use utsim_fit::gof::GofConfig;
use utsim_fit::report::FitConfig;

const N: usize = 1000;

fn config(criterion: Criterion) -> AutoConfig {
    AutoConfig {
        choose: Choice::Best,
        criterion,
        fit: FitConfig {
            alpha: 0.05,
            gof: GofConfig {
                bootstrap: 99,
                seed: 1,
                chi2_bins: None,
            },
        },
    }
}

fn sample(dist: &Dist, seed: u64) -> Vec<f64> {
    let mut rng = stream(seed, 0, 0);
    (0..N).map(|_| dist.sample(&mut rng)).collect()
}

/// The chosen family is one of `names`, or one of them is statistically equivalent.
fn assert_recognized(fit: &AutoFit, names: &[&str]) {
    let equivalent = &fit.report.ranking.equivalent;
    assert!(
        names.contains(&fit.family.as_str())
            || equivalent.iter().any(|e| names.contains(&e.as_str())),
        "{}: escolhida {} (equivalentes {equivalent:?}), esperada uma de {names:?}",
        fit.name,
        fit.family
    );
    assert!(!fit.empirical_fallback, "{}", fit.name);
}

#[test]
fn known_families_are_recognized() {
    let cases: Vec<(Dist, &[&str])> = vec![
        (Dist::Exp { mean: 3.0 }, &["Exponencial", "Erlang", "Gama"]),
        (
            Dist::Gamma {
                shape: 4.0,
                scale: 0.5,
            },
            &["Gama", "Erlang"],
        ),
        (
            Dist::Weibull {
                shape: 2.5,
                scale: 4.0,
            },
            &["Weibull"],
        ),
        (Dist::LogNormal { mean: 5.0, sd: 2.0 }, &["Lognormal"]),
        (
            Dist::Normal {
                mean: 50.0,
                sd: 4.0,
            },
            &["Normal"],
        ),
        (Dist::Uniform { min: 2.0, max: 6.0 }, &["Uniforme"]),
        // A binomial with large n and small p is a Poisson in practice: both are right.
        (
            Dist::Poisson { mean: 4.0 },
            &["Poisson", "Binomial", "Binomial negativa"],
        ),
    ];
    for (i, (dist, names)) in cases.into_iter().enumerate() {
        let data = sample(&dist, 500 + i as u64);
        let fit = auto_fit(&format!("{dist:?}"), &data, &config(Criterion::Aic)).unwrap();
        assert_recognized(&fit, names);
    }
}

#[test]
fn other_criteria_also_recognize_a_clear_case() {
    let data = sample(
        &Dist::Weibull {
            shape: 3.0,
            scale: 10.0,
        },
        77,
    );
    for crit in [Criterion::Bic, Criterion::Ks, Criterion::Ad] {
        let fit = auto_fit("Weibull", &data, &config(crit)).unwrap();
        let top: Vec<&str> = fit
            .order
            .iter()
            .take(3)
            .map(|&i| fit.report.ranking.candidates[i].family.as_str())
            .collect();
        assert!(
            top.iter().any(|f| f.starts_with("Weibull")),
            "{crit:?}: {top:?}"
        );
    }
}

#[test]
fn same_data_same_choice() {
    let data = sample(&Dist::LogNormal { mean: 3.0, sd: 1.0 }, 9);
    let a = auto_fit("x", &data, &config(Criterion::Aic)).unwrap();
    let b = auto_fit("x", &data, &config(Criterion::Aic)).unwrap();
    assert_eq!(a.dist, b.dist);
    assert_eq!(a.order, b.order);
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.to_csv(), b.to_csv());
    assert_eq!(a.to_html("t"), b.to_html("t"));
}
