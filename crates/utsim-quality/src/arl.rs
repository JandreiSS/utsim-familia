//! Average run length (ARL): how many points a scheme takes, on average, to signal.
//! Under control it is the mean time between false alarms; after a shift of `δσ` in the
//! mean, the mean detection delay.
//!
//! - [`simulate`]: Monte Carlo for any scheme (Shewhart with any rule set, EWMA, CUSUM),
//!   on standard normal values shifted by `δ`, deterministic for a seed. This is the
//!   number the twin uses to choose `k`, `h`, `λ` and `L` with the plant's own data
//!   (`docs/plano_gemeo_digital_e_painel.md`, E4).
//! - [`shewhart`]: closed form of the 3σ chart with rule 1, `1/(1 − Φ(L − δ) + Φ(−L − δ))`
//!   (370.4 under control).
//! - [`cusum_siegmund`]: Siegmund's approximation of the two-sided tabular CUSUM.

use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};
use rand_pcg::Pcg64;
use serde::{Deserialize, Serialize};
use utsim_stats::special::norm_cdf;

use crate::rules::RuleSet;

/// A monitoring scheme on standardised values (`μ0 = 0`, `σ = 1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Scheme {
    /// Shewhart chart with limits at `±3` and the given rules.
    Shewhart(RuleSet),
    /// EWMA with `λ` and `L`; `asymptotic` limits or the exact ones.
    Ewma {
        lambda: f64,
        l: f64,
        asymptotic: bool,
    },
    /// Two-sided tabular CUSUM with `k` and `h`.
    Cusum { k: f64, h: f64 },
}

/// Monte Carlo estimate of the ARL.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArlEstimate {
    pub arl: f64,
    /// Standard error of the mean run length.
    pub std_error: f64,
    /// Standard deviation of the run length.
    pub sdrl: f64,
    pub runs: usize,
    /// Runs stopped at `max_len` without a signal (counted as `max_len`).
    pub censored: usize,
}

/// Simulates `runs` run lengths of `scheme` after a mean shift of `shift` σ (0 for the
/// in-control ARL), each stopped at `max_len`.
pub fn simulate(
    scheme: &Scheme,
    shift: f64,
    runs: usize,
    seed: u64,
    max_len: usize,
) -> ArlEstimate {
    let mut rng = Pcg64::seed_from_u64(seed);
    let mut lengths = Vec::with_capacity(runs);
    let mut censored = 0;
    let mut z: Vec<f64> = Vec::new();
    for _ in 0..runs {
        let mut len = max_len;
        let mut stopped = false;
        match scheme {
            Scheme::Shewhart(rules) => {
                z.clear();
                for t in 0..max_len {
                    let x: f64 = StandardNormal.sample(&mut rng);
                    z.push(x + shift);
                    if rules.any_fires_at(&z, t) {
                        len = t + 1;
                        stopped = true;
                        break;
                    }
                }
            }
            Scheme::Ewma {
                lambda,
                l,
                asymptotic,
            } => {
                let mut e = 0.0;
                let asym = l * (lambda / (2.0 - lambda)).sqrt();
                let mut decay = 1.0;
                for t in 0..max_len {
                    let x: f64 = StandardNormal.sample(&mut rng);
                    e = lambda * (x + shift) + (1.0 - lambda) * e;
                    decay *= (1.0 - lambda) * (1.0 - lambda);
                    let hw = if *asymptotic {
                        asym
                    } else {
                        asym * (1.0 - decay).sqrt()
                    };
                    if e.abs() > hw {
                        len = t + 1;
                        stopped = true;
                        break;
                    }
                }
            }
            Scheme::Cusum { k, h } => {
                let (mut cp, mut cm) = (0.0f64, 0.0f64);
                for t in 0..max_len {
                    let x: f64 = Distribution::<f64>::sample(&StandardNormal, &mut rng) + shift;
                    cp = (x - k + cp).max(0.0);
                    cm = (-k - x + cm).max(0.0);
                    if cp > *h || cm > *h {
                        len = t + 1;
                        stopped = true;
                        break;
                    }
                }
            }
        }
        if !stopped {
            censored += 1;
        }
        lengths.push(len as f64);
    }
    let n = lengths.len() as f64;
    let mean = lengths.iter().sum::<f64>() / n;
    let var = lengths.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    ArlEstimate {
        arl: mean,
        std_error: (var / n).sqrt(),
        sdrl: var.sqrt(),
        runs,
        censored,
    }
}

/// ARL of a Shewhart chart with limits `±l` (rule 1 only) after a shift of `shift` σ.
pub fn shewhart(l: f64, shift: f64) -> f64 {
    1.0 / (1.0 - norm_cdf(l - shift) + norm_cdf(-l - shift))
}

/// Siegmund's approximation of the ARL of a two-sided tabular CUSUM (`k`, `h` in σ),
/// after a shift of `shift` σ: each side is `(e^{−2Δb} + 2Δb − 1)/(2Δ²)` with `b = h +
/// 1.166`, `Δ = δ − k` (upper) or `−δ − k` (lower), `b²` when `Δ = 0`, and
/// `1/ARL = 1/ARL⁺ + 1/ARL⁻`.
pub fn cusum_siegmund(k: f64, h: f64, shift: f64) -> f64 {
    let b = h + 1.166;
    let side = |d: f64| {
        if d.abs() < 1e-12 {
            b * b
        } else {
            ((-2.0 * d * b).exp() + 2.0 * d * b - 1.0) / (2.0 * d * d)
        }
    };
    let up = side(shift - k);
    let down = side(-shift - k);
    1.0 / (1.0 / up + 1.0 / down)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSet;

    #[test]
    fn closed_forms() {
        // 1/0.0026998 = 370.4: the in-control ARL of a 3σ chart.
        assert!((shewhart(3.0, 0.0) - 370.398).abs() < 0.01);
        // Shift of 1σ: 1/(1 − Φ(2) + Φ(−4)) = 43.89.
        assert!((shewhart(3.0, 1.0) - 43.89).abs() < 0.01);
        // CUSUM k = 0.5: h = 5 gives ARL0 ≈ 465 and ARL(1σ) ≈ 10.4; h = 4 gives 168 and
        // 8.38 (tables of the tabular CUSUM in Montgomery, "Introduction to Statistical
        // Quality Control", chapter on CUSUM and EWMA; obtained by Siegmund's
        // approximation, which this reproduces within 2%). [conferir edição e tabela]
        assert!((cusum_siegmund(0.5, 5.0, 0.0) / 465.0 - 1.0).abs() < 0.02);
        assert!((cusum_siegmund(0.5, 5.0, 1.0) / 10.4 - 1.0).abs() < 0.02);
        assert!((cusum_siegmund(0.5, 4.0, 0.0) / 168.0 - 1.0).abs() < 0.02);
        assert!((cusum_siegmund(0.5, 4.0, 1.0) / 8.38 - 1.0).abs() < 0.02);
    }

    #[test]
    fn rule_one_in_control_arl_is_370_by_simulation() {
        let est = simulate(&Scheme::Shewhart(RuleSet::basic()), 0.0, 20_000, 7, 100_000);
        // Standard error ≈ 370/√20000 = 2.6: four standard errors.
        assert!((est.arl - 370.4).abs() < 4.0 * est.std_error, "{est:?}");
        assert_eq!(est.censored, 0);
        // The run length is geometric: SDRL ≈ √(1 − p)/p ≈ 369.9.
        assert!((est.sdrl / 369.9 - 1.0).abs() < 0.05);
    }

    #[test]
    fn western_electric_rules_raise_false_alarms() {
        // All four Western Electric rules together: in-control ARL ≈ 91.75 (Champ and
        // Woodall, Technometrics 29, 1987, by a Markov chain). [conferir]
        let est = simulate(
            &Scheme::Shewhart(RuleSet::western_electric()),
            0.0,
            6_000,
            11,
            100_000,
        );
        assert!((est.arl - 91.75).abs() < 4.0 * est.std_error, "{est:?}");
    }

    #[test]
    fn cusum_simulation_agrees_with_the_tables() {
        let s = Scheme::Cusum { k: 0.5, h: 5.0 };
        let est = simulate(&s, 0.0, 4_000, 3, 100_000);
        assert!(
            (est.arl - 465.0).abs() < 4.0 * est.std_error + 5.0,
            "{est:?}"
        );
        let est = simulate(&s, 1.0, 4_000, 4, 100_000);
        assert!((est.arl - 10.4).abs() < 0.4, "{est:?}");
    }

    #[test]
    fn ewma_simulation_agrees_with_lucas_and_saccucci() {
        // λ = 0.10, L = 2.814 and λ = 0.20, L = 2.962 (asymptotic limits) give an
        // in-control ARL of 500 (Lucas and Saccucci, Technometrics 32, 1990, tables by a
        // Markov chain). [conferir]
        for (lambda, l, seed) in [(0.10, 2.814, 5), (0.20, 2.962, 6)] {
            let s = Scheme::Ewma {
                lambda,
                l,
                asymptotic: true,
            };
            let est = simulate(&s, 0.0, 4_000, seed, 100_000);
            assert!(
                (est.arl - 500.0).abs() < 4.0 * est.std_error + 5.0,
                "λ = {lambda}: {est:?}"
            );
        }
        // The exact limits are narrower at the start: fewer points to the first alarm.
        let exact = simulate(
            &Scheme::Ewma {
                lambda: 0.1,
                l: 2.814,
                asymptotic: false,
            },
            0.0,
            4_000,
            5,
            100_000,
        );
        let asym = simulate(
            &Scheme::Ewma {
                lambda: 0.1,
                l: 2.814,
                asymptotic: true,
            },
            0.0,
            4_000,
            5,
            100_000,
        );
        assert!(exact.arl < asym.arl);
    }

    #[test]
    fn simulation_is_deterministic() {
        let s = Scheme::Cusum { k: 0.5, h: 4.0 };
        assert_eq!(
            simulate(&s, 1.0, 200, 9, 10_000),
            simulate(&s, 1.0, 200, 9, 10_000)
        );
    }
}
