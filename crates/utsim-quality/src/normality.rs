//! Normality test and the Box-Cox transformation.
//!
//! **Anderson-Darling with estimated mean and σ** (case 3 of Stephens): `A²` against the
//! normal with the sample mean and standard deviation, the modified statistic
//! `A*² = A²(1 + 0.75/n + 2.25/n²)` and the p-value by the piecewise formulas of
//! D'Agostino and Stephens (1986), *Goodness-of-Fit Techniques*, table 4.9. `utsim-fit`
//! has `A²` for fitted families with a bootstrap p-value; this is the closed form for
//! the normal, which capability needs.
//!
//! **Box-Cox**: `y = (x^λ − 1)/λ` (`ln x` at `λ = 0`) for positive data; `λ` maximises the
//! profile log-likelihood `−(n/2) ln σ̂²(λ) + (λ − 1) Σ ln xᵢ`; the interval is
//! `{λ : 2[ℓ(λ̂) − ℓ(λ)] ≤ χ²₁(level)}`; optionally `λ` is rounded to the nearest usual
//! value (−2, −1, −0.5, 0, 0.5, 1, 2) inside the interval.

use serde::{Deserialize, Serialize};
use utsim_stats::special::{brent, norm_cdf};

use crate::prob::chi2_ppf;

/// Result of the Anderson-Darling normality test.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AndersonDarling {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    /// `A²`.
    pub a2: f64,
    /// `A*² = A²(1 + 0.75/n + 2.25/n²)`.
    pub a2_star: f64,
    pub p_value: f64,
}

/// Anderson-Darling test of normality with estimated parameters (`n ≥ 8` recommended;
/// `None` for fewer than 3 values or zero spread).
pub fn anderson_darling(xs: &[f64]) -> Option<AndersonDarling> {
    let n = xs.len();
    if n < 3 {
        return None;
    }
    let nf = n as f64;
    let mean = xs.iter().sum::<f64>() / nf;
    let sd = (xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nf - 1.0)).sqrt();
    if !(sd > 0.0) {
        return None;
    }
    let mut z: Vec<f64> = xs.iter().map(|x| (x - mean) / sd).collect();
    z.sort_by(|a, b| a.total_cmp(b));
    let s: f64 = (0..n)
        .map(|i| {
            let f = norm_cdf(z[i]).clamp(1e-300, 1.0);
            // ln(1 − Φ(z)) = ln Φ(−z), more precise in the upper tail.
            let g = norm_cdf(-z[n - 1 - i]).clamp(1e-300, 1.0);
            (2 * i + 1) as f64 * (f.ln() + g.ln())
        })
        .sum();
    let a2 = -nf - s / nf;
    let a2_star = a2 * (1.0 + 0.75 / nf + 2.25 / (nf * nf));
    Some(AndersonDarling {
        n,
        mean,
        sd,
        a2,
        a2_star,
        p_value: ad_p_value(a2_star),
    })
}

/// p-value of the modified statistic `A*²` (D'Agostino and Stephens, 1986).
pub fn ad_p_value(a: f64) -> f64 {
    let p = if a >= 0.6 {
        (1.2937 - 5.709 * a + 0.0186 * a * a).exp()
    } else if a >= 0.34 {
        (0.9177 - 4.279 * a - 1.38 * a * a).exp()
    } else if a >= 0.2 {
        1.0 - (-8.318 + 42.796 * a - 59.938 * a * a).exp()
    } else {
        1.0 - (-13.436 + 101.14 * a - 223.73 * a * a).exp()
    };
    p.clamp(0.0, 1.0)
}

/// Box-Cox transform of one value.
pub fn box_cox(x: f64, lambda: f64) -> f64 {
    if lambda.abs() < 1e-12 {
        x.ln()
    } else {
        (x.powf(lambda) - 1.0) / lambda
    }
}

/// Inverse Box-Cox transform.
pub fn box_cox_inverse(y: f64, lambda: f64) -> f64 {
    if lambda.abs() < 1e-12 {
        y.exp()
    } else {
        (lambda * y + 1.0).powf(1.0 / lambda)
    }
}

/// Fitted Box-Cox transformation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxCoxFit {
    /// Maximum-likelihood `λ`.
    pub lambda_hat: f64,
    /// Interval of `λ` at the requested level (profile likelihood).
    pub ci: (f64, f64),
    /// `λ` used: `lambda_hat`, or the rounded value when asked and inside the interval.
    pub lambda: f64,
    pub rounded: bool,
    pub log_likelihood: f64,
}

/// Profile log-likelihood of `λ` (up to a constant).
pub fn box_cox_log_likelihood(xs: &[f64], lambda: f64) -> f64 {
    let n = xs.len() as f64;
    let y: Vec<f64> = xs.iter().map(|&x| box_cox(x, lambda)).collect();
    let m = y.iter().sum::<f64>() / n;
    let var = y.iter().map(|v| (v - m).powi(2)).sum::<f64>() / n;
    let sum_ln: f64 = xs.iter().map(|x| x.ln()).sum();
    -0.5 * n * var.ln() + (lambda - 1.0) * sum_ln
}

/// Fits `λ` in `[−5, 5]`. All values must be positive.
pub fn box_cox_fit(xs: &[f64], level: f64, round: bool) -> Result<BoxCoxFit, String> {
    if xs.len() < 3 {
        return Err("Box-Cox exige ao menos 3 valores".into());
    }
    if xs.iter().any(|&x| !(x > 0.0) || !x.is_finite()) {
        return Err("Box-Cox exige valores positivos; some uma constante aos dados antes".into());
    }
    let first = xs[0];
    if xs.iter().all(|&x| x == first) {
        return Err("Box-Cox com dados constantes".into());
    }
    let ll = |l: f64| box_cox_log_likelihood(xs, l);
    // Grid, then golden-section refinement around the best grid point.
    let grid: Vec<f64> = (0..=200).map(|i| -5.0 + 0.05 * i as f64).collect();
    let best = grid
        .iter()
        .copied()
        .max_by(|a, b| ll(*a).total_cmp(&ll(*b)))
        .unwrap_or(1.0);
    let lambda_hat = utsim_stats::special::golden_min(
        |l| -ll(l),
        (best - 0.05).max(-5.0),
        (best + 0.05).min(5.0),
        100,
    );
    let lmax = ll(lambda_hat);
    let cut = lmax - chi2_ppf(level, 1.0) / 2.0;
    let g = |l: f64| ll(l) - cut;
    let lo = if g(-5.0) < 0.0 {
        brent(g, -5.0, lambda_hat, 1e-10).unwrap_or(-5.0)
    } else {
        -5.0
    };
    let hi = if g(5.0) < 0.0 {
        brent(g, lambda_hat, 5.0, 1e-10).unwrap_or(5.0)
    } else {
        5.0
    };
    let mut lambda = lambda_hat;
    let mut rounded = false;
    if round {
        let usual = [-2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0];
        if let Some(&u) = usual
            .iter()
            .filter(|&&u| u >= lo && u <= hi)
            .min_by(|a, b| (*a - lambda_hat).abs().total_cmp(&(*b - lambda_hat).abs()))
        {
            lambda = u;
            rounded = true;
        }
    }
    Ok(BoxCoxFit {
        lambda_hat,
        ci: (lo, hi),
        lambda,
        rounded,
        log_likelihood: lmax,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_distr::{Distribution, StandardNormal};
    use rand_pcg::Pcg64;

    #[test]
    fn critical_values_of_stephens() {
        // Upper-tail critical values of A*² for the normal with both parameters
        // estimated: 0.631 (10%), 0.752 (5%), 0.873 (2.5%), 1.035 (1%) (Stephens, JASA
        // 69, 1974; D'Agostino and Stephens 1986, table 4.7). [conferir]
        for (a, p) in [(0.631, 0.10), (0.752, 0.05), (0.873, 0.025), (1.035, 0.01)] {
            let got = ad_p_value(a);
            assert!(
                (got - p).abs() < 0.1 * p,
                "A* = {a}: p = {got}, esperado {p}"
            );
        }
    }

    #[test]
    fn statistic_by_hand_for_a_symmetric_sample() {
        // For −1, 0, 1: mean 0, sd 1, z = −1, 0, 1; A² = −3 − (1/3)Σ(2i−1)[ln Φ(zᵢ) +
        // ln Φ(−z₍ₙ₊₁₋ᵢ₎)] with Φ(−1) = 0.158655, Φ(0) = 0.5, Φ(1) = 0.841345.
        let r = anderson_darling(&[-1.0, 0.0, 1.0]).unwrap();
        let (a, b, c) = (0.158655254f64.ln(), 0.5f64.ln(), 0.841344746f64.ln());
        let s = 1.0 * (a + a) + 3.0 * (b + b) + 5.0 * (c + c);
        let want = -3.0 - s / 3.0;
        assert!((r.a2 - want).abs() < 1e-8, "{} vs {want}", r.a2);
    }

    #[test]
    fn type_one_error_is_about_alpha() {
        let mut rng = Pcg64::seed_from_u64(42);
        let (reps, n) = (2000, 30);
        let mut rejected = 0;
        for _ in 0..reps {
            let xs: Vec<f64> = (0..n)
                .map(|_| 5.0 + 2.0 * Distribution::<f64>::sample(&StandardNormal, &mut rng))
                .collect();
            if anderson_darling(&xs).unwrap().p_value < 0.05 {
                rejected += 1;
            }
        }
        let rate = rejected as f64 / reps as f64;
        // Binomial standard error √(0.05·0.95/2000) = 0.0049.
        assert!((rate - 0.05).abs() < 0.015, "taxa {rate}");
    }

    #[test]
    fn rejects_skewed_data() {
        let mut rng = Pcg64::seed_from_u64(1);
        let xs: Vec<f64> = (0..100)
            .map(|_| (Distribution::<f64>::sample(&StandardNormal, &mut rng)).exp())
            .collect();
        assert!(anderson_darling(&xs).unwrap().p_value < 0.01);
    }

    #[test]
    fn box_cox_recovers_known_lambdas() {
        let mut rng = Pcg64::seed_from_u64(3);
        // Lognormal: λ ≈ 0.
        let ln: Vec<f64> = (0..500)
            .map(|_| (1.0 + 0.5 * Distribution::<f64>::sample(&StandardNormal, &mut rng)).exp())
            .collect();
        let fit = box_cox_fit(&ln, 0.95, true).unwrap();
        assert!(fit.ci.0 < 0.0 && 0.0 < fit.ci.1, "{fit:?}");
        assert_eq!(fit.lambda, 0.0);
        assert!(fit.rounded);
        // Square of a positive normal: √x is normal, λ ≈ 0.5.
        let sq: Vec<f64> = (0..500)
            .map(|_| (10.0 + Distribution::<f64>::sample(&StandardNormal, &mut rng)).powi(2))
            .collect();
        let fit = box_cox_fit(&sq, 0.95, false).unwrap();
        assert!(fit.ci.0 < 0.5 && 0.5 < fit.ci.1, "{fit:?}");
        assert_eq!(fit.lambda, fit.lambda_hat);
        // The transformed data pass the normality test.
        let y: Vec<f64> = ln.iter().map(|&x| box_cox(x, 0.0)).collect();
        assert!(anderson_darling(&y).unwrap().p_value > 0.01);
    }

    #[test]
    fn box_cox_round_trip_and_errors() {
        for l in [-1.0, 0.0, 0.5, 2.0] {
            let x = 3.7;
            assert!((box_cox_inverse(box_cox(x, l), l) - x).abs() < 1e-12);
        }
        assert!(box_cox_fit(&[1.0, -2.0, 3.0], 0.95, false).is_err());
        assert!(box_cox_fit(&[2.0, 2.0, 2.0], 0.95, false).is_err());
    }
}
