//! Output analysis: how much to trust the numbers of a simulation.
//!
//! Pure functions over numbers (no model, no kernel), used by `utsim-model::analysis`
//! and the `utsim analisar` command (see `docs/plano_analises_e_dinamica.md`, section 2):
//! - [`special`]: special functions (moved from `utsim-fit`, which re-exports them) and
//!   the Student-t quantile [`special::t_ppf`];
//! - [`summary`] / [`t_quantile`]: confidence intervals at any level, identical to
//!   [`Summary::from_samples`] at the default 95%;
//! - [`warmup`]: Welch's moving average and MSER-5;
//! - [`precision`]: replications needed for a target half-width, batch means;
//! - [`little`]: Little's law `L = λ·W` as a consistency check.

pub mod little;
pub mod precision;
pub mod special;
pub mod warmup;

use utsim_core::stats::{Summary, t_975};

/// Confidence level of every interval unless the user picks another one.
pub const DEFAULT_LEVEL: f64 = 0.95;

/// Whether `level` is the default 95% (the published table is used then).
fn is_default(level: f64) -> bool {
    (level - DEFAULT_LEVEL).abs() < 1e-12
}

/// Two-sided Student-t quantile for a confidence `level` in `(0, 1)` and `df` degrees
/// of freedom: `t_{1-(1-level)/2, df}`. At 95% it is the table of
/// [`utsim_core::stats::t_975`], so every 95% interval stays exactly as before; other
/// levels use [`special::t_ppf`]. NaN for `df = 0` or a level outside `(0, 1)`.
pub fn t_quantile(level: f64, df: usize) -> f64 {
    if df == 0 || !(level > 0.0 && level < 1.0) {
        return f64::NAN;
    }
    if is_default(level) {
        t_975(df)
    } else {
        special::t_ppf(1.0 - (1.0 - level) / 2.0, df as f64)
    }
}

/// Across-replication summary with a confidence interval at `level`. At 95% it is
/// exactly [`Summary::from_samples`] (bit for bit).
pub fn summary(xs: &[f64], level: f64) -> Summary {
    let mut s = Summary::from_samples(xs);
    if !is_default(level) && s.n >= 2 {
        s.half_width = t_quantile(level, s.n - 1) * s.std_dev / (s.n as f64).sqrt();
    }
    s
}

/// Parses a confidence level written as `95%`, `95`, `0.95` or `0,95`. `None` when
/// it is not strictly between 0 and 1 (0% and 100%).
pub fn parse_level(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    let (number, percent) = match t.strip_suffix('%') {
        Some(n) => (n.trim().to_string(), true),
        None => (t.clone(), false),
    };
    let v: f64 = number.parse().ok()?;
    let level = if percent || v > 1.0 { v / 100.0 } else { v };
    (level > 0.0 && level < 1.0).then_some(level)
}

/// Lag-1 sample autocorrelation (the same estimator as `utsim-fit`'s diagnostics:
/// lagged covariance over the total sum of squares). 0 for a constant series and for
/// fewer than two values.
pub fn lag1_autocorrelation(xs: &[f64]) -> f64 {
    let n = xs.len();
    if n < 2 {
        return 0.0;
    }
    let mean = xs.iter().sum::<f64>() / n as f64;
    let denom: f64 = xs.iter().map(|x| (x - mean).powi(2)).sum();
    if denom <= 0.0 {
        return 0.0;
    }
    let cov: f64 = xs.windows(2).map(|w| (w[0] - mean) * (w[1] - mean)).sum();
    cov / denom
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table of `utsim-core` that every 95% interval used before this crate.
    const TABLE_95: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];

    #[test]
    fn general_quantile_reproduces_the_table() {
        for (k, &want) in TABLE_95.iter().enumerate() {
            let df = k + 1;
            let t = special::t_ppf(0.975, df as f64);
            // The table has three decimals: the general quantile rounds to it.
            assert_eq!((t * 1000.0).round() / 1000.0, want, "df {df}: {t}");
            assert!((t - want).abs() < 5e-4);
            // The default level keeps the table itself.
            assert_eq!(t_quantile(0.95, df), want);
        }
        assert_eq!(t_quantile(0.95, 200), 1.960);
        assert!(t_quantile(0.95, 0).is_nan() && t_quantile(1.0, 5).is_nan());
    }

    #[test]
    fn default_level_is_bit_identical() {
        let xs = [3.1, 2.7, 3.9, 4.4, 2.2, 3.0, 3.3];
        let a = Summary::from_samples(&xs);
        let b = summary(&xs, 0.95);
        assert_eq!(a.half_width.to_bits(), b.half_width.to_bits());
        assert_eq!(a.mean.to_bits(), b.mean.to_bits());
        // 90% and 99% bracket the 95% interval, with the published quantiles.
        let s90 = summary(&xs, 0.90);
        let s99 = summary(&xs, 0.99);
        assert!(s90.half_width < a.half_width && a.half_width < s99.half_width);
        let se = a.std_dev / 7f64.sqrt();
        assert!((s90.half_width / se - 1.943).abs() < 5e-4);
        assert!((s99.half_width / se - 3.707).abs() < 5e-4);
        assert!(summary(&[1.0], 0.9).half_width.is_nan());
    }

    #[test]
    fn levels_parse_in_every_usual_spelling() {
        for text in ["95%", "95", "0.95", "0,95", " 95 % "] {
            assert_eq!(parse_level(text), Some(0.95), "{text}");
        }
        assert_eq!(parse_level("99,5%"), Some(0.995));
        assert_eq!(parse_level("100%"), None);
        assert_eq!(parse_level("0"), None);
        assert_eq!(parse_level("abc"), None);
    }

    #[test]
    fn lag1_autocorrelation_by_hand() {
        // mean 2.5, Σ(x-m)² = 5, Σ lagged products = (-1.5)(-0.5) + (-0.5)(0.5) + (0.5)(1.5) = 1.25
        assert!((lag1_autocorrelation(&[1.0, 2.0, 3.0, 4.0]) - 0.25).abs() < 1e-15);
        assert_eq!(lag1_autocorrelation(&[2.0, 2.0, 2.0]), 0.0);
        assert_eq!(lag1_autocorrelation(&[1.0]), 0.0);
    }
}
