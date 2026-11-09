//! Equivalence by two one-sided tests (TOST, Schuirmann 1987).
//!
//! `H₀: θ ≤ low` or `θ ≥ high` against `H₁: low < θ < high`, where θ is a mean, a
//! difference of means (Welch or pooled) or a mean paired difference. Each side is a
//! one-sided t test at `alpha`; equivalence is declared when both reject, which is the
//! same as the `1 − 2·alpha` interval lying inside `(low, high)`. The estimate, its
//! standard error and degrees of freedom come from the t tests of [`crate::hypothesis`].

use serde::Serialize;

use crate::hypothesis::{self, Alternative, TTest};
use crate::special::{t_cdf, t_ppf};

/// Result of a TOST.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Tost {
    /// The underlying t test (its own p-value is for `θ = 0`).
    pub test: TTest,
    pub low: f64,
    pub high: f64,
    pub alpha: f64,
    /// `(θ̂ − low) / se`, tested against `θ > low`.
    pub t_low: f64,
    /// `(θ̂ − high) / se`, tested against `θ < high`.
    pub t_high: f64,
    pub p_low: f64,
    pub p_high: f64,
    /// The larger of the two one-sided p-values.
    pub p_value: f64,
    /// The `1 − 2·alpha` interval of θ.
    pub ci: (f64, f64),
    pub equivalent: bool,
}

/// TOST over a finished t test. `None` when the margins are not `low < high`, `alpha`
/// is outside `(0, 0.5)` or the standard error is not positive.
pub fn from_test(test: TTest, low: f64, high: f64, alpha: f64) -> Option<Tost> {
    if !(low.is_finite() && high.is_finite() && low < high) || !(alpha > 0.0 && alpha < 0.5) {
        return None;
    }
    let (est, se, df) = (test.estimate, test.se, test.df);
    if !(se > 0.0 && se.is_finite() && df > 0.0) {
        return None;
    }
    let t_low = (est - low) / se;
    let t_high = (est - high) / se;
    let p_low = 1.0 - t_cdf(t_low, df);
    let p_high = t_cdf(t_high, df);
    let p_value = p_low.max(p_high);
    let q = t_ppf(1.0 - alpha, df);
    Some(Tost {
        test,
        low,
        high,
        alpha,
        t_low,
        t_high,
        p_low,
        p_high,
        p_value,
        ci: (est - q * se, est + q * se),
        equivalent: p_value < alpha,
    })
}

/// One sample: is the mean within `(mu0 + low, mu0 + high)`? θ is `x̄ − mu0`.
pub fn one_sample(xs: &[f64], mu0: f64, low: f64, high: f64, alpha: f64) -> Option<Tost> {
    let mut t = hypothesis::t_one_sample(xs, mu0, 1.0 - 2.0 * alpha, Alternative::TwoSided)?;
    t.estimate -= mu0;
    t.hypothesized = 0.0;
    from_test(t, low, high, alpha)
}

/// Two independent samples, θ = `μ₁ − μ₂` (Welch when `pooled` is false).
pub fn two_sample(
    x: &[f64],
    y: &[f64],
    pooled: bool,
    low: f64,
    high: f64,
    alpha: f64,
) -> Option<Tost> {
    let t = hypothesis::t_two_sample(x, y, 0.0, pooled, 1.0 - 2.0 * alpha, Alternative::TwoSided)?;
    from_test(t, low, high, alpha)
}

/// Paired samples, θ = mean of `xᵢ − yᵢ`.
pub fn paired(x: &[f64], y: &[f64], low: f64, high: f64, alpha: f64) -> Option<Tost> {
    let t = hypothesis::t_paired(x, y, 0.0, 1.0 - 2.0 * alpha, Alternative::TwoSided)?;
    from_test(t, low, high, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn symmetric_margins_at_the_critical_value_give_alpha() {
        // Eleven values with mean 0 and sd 1·√11: se = 1, df = 10.
        // t_{0,95; 10} = 1,812461, so margins ±1,812461 give p = 0,05 on each side.
        let s = (11.0f64 * 10.0 / 10.0).sqrt();
        let mut xs: Vec<f64> = (0..11).map(|i| i as f64 - 5.0).collect();
        let sd = (xs.iter().map(|x| x * x).sum::<f64>() / 10.0).sqrt();
        xs.iter_mut().for_each(|x| *x *= s / sd);
        let r = one_sample(&xs, 0.0, -1.812461, 1.812461, 0.05).unwrap();
        assert!(close(r.test.se, 1.0, 1e-12), "se {}", r.test.se);
        assert!(close(r.p_low, 0.05, 1e-6), "{}", r.p_low);
        assert!(close(r.p_high, 0.05, 1e-6), "{}", r.p_high);
        assert!(close(r.ci.0, -1.812461, 1e-5) && close(r.ci.1, 1.812461, 1e-5));
    }

    #[test]
    fn equivalent_exactly_when_the_interval_is_inside_the_margins() {
        let a = [10.1, 9.8, 10.3, 10.0, 9.9, 10.2, 10.1, 9.7, 10.0, 10.2];
        let b = [10.0, 9.9, 10.1, 10.2, 9.8, 10.1, 10.0, 9.9, 10.1, 10.0];
        for margin in [0.05, 0.1, 0.2, 0.3, 0.5, 1.0] {
            let r = two_sample(&a, &b, false, -margin, margin, 0.05).unwrap();
            let inside = r.ci.0 > -margin && r.ci.1 < margin;
            assert_eq!(r.equivalent, inside, "margem {margin}");
        }
        assert!(
            two_sample(&a, &b, false, -0.5, 0.5, 0.05)
                .unwrap()
                .equivalent
        );
        assert!(
            !two_sample(&a, &b, false, -0.05, 0.05, 0.05)
                .unwrap()
                .equivalent
        );
    }

    #[test]
    fn welch_uses_the_t_test_estimate_and_df() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [2.0, 4.0, 6.0, 8.0, 10.0, 12.0];
        let r = two_sample(&a, &b, false, -10.0, 10.0, 0.05).unwrap();
        let t = hypothesis::t_two_sample(&a, &b, 0.0, false, 0.9, Alternative::TwoSided).unwrap();
        assert_eq!(r.test.df, t.df);
        assert!(close(r.ci.0, t.ci.low, 1e-9) && close(r.ci.1, t.ci.high, 1e-9));
    }

    #[test]
    fn paired_works_on_differences() {
        let x = [5.0, 6.0, 7.0, 8.0];
        let y = [4.9, 6.1, 6.9, 8.05];
        let r = paired(&x, &y, -0.5, 0.5, 0.05).unwrap();
        assert!(r.equivalent);
        assert!(close(r.test.estimate, 0.0125, 1e-12));
    }

    #[test]
    fn bad_inputs_give_none() {
        let x = [1.0, 2.0, 3.0];
        assert!(one_sample(&x, 0.0, 1.0, -1.0, 0.05).is_none());
        assert!(one_sample(&x, 0.0, -1.0, 1.0, 0.6).is_none());
        assert!(one_sample(&[1.0, 1.0, 1.0], 0.0, -1.0, 1.0, 0.05).is_none());
        assert!(one_sample(&[1.0], 0.0, -1.0, 1.0, 0.05).is_none());
    }
}
