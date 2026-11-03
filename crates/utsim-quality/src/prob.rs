//! Distribution functions built on `utsim_stats::special`: F and χ² tails, χ² and beta
//! quantiles, two-sided t p-values and a composite Simpson rule.

use utsim_stats::special::{beta_inc, brent, gamma_p, norm_cdf, t_cdf};

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

/// χ² CDF with `k` degrees of freedom.
pub fn chi2_cdf(x: f64, k: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        gamma_p(k / 2.0, x / 2.0)
    }
}

/// χ² quantile with `k > 0` degrees of freedom, `0 < p < 1` (NaN outside).
pub fn chi2_ppf(p: f64, k: f64) -> f64 {
    if !(p > 0.0 && p < 1.0) || k <= 0.0 {
        return f64::NAN;
    }
    let mut hi = (k + 10.0 * (2.0 * k).sqrt()).max(10.0);
    while chi2_cdf(hi, k) < p {
        hi *= 2.0;
    }
    brent(|x| chi2_cdf(x, k) - p, 0.0, hi, 1e-12).unwrap_or(f64::NAN)
}

/// Quantile of the beta distribution `Beta(a, b)`.
pub fn beta_ppf(p: f64, a: f64, b: f64) -> f64 {
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    brent(|x| beta_inc(a, b, x) - p, 0.0, 1.0, 1e-13).unwrap_or(f64::NAN)
}

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

/// Clopper-Pearson (exact) interval for `x` successes in `n` trials at `level`.
pub fn clopper_pearson(x: usize, n: usize, level: f64) -> (f64, f64) {
    if n == 0 {
        return (f64::NAN, f64::NAN);
    }
    let a = (1.0 - level) / 2.0;
    let (xf, nf) = (x as f64, n as f64);
    let lo = if x == 0 {
        0.0
    } else {
        beta_ppf(a, xf, nf - xf + 1.0)
    };
    let hi = if x == n {
        1.0
    } else {
        beta_ppf(1.0 - a, xf + 1.0, nf - xf)
    };
    (lo, hi)
}

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
    fn chi2_quantiles_match_the_usual_table() {
        // χ² quantiles printed in every statistics table (e.g. NIST/SEMATECH e-Handbook,
        // section 1.3.6.7.4): χ²(0.95; 1) = 3.841, χ²(0.975; 10) = 20.483, χ²(0.025; 10) = 3.247.
        assert!((chi2_ppf(0.95, 1.0) - 3.841).abs() < 5e-4);
        assert!((chi2_ppf(0.975, 10.0) - 20.483).abs() < 5e-4);
        assert!((chi2_ppf(0.025, 10.0) - 3.247).abs() < 5e-4);
    }

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
