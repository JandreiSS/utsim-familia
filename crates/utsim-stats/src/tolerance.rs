//! Tolerance intervals: limits that hold a given proportion of the population
//! (`coverage`) with a given confidence.
//!
//! - [`normal`]: `x̄ ± k·s` for normal data, with the exact factors
//!   ([`k_two_sided`] by Odeh's integral, [`k_one_sided`] by the noncentral t);
//! - [`nonparametric`]: order statistics for any continuous distribution, with the
//!   confidence actually reached and the smallest sample that reaches the one asked
//!   ([`min_n_nonparametric`]).

use crate::descriptive::{mean, std_dev};
use crate::special::{beta_inc, brent, chi2_sf, ln_gamma, norm_cdf, norm_ppf};

/// Which limits the interval has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sides {
    /// Lower and upper limits.
    Two,
    /// Only a lower limit (the proportion lies above it).
    Lower,
    /// Only an upper limit (the proportion lies below it).
    Upper,
}

/// A normal tolerance interval `x̄ ± k·s`.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalTolerance {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    /// The tolerance factor.
    pub k: f64,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
}

/// A nonparametric tolerance interval between order statistics.
#[derive(Clone, Debug, PartialEq)]
pub struct NonparametricTolerance {
    pub n: usize,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
    /// 1-based ranks of the order statistics used (`None` on an open side).
    pub lower_rank: Option<usize>,
    pub upper_rank: Option<usize>,
    /// Confidence that the interval holds at least `coverage` of the population.
    pub achieved: f64,
    /// Whether `achieved` reaches the confidence asked (otherwise the sample is too small
    /// and the interval is the widest one, from the minimum or to the maximum).
    pub enough: bool,
}

fn valid(p: f64) -> bool {
    p > 0.0 && p < 1.0
}

/// Composite Simpson rule on `[a, b]` with `m` (even) intervals.
fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64, m: usize) -> f64 {
    let h = (b - a) / m as f64;
    let mut s = f(a) + f(b);
    for i in 1..m {
        s += f(a + i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    s * h / 3.0
}

/// Confidence of `x̄ ± k·s` (two-sided) for `n` normal values: Odeh's integral
/// `√(2n/π) ∫₀^∞ Q_{χ²,n−1}((n−1)·r(x)²/k²) e^{−n x²/2} dx`, where `Φ(x+r) − Φ(x−r) = p`.
fn confidence_two_sided(n: usize, p: f64, k: f64) -> f64 {
    let nf = n as f64;
    let df = nf - 1.0;
    let z = norm_ppf((1.0 + p) / 2.0);
    let r = |x: f64| {
        brent(
            |r| norm_cdf(x + r) - norm_cdf(x - r) - p,
            0.0,
            z + x + 10.0,
            1e-13,
        )
        .unwrap_or(z)
    };
    let top = 10.0 / nf.sqrt();
    let f = |x: f64| chi2_sf(df * r(x).powi(2) / (k * k), df) * (-nf * x * x / 2.0).exp();
    (2.0 * nf / std::f64::consts::PI).sqrt() * simpson(f, 0.0, top, 400)
}

/// Exact two-sided normal tolerance factor: `x̄ ± k·s` holds at least `coverage` of the
/// population with probability `confidence`. `None` for `n < 2` or levels outside (0, 1).
pub fn k_two_sided(n: usize, coverage: f64, confidence: f64) -> Option<f64> {
    if n < 2 || !valid(coverage) || !valid(confidence) {
        return None;
    }
    let z = norm_ppf((1.0 + coverage) / 2.0);
    let g = |k: f64| confidence_two_sided(n, coverage, k) - confidence;
    let mut hi = 2.0 * z + 1.0;
    while g(hi) < 0.0 {
        hi *= 2.0;
        if hi > 1e6 {
            return None;
        }
    }
    brent(g, z * 0.5, hi, 1e-10)
}

/// `P(T ≤ t)` for the noncentral t with `df` degrees of freedom and noncentrality
/// `delta`: `∫₀^∞ Φ(t·w − δ) g(w) dw`, where `w = √(χ²_df / df)`.
fn noncentral_t_cdf(t: f64, df: f64, delta: f64) -> f64 {
    let ln_c = (df / 2.0) * df.ln() - (df / 2.0 - 1.0) * 2f64.ln() - ln_gamma(df / 2.0);
    let dens = |w: f64| {
        if w <= 0.0 {
            return if df == 1.0 { (ln_c).exp() } else { 0.0 };
        }
        (ln_c + (df - 1.0) * w.ln() - df * w * w / 2.0).exp()
    };
    let top = 1.0 + 12.0 / df.sqrt();
    simpson(|w| norm_cdf(t * w - delta) * dens(w), 0.0, top, 2000)
}

/// Exact one-sided normal tolerance factor: `x̄ − k·s` (or `x̄ + k·s`) leaves at least
/// `coverage` of the population above (below) it with probability `confidence`.
pub fn k_one_sided(n: usize, coverage: f64, confidence: f64) -> Option<f64> {
    if n < 2 || !valid(coverage) || !valid(confidence) {
        return None;
    }
    let nf = n as f64;
    let delta = norm_ppf(coverage) * nf.sqrt();
    let g = |k: f64| noncentral_t_cdf(k * nf.sqrt(), nf - 1.0, delta) - confidence;
    let mut lo = norm_ppf(coverage) - 1.0;
    while g(lo) > 0.0 {
        lo -= 2.0;
        if lo < -1e6 {
            return None;
        }
    }
    let mut hi = norm_ppf(coverage).abs() + 2.0;
    while g(hi) < 0.0 {
        hi *= 2.0;
        if hi > 1e6 {
            return None;
        }
    }
    brent(g, lo, hi, 1e-10)
}

/// Normal tolerance interval of `xs` (non-finite values are ignored).
pub fn normal(xs: &[f64], coverage: f64, confidence: f64, sides: Sides) -> Option<NormalTolerance> {
    let v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    let n = v.len();
    let k = match sides {
        Sides::Two => k_two_sided(n, coverage, confidence)?,
        _ => k_one_sided(n, coverage, confidence)?,
    };
    let (m, s) = (mean(&v), std_dev(&v));
    Some(NormalTolerance {
        n,
        mean: m,
        sd: s,
        k,
        lower: (sides != Sides::Upper).then_some(m - k * s),
        upper: (sides != Sides::Lower).then_some(m + k * s),
    })
}

/// Confidence that the order statistics `r` and `n + 1 − r` (two-sided) or `r` alone
/// (one-sided) hold at least `coverage`: the coverage is Beta(n + 1 − m, m) with `m`
/// the ranks left out (2r or r).
fn order_confidence(n: usize, r: usize, coverage: f64, sides: Sides) -> f64 {
    let m = if sides == Sides::Two { 2 * r } else { r };
    if m > n {
        return 0.0;
    }
    1.0 - beta_inc((n + 1 - m) as f64, m as f64, coverage)
}

/// Nonparametric tolerance interval of `xs`: the narrowest symmetric pair of order
/// statistics (or the highest lower / lowest upper one) whose confidence reaches
/// `confidence`.
pub fn nonparametric(
    xs: &[f64],
    coverage: f64,
    confidence: f64,
    sides: Sides,
) -> Option<NonparametricTolerance> {
    if !valid(coverage) || !valid(confidence) {
        return None;
    }
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    let n = v.len();
    if n < 2 {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let mut r = 1;
    while order_confidence(n, r + 1, coverage, sides) >= confidence {
        r += 1;
    }
    let achieved = order_confidence(n, r, coverage, sides);
    let (lower_rank, upper_rank) = match sides {
        Sides::Two => (Some(r), Some(n + 1 - r)),
        Sides::Lower => (Some(r), None),
        Sides::Upper => (None, Some(n + 1 - r)),
    };
    Some(NonparametricTolerance {
        n,
        lower: lower_rank.map(|i| v[i - 1]),
        upper: upper_rank.map(|i| v[i - 1]),
        lower_rank,
        upper_rank,
        achieved,
        enough: achieved >= confidence,
    })
}

/// Smallest sample whose extreme values (minimum and maximum, or one of them) give the
/// nonparametric interval with `coverage` and `confidence`.
pub fn min_n_nonparametric(coverage: f64, confidence: f64, sides: Sides) -> Option<usize> {
    if !valid(coverage) || !valid(confidence) {
        return None;
    }
    (2..=1_000_000).find(|&n| order_confidence(n, 1, coverage, sides) >= confidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perto(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    #[test]
    fn exact_two_sided_factors_match_the_tables() {
        // Exact factors, each checked by simulation (300 000 normal samples reach the
        // confidence within 0.001): n = 10, P = 0.90, γ = 0.95 → 2.8563; n = 20,
        // P = 0.95, γ = 0.95 → 2.7604; n = 5, P = 0.99, γ = 0.99 → 10.220.
        perto(k_two_sided(10, 0.90, 0.95).unwrap(), 2.8563, 0.0005);
        perto(k_two_sided(20, 0.95, 0.95).unwrap(), 2.7604, 0.0005);
        perto(k_two_sided(5, 0.99, 0.99).unwrap(), 10.220, 0.002);
        // Close to Howe's approximation.
        let howe = |n: usize, p: f64, g: f64| {
            let nf = n as f64;
            let z = norm_ppf((1.0 + p) / 2.0);
            let chi = crate::special::chi2_ppf(1.0 - g, nf - 1.0);
            ((nf - 1.0) * (1.0 + 1.0 / nf) * z * z / chi).sqrt()
        };
        for (n, p, g) in [(8, 0.9, 0.9), (30, 0.99, 0.95), (100, 0.95, 0.99)] {
            let k = k_two_sided(n, p, g).unwrap();
            let h = howe(n, p, g);
            assert!((k - h).abs() < 0.02 * h, "n={n}: {k} vs Howe {h}");
        }
        // A huge sample approaches the normal quantile.
        perto(k_two_sided(20_000, 0.95, 0.95).unwrap(), 1.96, 0.03);
    }

    #[test]
    fn exact_one_sided_factors_match_the_tables() {
        // n = 10, P = 0.90, γ = 0.95 → 2.355; n = 10, P = 0.95, γ = 0.95 → 2.911;
        // n = 20, P = 0.95, γ = 0.95 → 2.396.
        perto(k_one_sided(10, 0.90, 0.95).unwrap(), 2.355, 0.002);
        perto(k_one_sided(10, 0.95, 0.95).unwrap(), 2.911, 0.002);
        perto(k_one_sided(20, 0.95, 0.95).unwrap(), 2.396, 0.002);
        perto(k_one_sided(2, 0.90, 0.95).unwrap(), 20.581, 0.05);
        assert!(k_one_sided(1, 0.9, 0.95).is_none());
        assert!(k_two_sided(10, 1.0, 0.95).is_none());
    }

    #[test]
    fn normal_interval_uses_the_sample() {
        let xs = [9.8, 10.2, 10.1, 9.9, 10.0, 10.3, 9.7, 10.0, 10.1, 9.9];
        let t = normal(&xs, 0.90, 0.95, Sides::Two).unwrap();
        perto(t.mean, 10.0, 1e-12);
        let k = k_two_sided(10, 0.90, 0.95).unwrap();
        perto(t.lower.unwrap(), 10.0 - k * t.sd, 1e-12);
        perto(t.upper.unwrap(), 10.0 + k * t.sd, 1e-12);
        let up = normal(&xs, 0.90, 0.95, Sides::Upper).unwrap();
        assert!(up.lower.is_none() && up.upper.unwrap() < t.upper.unwrap());
    }

    #[test]
    fn nonparametric_needs_enough_values() {
        // Minimum and maximum hold 90% with 95% confidence from 46 values on (two-sided),
        // 29 for one side.
        assert_eq!(min_n_nonparametric(0.90, 0.95, Sides::Two), Some(46));
        assert_eq!(min_n_nonparametric(0.90, 0.95, Sides::Lower), Some(29));
        assert_eq!(min_n_nonparametric(0.95, 0.95, Sides::Two), Some(93));
        let xs: Vec<f64> = (1..=46).map(f64::from).collect();
        let t = nonparametric(&xs, 0.90, 0.95, Sides::Two).unwrap();
        assert!(t.enough);
        assert_eq!((t.lower, t.upper), (Some(1.0), Some(46.0)));
        perto(t.achieved, 0.9520, 5e-4);
        let poucos = nonparametric(&xs[..45], 0.90, 0.95, Sides::Two).unwrap();
        assert!(!poucos.enough);
        perto(poucos.achieved, 0.9476, 5e-4);
        // A larger sample moves inward: 1 − I_0.9(n+1−2r, 2r) still ≥ 0.95.
        let xs: Vec<f64> = (1..=200).map(f64::from).collect();
        let t = nonparametric(&xs, 0.90, 0.95, Sides::Two).unwrap();
        let r = t.lower_rank.unwrap();
        assert!(r > 1 && t.upper_rank == Some(201 - r));
        assert!(order_confidence(200, r, 0.9, Sides::Two) >= 0.95);
        assert!(order_confidence(200, r + 1, 0.9, Sides::Two) < 0.95);
    }
}
