//! Parameter recovery and sanity checks for every family.
//!
//! Method: draw n = 2000 from a known `Dist` with a fixed seed, fit, and compare.
//! Two kinds of check, because they fail for different reasons:
//! * parameters, each with a relative tolerance of about 3-4 standard errors of the
//!   MLE at n = 2000 (the standard error of a scale/mean is ~ CV / sqrt(n) ≈ 2-3 %, of
//!   a shape parameter ~ 3-5 %, and strongly correlated parameters get more room);
//! * the whole distribution, which is what matters when parameters are poorly
//!   identified (location of shifted families, Pearson VI): the fitted CDF at the true
//!   quantiles (from a 200k-draw reference sample) must be within 0.03 of the truth.
//!   0.03 is ~ 2.5x the Kolmogorov-Smirnov distance expected between a sample of 2000
//!   and its parent.

use utsim_core::Dist;
use utsim_core::rng::stream;

use super::*;

const N: usize = 2000;
const SEED: u64 = 20_240_607;

fn draw(d: &Dist, n: usize, seed: u64) -> Vec<f64> {
    let mut rng = stream(seed, 0, 0);
    (0..n).map(|_| d.sample(&mut rng)).collect()
}

struct Case {
    family: Box<dyn Family>,
    truth: Dist,
    /// (parameter name, true value, relative tolerance)
    params: Vec<(&'static str, f64, f64)>,
}

fn case(family: impl Family + 'static, truth: Dist, params: &[(&'static str, f64, f64)]) -> Case {
    Case {
        family: Box::new(family),
        truth,
        params: params.to_vec(),
    }
}

fn cases() -> Vec<Case> {
    // Lognormal: mu and sigma of ln X for X with mean 4, sd 1.5.
    let s2 = (1.0 + (1.5_f64 / 4.0).powi(2)).ln();
    let (mu, sigma) = (4.0_f64.ln() - s2 / 2.0, s2.sqrt());
    // Shifted lognormal: heavier skew, so that the location is identifiable.
    let s2b = (1.0 + 1.0_f64).ln(); // mean 3, sd 3
    let (mub, sigmab) = (3.0_f64.ln() - s2b / 2.0, s2b.sqrt());
    vec![
        case(
            Exponential,
            Dist::Exp { mean: 3.0 },
            &[("média", 3.0, 0.08)],
        ),
        // k is an integer, so "within 1" is the natural accuracy (relative 0.34 of 3).
        case(
            Erlang,
            Dist::Erlang { k: 3, mean: 6.0 },
            &[("k", 3.0, 0.34), ("média", 6.0, 0.05)],
        ),
        case(
            Gamma,
            Dist::Gamma {
                shape: 2.5,
                scale: 1.5,
            },
            &[("forma", 2.5, 0.10), ("escala", 1.5, 0.10)],
        ),
        case(
            Weibull,
            Dist::Weibull {
                shape: 1.8,
                scale: 4.0,
            },
            &[("forma", 1.8, 0.08), ("escala", 4.0, 0.05)],
        ),
        case(
            Lognormal,
            Dist::LogNormal { mean: 4.0, sd: 1.5 },
            &[("mu", mu, 0.03), ("sigma", sigma, 0.08)],
        ),
        case(
            Normal,
            Dist::Normal {
                mean: 10.0,
                sd: 2.0,
            },
            &[("média", 10.0, 0.02), ("desvio", 2.0, 0.06)],
        ),
        // The sample covers almost the whole support (gap ~ range / n).
        case(
            Uniform,
            Dist::Uniform { min: 2.0, max: 9.0 },
            &[("mínimo", 2.0, 0.02), ("máximo", 9.0, 0.02)],
        ),
        // The density vanishes linearly at both ends, so the sample extremes sit inside
        // the true support by ~ sqrt(range * (mode - min) / n) = 0.05 (min) and 0.1 (max);
        // the mode has a standard error of a few percent of the range.
        case(
            Triangular,
            Dist::Triangular {
                min: 1.0,
                mode: 2.0,
                max: 6.0,
            },
            &[
                ("mínimo", 1.0, 0.08),
                ("moda", 2.0, 0.15),
                ("máximo", 6.0, 0.05),
            ],
        ),
        // Shapes > 1 would make the bounds uncertain; shapes just above 1 keep mass at the edges.
        case(
            Beta,
            Dist::Beta {
                alpha: 1.2,
                beta: 1.5,
                min: 1.0,
                max: 4.0,
            },
            &[
                ("alfa", 1.2, 0.10),
                ("beta", 1.5, 0.10),
                ("mínimo", 1.0, 0.03),
                ("máximo", 4.0, 0.03),
            ],
        ),
        case(
            LogLogistic,
            Dist::LogLogistic {
                shape: 4.0,
                scale: 3.0,
            },
            &[("forma", 4.0, 0.08), ("escala", 3.0, 0.05)],
        ),
        // Heavy right tail: shape and scale are strongly correlated, hence wider.
        case(
            PearsonV,
            Dist::PearsonV {
                shape: 6.0,
                scale: 10.0,
            },
            &[("forma", 6.0, 0.15), ("escala", 10.0, 0.15)],
        ),
        // Three parameters, two of them nearly collinear: only the distribution is checked.
        case(
            PearsonVI,
            Dist::PearsonVI {
                shape1: 3.0,
                shape2: 8.0,
                scale: 2.0,
            },
            &[],
        ),
        case(
            ShiftedExponential,
            Dist::Shifted {
                offset: 5.0,
                dist: Box::new(Dist::Exp { mean: 2.0 }),
            },
            &[("deslocamento", 5.0, 0.01), ("média", 2.0, 0.08)],
        ),
        // The location of a 3-parameter Weibull/gamma/lognormal is weakly identified
        // (its standard error is a sizeable fraction of the scale): loose parameter
        // tolerances, the distribution check does the real work.
        case(
            ShiftedFamily::weibull(),
            Dist::Shifted {
                offset: 10.0,
                dist: Box::new(Dist::Weibull {
                    shape: 2.5,
                    scale: 4.0,
                }),
            },
            &[("deslocamento", 10.0, 0.15)],
        ),
        case(
            ShiftedFamily::gamma(),
            Dist::Shifted {
                offset: 3.0,
                dist: Box::new(Dist::Gamma {
                    shape: 3.0,
                    scale: 2.0,
                }),
            },
            &[],
        ),
        case(
            ShiftedFamily::lognormal(),
            Dist::Shifted {
                offset: 4.0,
                dist: Box::new(Dist::LogNormal { mean: 3.0, sd: 3.0 }),
            },
            &[("mu", mub, 0.5), ("sigma", sigmab, 0.3)],
        ),
        case(
            Poisson,
            Dist::Poisson { mean: 4.0 },
            &[("média", 4.0, 0.05)],
        ),
        case(Geometric, Dist::Geometric { p: 0.3 }, &[("p", 0.3, 0.08)]),
        // n is weakly identified from counts; allow ±2 of 10, and p moves inversely.
        case(
            Binomial,
            Dist::Binomial { n: 10, p: 0.5 },
            &[("n", 10.0, 0.20), ("p", 0.5, 0.25)],
        ),
        case(
            NegBinomial,
            Dist::NegBinomial { r: 3.0, p: 0.4 },
            &[("r", 3.0, 0.30), ("p", 0.4, 0.10)],
        ),
    ]
}

fn err_of(r: Result<Fitted, String>) -> String {
    match r {
        Ok(f) => panic!("{}: deveria falhar", f.family),
        Err(e) => e,
    }
}

fn param(f: &Fitted, name: &str) -> f64 {
    f.params
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("{}: sem parâmetro {name}", f.family))
        .1
}

#[test]
fn all_lists_every_family_continuous_first() {
    let names: Vec<&str> = all().iter().map(|f| f.name()).collect();
    assert_eq!(
        names,
        [
            "Exponencial",
            "Erlang",
            "Gama",
            "Weibull",
            "Lognormal",
            "Normal",
            "Uniforme",
            "Triangular",
            "Beta",
            "Log-logística",
            "Pearson V",
            "Pearson VI",
            "Exponencial deslocada",
            "Weibull deslocada",
            "Gama deslocada",
            "Lognormal deslocada",
            "Poisson",
            "Geométrica",
            "Binomial",
            "Binomial negativa",
        ]
    );
    let flags: Vec<bool> = all().iter().map(|f| f.is_discrete()).collect();
    assert!(flags[..16].iter().all(|d| !d));
    assert!(flags[16..].iter().all(|d| *d));
}

#[test]
fn parameters_are_recovered() {
    for c in cases() {
        let data = draw(&c.truth, N, SEED);
        let f = c
            .family
            .fit(&data)
            .unwrap_or_else(|e| panic!("{}: {e}", c.family.name()));
        assert_eq!(f.family, c.family.name());
        assert_eq!(f.n_params, c.family.n_params());
        assert_eq!(f.params.len(), f.n_params, "{}", f.family);
        for &(name, truth, tol) in &c.params {
            let est = param(&f, name);
            let rel = (est - truth).abs() / truth.abs();
            assert!(
                rel <= tol,
                "{} / {name}: estimado {est}, verdadeiro {truth} (erro relativo {rel:.3} > {tol})",
                f.family
            );
        }
    }
}

#[test]
fn fitted_distribution_is_close_to_the_truth() {
    for c in cases() {
        let data = draw(&c.truth, N, SEED);
        let f = c.family.fit(&data).unwrap();
        let mut reference = draw(&c.truth, 200_000, SEED + 1);
        reference.sort_by(f64::total_cmp);
        let m = reference.len();
        for i in 1..20 {
            let p = f64::from(i) / 20.0;
            let q = reference[(p * m as f64) as usize];
            // Empirical CDF of the reference at q (right-continuous).
            let ecdf = reference.partition_point(|&x| x <= q) as f64 / m as f64;
            let diff = (f.model.cdf(q) - ecdf).abs();
            assert!(
                diff < 0.03,
                "{}: CDF ajustada em q({p}) = {q} difere {diff:.3} da verdadeira",
                f.family
            );
        }
    }
}

#[test]
fn models_are_internally_consistent() {
    for c in cases() {
        let data = draw(&c.truth, N, SEED);
        let f = c.family.fit(&data).unwrap();
        let name = f.family;
        let dist = f.dist.clone().unwrap_or_else(|| panic!("{name}: sem Dist"));
        dist.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
        let model = &f.model;
        let q = |p| model.quantile(p);
        if c.family.is_discrete() {
            // pmf sums to 1, cdf is its running sum, quantile inverts the cdf.
            let top = q(1.0 - 1e-9) as u64;
            let mut acc = 0.0;
            let mut prev = 0.0;
            for k in 0..=top {
                let x = k as f64;
                acc += model.pdf(x);
                let cdf = model.cdf(x);
                assert!(
                    (cdf - acc).abs() < 1e-8,
                    "{name}: cdf({k}) = {cdf} vs soma {acc}"
                );
                assert!(
                    cdf >= prev - 1e-15 && cdf <= 1.0 + 1e-12,
                    "{name}: cdf não monótona"
                );
                prev = cdf;
                assert_eq!(q(cdf - 1e-12), x, "{name}: quantil de cdf({k})");
            }
            assert!((acc - 1.0).abs() < 1e-6, "{name}: soma da pmf = {acc}");
            assert_eq!(model.pdf(0.5), 0.0, "{name}: pmf em não inteiro");
            assert_eq!(model.cdf(-1.0), 0.0);
        } else {
            let (lo, hi) = (q(1e-6), q(1.0 - 1e-6));
            let steps = 40_000;
            let h = (hi - lo) / steps as f64;
            let mut area = 0.0;
            let mut prev = model.cdf(lo);
            for i in 0..=steps {
                let x = lo + h * i as f64;
                let w = if i == 0 || i == steps { 0.5 } else { 1.0 };
                area += w * model.pdf(x) * h;
                let c = model.cdf(x);
                assert!(
                    (0.0..=1.0).contains(&c) && c >= prev - 1e-12,
                    "{name}: cdf não monótona em {x}"
                );
                prev = c;
            }
            // Trapezoid error is tiny at 40k points; the tails beyond q(1e-6) hold 2e-6.
            assert!(
                (area - 1.0).abs() < 2e-3,
                "{name}: integral da pdf = {area}"
            );
            for p in [0.05, 0.3, 0.5, 0.9] {
                let back = model.cdf(q(p));
                assert!(
                    (back - p).abs() < 1e-9,
                    "{name}: cdf(quantil({p})) = {back}"
                );
            }
        }
        // The sampler follows the fitted CDF: ~half of the draws fall below the median.
        let med = q(0.5);
        let mut rng = stream(SEED + 2, 0, 0);
        let below = (0..20_000)
            .filter(|_| model.sample(&mut rng) <= med)
            .count() as f64
            / 20_000.0;
        let target = if c.family.is_discrete() {
            model.cdf(med)
        } else {
            0.5
        };
        assert!(
            (below - target).abs() < 0.015,
            "{name}: {below} das amostras abaixo da mediana (esperado {target})"
        );
    }
}

#[test]
fn unsupported_data_is_rejected_with_a_reason() {
    let with_negative = [1.0, 2.0, -0.5, 3.0, 4.0];
    for fam in [
        &Erlang as &dyn Family,
        &Gamma,
        &Weibull,
        &Lognormal,
        &LogLogistic,
        &PearsonV,
        &PearsonVI,
    ] {
        let e = err_of(fam.fit(&with_negative));
        assert!(!e.is_empty(), "{}", fam.name());
        assert!(
            fam.fit(&[0.0, 1.0, 2.0, 3.0]).is_err(),
            "{}: zero",
            fam.name()
        );
    }
    // Families defined on the whole line, on a data-driven range, or with a location
    // parameter accept negatives.
    for fam in [
        &Normal as &dyn Family,
        &Uniform,
        &Triangular,
        &Beta,
        &ShiftedExponential,
        &ShiftedFamily::gamma(),
    ] {
        assert!(fam.fit(&with_negative).is_ok(), "{}", fam.name());
    }
    // Discrete families need non-negative integers.
    for fam in [&Poisson as &dyn Family, &Geometric, &Binomial, &NegBinomial] {
        assert!(fam.is_discrete());
        let e = err_of(fam.fit(&[1.5, 2.0, 3.0, 4.0]));
        assert!(e.contains("inteiros"), "{}: {e}", fam.name());
        assert!(fam.fit(&[1.0, 2.0, -3.0, 4.0]).is_err(), "{}", fam.name());
    }
    // Degenerate input, for every family.
    for fam in all() {
        assert!(fam.fit(&[]).is_err(), "{}: vazio", fam.name());
        assert!(
            fam.fit(&[1.0, f64::NAN, 2.0]).is_err(),
            "{}: NaN",
            fam.name()
        );
        // A constant sample is a legitimate (if boring) exponential: it only fails
        // where a spread is needed to estimate a parameter.
        if fam.name() != "Exponencial" {
            assert!(
                fam.fit(&[3.0, 3.0, 3.0, 3.0]).is_err(),
                "{}: constante",
                fam.name()
            );
        }
    }
}

#[test]
fn dispersion_mismatch_is_reported_for_binomial_and_negative_binomial() {
    // Underdispersed: mean 2, variance 0.5.
    let under = [2.0, 2.0, 1.0, 3.0, 2.0, 2.0, 2.0, 2.0];
    let e = err_of(NegBinomial.fit(&under));
    assert!(e.contains("superdispersão"), "{e}");
    assert!(Binomial.fit(&under).is_ok());
    // Overdispersed: variance well above the mean.
    let over = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 10.0, 12.0];
    let e = err_of(Binomial.fit(&over));
    assert!(e.contains("subdispersão"), "{e}");
    assert!(NegBinomial.fit(&over).is_ok());
}

#[test]
fn triangular_mode_is_the_best_observation() {
    // Brute force over a fine grid of modes must not beat the exact order-statistic search.
    let truth = Dist::Triangular {
        min: 0.0,
        mode: 7.0,
        max: 10.0,
    };
    let data = draw(&truth, 300, 5);
    let f = Triangular.fit(&data).unwrap();
    let (a, m, c) = (param(&f, "mínimo"), param(&f, "moda"), param(&f, "máximo"));
    let ll = |mode: f64| -> f64 {
        data.iter()
            .map(|&x| {
                let pdf = if x < mode {
                    2.0 * (x - a) / ((c - a) * (mode - a))
                } else {
                    2.0 * (c - x) / ((c - a) * (c - mode))
                };
                pdf.ln()
            })
            .sum()
    };
    let best_grid = (1..4000)
        .map(|i| ll(a + (c - a) * f64::from(i) / 4000.0))
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(ll(m) >= best_grid - 1e-9, "{} < {best_grid}", ll(m));
    assert!(data.contains(&m), "a moda deve ser uma observação");
}

#[test]
fn shifted_variants_keep_the_location_below_the_data() {
    let data = draw(
        &Dist::Shifted {
            offset: 100.0,
            dist: Box::new(Dist::Gamma {
                shape: 0.8,
                scale: 3.0,
            }),
        },
        N,
        9,
    );
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    for fam in [
        &ShiftedFamily::weibull() as &dyn Family,
        &ShiftedFamily::gamma(),
        &ShiftedFamily::lognormal(),
        &ShiftedExponential,
    ] {
        let f = fam.fit(&data).unwrap();
        let off = param(&f, "deslocamento");
        assert!(
            off < min,
            "{}: deslocamento {off} >= mínimo {min}",
            f.family
        );
        // Shape < 1 makes the likelihood unbounded at the minimum: the estimate is
        // regularised to a point just below it, never far from the truth.
        assert!(off > 90.0, "{}: deslocamento {off}", f.family);
        assert!(matches!(f.dist, Some(Dist::Shifted { .. })));
    }
}

#[test]
fn shifted_exponential_matches_the_closed_form() {
    let data = [3.0, 4.0, 5.0, 8.0];
    let f = ShiftedExponential.fit(&data).unwrap();
    // mean 5, min 3, n 4: theta = 4 * 2 / 3, gamma = 3 - theta / 4
    let theta = 8.0 / 3.0;
    assert!((param(&f, "média") - theta).abs() < 1e-12);
    assert!((param(&f, "deslocamento") - (3.0 - theta / 4.0)).abs() < 1e-12);
}

#[test]
fn every_fitted_family_exposes_a_ron_ready_dist() {
    // Positive, right-skewed data that most families accept.
    let data = draw(
        &Dist::Gamma {
            shape: 4.0,
            scale: 2.0,
        },
        500,
        11,
    );
    let counts: Vec<f64> = draw(&Dist::NegBinomial { r: 3.0, p: 0.3 }, 500, 12);
    let mut fitted = 0;
    for fam in all() {
        let sample = if fam.is_discrete() { &counts } else { &data };
        if let Ok(f) = fam.fit(sample) {
            let d = f.dist.expect("Fitted.dist deve ser Some");
            d.validate().unwrap();
            let text = ron::to_string(&d).unwrap();
            assert_eq!(ron::from_str::<Dist>(&text).unwrap(), d, "{}", f.family);
            fitted += 1;
        }
    }
    assert!(fitted >= 15, "só {fitted} famílias ajustaram");
}
