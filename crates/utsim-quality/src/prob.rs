//! Distribution functions built on `utsim_stats::special`: F and χ² tails, χ² and beta
//! quantiles, two-sided t p-values and a composite Simpson rule.

use utsim_stats::special::{beta_inc, norm_cdf, t_cdf};

/// Upper tail of the F distribution, `P(F > f)` with `d1` and `d2` degrees of freedom.
pub fn f_sf(f: f64, d1: f64, d2: f64) -> f64 {
    if !f.is_finite() {
        return if f > 0.0 { 0.0 } else { f64::NAN };
    }
    if f <= 0.0 {
        return 1.0;
    }
    beta_inc(d2 / 2.0, d1 / 2.0, d2 / (d2 + d1 * f))
}

// The χ² CDF and quantile and the beta quantile live in `utsim_stats::special` (the
// hypothesis tests of `utsim-stats` need them too); re-exported here for the callers.
pub use utsim_stats::special::{beta_ppf, chi2_cdf, chi2_ppf};

/// Two-sided p-value of a Student-t statistic.
pub fn t_two_sided(t: f64, df: f64) -> f64 {
    if !t.is_finite() {
        return if t.is_nan() { f64::NAN } else { 0.0 };
    }
    (2.0 * (1.0 - t_cdf(t.abs(), df))).clamp(0.0, 1.0)
}

/// Two-sided p-value of a standard normal statistic.
pub fn z_two_sided(z: f64) -> f64 {
    if z.is_nan() {
        return f64::NAN;
    }
    2.0 * norm_cdf(-z.abs())
}

/// Clopper-Pearson (exact) interval (moved to [`utsim_stats::hypothesis`]).
pub use utsim_stats::hypothesis::clopper_pearson;

/// Composite Simpson rule of `f` on `[a, b]` with `n` (rounded up to even) intervals.
pub fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
    let n = n.max(2) + n % 2;
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        let w = if i % 2 == 1 { 4.0 } else { 2.0 };
        s += w * f(a + h * i as f64);
    }
    s * h / 3.0
}

/// Simpson weights for `n` (even) intervals, already multiplied by `h / 3`.
pub(crate) fn simpson_weights(n: usize, h: f64) -> Vec<f64> {
    (0..=n)
        .map(|i| {
            let w = if i == 0 || i == n {
                1.0
            } else if i % 2 == 1 {
                4.0
            } else {
                2.0
            };
            w * h / 3.0
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f_tail_matches_the_usual_table() {
        // F(0.95; 3, 10) = 3.708 (upper 5% point of the F table).
        assert!((f_sf(3.708, 3.0, 10.0) - 0.05).abs() < 2e-4);
        assert_eq!(f_sf(0.0, 2.0, 5.0), 1.0);
    }

    #[test]
    fn clopper_pearson_limits() {
        // 0 of 10: upper limit 1 - 0.025^(1/10) = 0.3085.
        let (lo, hi) = clopper_pearson(0, 10, 0.95);
        assert_eq!(lo, 0.0);
        assert!((hi - (1.0 - 0.025f64.powf(0.1))).abs() < 1e-9);
        let (lo, hi) = clopper_pearson(5, 10, 0.95);
        assert!((lo - 0.1871).abs() < 1e-4 && (hi - 0.8129).abs() < 1e-4);
    }

    #[test]
    fn simpson_is_exact_on_cubics() {
        let v = simpson(|x| x * x * x - 2.0 * x + 1.0, 0.0, 2.0, 4);
        assert!((v - 2.0).abs() < 1e-12);
    }
}
