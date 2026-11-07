//! Power and sample size (`docs/plano_lean_seis_sigma.md`, 2.3, P2) of the comparisons a
//! belt plans before collecting: the mean of one sample against a value, two means
//! (independent samples, equal sizes) and two proportions, two-sided at significance `α`.
//! The power of the t tests uses the normal approximation of the noncentral t with the
//! degrees of freedom of the t quantile (good from about ten per group); the sample size is
//! the smallest `n` whose power reaches the target.

use crate::special::{norm_cdf, norm_ppf, t_ppf};

/// What is compared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Design {
    /// One sample against a value: difference `delta`, standard deviation `sd`.
    OneMean { delta: f64, sd: f64 },
    /// Two independent samples of `n` each: difference of means, common `sd`.
    TwoMeans { delta: f64, sd: f64 },
    /// Two proportions `p1` and `p2`, `n` in each group.
    TwoProportions { p1: f64, p2: f64 },
}

/// Power with `n` per group (per sample) at significance `alpha`; `None` for an
/// impossible design (no spread, equal proportions, `n` too small).
pub fn power(d: Design, n: usize, alpha: f64) -> Option<f64> {
    let nf = n as f64;
    let two = |z: f64, crit: f64| norm_cdf(z - crit) + norm_cdf(-z - crit);
    match d {
        Design::OneMean { delta, sd } => {
            if sd.is_nan() || sd <= 0.0 || n < 2 {
                return None;
            }
            let crit = t_ppf(1.0 - alpha / 2.0, nf - 1.0);
            Some(two(delta.abs() / sd * nf.sqrt(), crit))
        }
        Design::TwoMeans { delta, sd } => {
            if sd.is_nan() || sd <= 0.0 || n < 2 {
                return None;
            }
            let crit = t_ppf(1.0 - alpha / 2.0, 2.0 * nf - 2.0);
            Some(two(delta.abs() / (sd * (2.0 / nf).sqrt()), crit))
        }
        Design::TwoProportions { p1, p2 } => {
            let ok = |p: f64| p > 0.0 && p < 1.0;
            if !ok(p1) || !ok(p2) || p1 == p2 || n < 1 {
                return None;
            }
            let pb = 0.5 * (p1 + p2);
            let z = norm_ppf(1.0 - alpha / 2.0);
            let s0 = (2.0 * pb * (1.0 - pb) / nf).sqrt();
            let s1 = ((p1 * (1.0 - p1) + p2 * (1.0 - p2)) / nf).sqrt();
            let d = (p1 - p2).abs();
            Some(norm_cdf((d - z * s0) / s1) + norm_cdf((-d - z * s0) / s1))
        }
    }
}

/// The smallest `n` per group (per sample) with power at least `target`, up to one
/// million; `None` when not reachable or the design is impossible.
pub fn sample_size(d: Design, target: f64, alpha: f64) -> Option<usize> {
    if target.is_nan()
        || target <= 0.0
        || target >= 1.0
        || alpha.is_nan()
        || alpha <= 0.0
        || alpha >= 1.0
    {
        return None;
    }
    let ok = |n: usize| power(d, n, alpha).is_some_and(|p| p >= target);
    let start = if matches!(d, Design::TwoProportions { .. }) {
        1
    } else {
        2
    };
    power(d, start.max(2), alpha)?;
    let mut hi = start;
    while !ok(hi) {
        hi *= 2;
        if hi > 1_000_000 {
            return None;
        }
    }
    let mut lo = hi / 2;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if ok(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some(hi.max(start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_means_matches_the_normal_formula_for_large_n() {
        // n ≈ 2 (z₀,₉₇₅ + z₀,₈)² / d², with d = 0,5: about 63, one or two more with t.
        let d = Design::TwoMeans {
            delta: 0.5,
            sd: 1.0,
        };
        let n = sample_size(d, 0.8, 0.05).unwrap();
        let z = norm_ppf(0.975) + norm_ppf(0.8);
        let approx = (2.0 * z * z / 0.25).ceil() as usize;
        assert!(n >= approx && n <= approx + 2, "{n} vs {approx}");
        assert!(power(d, n, 0.05).unwrap() >= 0.8);
        assert!(power(d, n - 1, 0.05).unwrap() < 0.8);
        // Power grows with n and with the difference.
        assert!(power(d, 100, 0.05).unwrap() > power(d, 50, 0.05).unwrap());
        assert!(
            power(
                Design::TwoMeans {
                    delta: 1.0,
                    sd: 1.0
                },
                20,
                0.05
            )
            .unwrap()
                > power(d, 20, 0.05).unwrap()
        );
    }

    #[test]
    fn one_mean_and_proportions() {
        let n1 = sample_size(
            Design::OneMean {
                delta: 0.5,
                sd: 1.0,
            },
            0.8,
            0.05,
        )
        .unwrap();
        assert!((32..=36).contains(&n1), "{n1}");
        // 10 % → 5 % defects: the classical about 430–440 per group.
        let np = sample_size(Design::TwoProportions { p1: 0.10, p2: 0.05 }, 0.8, 0.05).unwrap();
        assert!((420..=450).contains(&np), "{np}");
        assert_eq!(
            power(Design::TwoProportions { p1: 0.1, p2: 0.1 }, 50, 0.05),
            None
        );
        assert_eq!(
            sample_size(
                Design::OneMean {
                    delta: 1.0,
                    sd: 0.0
                },
                0.8,
                0.05
            ),
            None
        );
    }
}
