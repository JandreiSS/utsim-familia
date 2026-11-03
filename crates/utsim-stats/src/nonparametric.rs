//! Rank tests: Mann-Whitney (Wilcoxon rank sum) for two independent samples and the
//! Wilcoxon signed-rank test for one sample or paired samples.
//!
//! **Mann-Whitney.** `U₁ = R₁ − n₁(n₁+1)/2` (`R₁` the rank sum of the first sample, ties
//! at mid-ranks) counts the pairs with `x > y` (half for ties). Without ties and with
//! `n₁ + n₂ ≤` [`EXACT_LIMIT`], the p-value is exact: the null distribution of `U₁` is
//! the coefficients of the Gaussian binomial `[n₁+n₂ choose n₁]_q`, over
//! `C(n₁+n₂, n₁)`. Otherwise it is the normal approximation with mean `n₁n₂/2`,
//! variance `n₁n₂/12 · (N + 1 − Σ(t³ − t)/(N(N − 1)))` (tie correction) and continuity
//! correction 0.5. The shift is estimated by Hodges and Lehmann: the median of the
//! `n₁n₂` differences `xᵢ − yⱼ`.
//!
//! **Wilcoxon signed rank.** `d = x − μ₀` (or `x − y`); zeros are dropped (Wilcoxon's
//! convention), `|d|` is ranked and `W⁺` is the rank sum of the positive differences.
//! Exact without ties and with `n ≤` [`EXACT_LIMIT`]: the distribution of `W⁺` is the
//! coefficients of `∏ (1 + q^i)` over `2ⁿ`. Otherwise normal with mean `n(n+1)/4`,
//! variance `n(n+1)(2n+1)/24 − Σ(t³ − t)/48` and continuity correction 0.5. The location
//! is estimated by the Hodges-Lehmann pseudo-median (median of the Walsh averages).
//!
//! Formulas as in Hollander, Wolfe and Chicken, *Nonparametric Statistical Methods*,
//! chapters 3 and 4.

use serde::Serialize;

use crate::correlation::ranks;
use crate::descriptive::{QuantileMethod, quantile_sorted};
use crate::hypothesis::{Alternative, PValues};
use crate::special::norm_cdf;

/// Largest total sample size with an exact p-value (the counts stay exact in `u128`).
pub const EXACT_LIMIT: usize = 60;

/// How the p-value of a rank test is computed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum RankMethod {
    /// Exact when there are no ties and the sample is small enough, else normal.
    #[default]
    Auto,
    /// Always the normal approximation.
    Normal,
}

/// Result of the Mann-Whitney test.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MannWhitney {
    pub n1: usize,
    pub n2: usize,
    /// Rank sum of the first sample (Wilcoxon's `W`).
    pub rank_sum: f64,
    /// `U₁ = W − n₁(n₁+1)/2`; `U₂ = n₁n₂ − U₁`.
    pub u1: f64,
    pub u2: f64,
    /// Whether the samples share a value (then the approximation is used).
    pub ties: bool,
    /// Whether the p-values are exact.
    pub exact: bool,
    /// Standardised `U₁` with the continuity correction (also given when exact).
    pub z: f64,
    /// About the location of `x` against `y`: `less` for `x` tending smaller.
    pub alternative: Alternative,
    pub p_value: f64,
    pub p: PValues,
    /// Hodges-Lehmann estimate of the shift `x − y`.
    pub shift: f64,
}

/// Mann-Whitney test of `x` against `y` (NaN dropped). `None` when a sample is empty.
pub fn mann_whitney(
    x: &[f64],
    y: &[f64],
    method: RankMethod,
    alternative: Alternative,
) -> Option<MannWhitney> {
    let x: Vec<f64> = x.iter().copied().filter(|v| !v.is_nan()).collect();
    let y: Vec<f64> = y.iter().copied().filter(|v| !v.is_nan()).collect();
    let (n1, n2) = (x.len(), y.len());
    if n1 == 0 || n2 == 0 {
        return None;
    }
    let all: Vec<f64> = x.iter().chain(&y).copied().collect();
    let r = ranks(&all);
    let rank_sum: f64 = r[..n1].iter().sum();
    let (f1, f2) = (n1 as f64, n2 as f64);
    let u1 = rank_sum - f1 * (f1 + 1.0) / 2.0;
    let ties_term = tie_sum(&all);
    let ties = ties_term > 0.0;
    let nn = f1 + f2;
    let mu = f1 * f2 / 2.0;
    let var = f1 * f2 / 12.0 * (nn + 1.0 - ties_term / (nn * (nn - 1.0)));
    let z = corrected_z(u1, mu, var);
    let exact = method == RankMethod::Auto && !ties && n1 + n2 <= EXACT_LIMIT;
    let p = if exact {
        let u = u1.round() as usize;
        let dist = mann_whitney_distribution(n1, n2);
        tails(&dist, u)
    } else {
        normal_tails(u1, mu, var)
    };
    let mut diffs: Vec<f64> = x
        .iter()
        .flat_map(|a| y.iter().map(move |b| a - b))
        .collect();
    diffs.sort_by(|a, b| a.total_cmp(b));
    Some(MannWhitney {
        n1,
        n2,
        rank_sum,
        u1,
        u2: f1 * f2 - u1,
        ties,
        exact,
        z,
        alternative,
        p_value: p.get(alternative),
        p,
        shift: quantile_sorted(&diffs, 0.5, QuantileMethod::Linear),
    })
}

/// Result of the Wilcoxon signed-rank test.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Wilcoxon {
    /// Non-zero differences used.
    pub n: usize,
    /// Differences equal to zero, dropped.
    pub zeros: usize,
    /// Rank sums of the positive and of the negative differences.
    pub w_plus: f64,
    pub w_minus: f64,
    pub ties: bool,
    pub exact: bool,
    /// Standardised `W⁺` with the continuity correction.
    pub z: f64,
    /// About the location of the differences against `μ₀`.
    pub alternative: Alternative,
    pub p_value: f64,
    pub p: PValues,
    /// Hodges-Lehmann pseudo-median of `x` (or of `x − y`), `μ₀` not subtracted.
    pub pseudo_median: f64,
}

/// Wilcoxon signed-rank test of `H₀`: the differences `x − mu0` are symmetric about 0
/// (NaN dropped). `None` when no non-zero difference is left.
pub fn wilcoxon_signed_rank(
    x: &[f64],
    mu0: f64,
    method: RankMethod,
    alternative: Alternative,
) -> Option<Wilcoxon> {
    let values: Vec<f64> = x.iter().copied().filter(|v| !v.is_nan()).collect();
    let d: Vec<f64> = values.iter().map(|v| v - mu0).collect();
    let nonzero: Vec<f64> = d.iter().copied().filter(|v| *v != 0.0).collect();
    let n = nonzero.len();
    if n == 0 {
        return None;
    }
    let abs: Vec<f64> = nonzero.iter().map(|v| v.abs()).collect();
    let r = ranks(&abs);
    let w_plus: f64 = r
        .iter()
        .zip(&nonzero)
        .filter(|(_, v)| **v > 0.0)
        .map(|(r, _)| r)
        .sum();
    let nf = n as f64;
    let total = nf * (nf + 1.0) / 2.0;
    let ties_term = tie_sum(&abs);
    let ties = ties_term > 0.0;
    let mu = total / 2.0;
    let var = nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0 - ties_term / 48.0;
    let exact = method == RankMethod::Auto && !ties && n <= EXACT_LIMIT;
    let p = if exact {
        tails(&signed_rank_distribution(n), w_plus.round() as usize)
    } else {
        normal_tails(w_plus, mu, var)
    };
    let mut walsh = Vec::with_capacity(values.len() * (values.len() + 1) / 2);
    for i in 0..values.len() {
        for j in i..values.len() {
            walsh.push((values[i] + values[j]) / 2.0);
        }
    }
    walsh.sort_by(|a, b| a.total_cmp(b));
    Some(Wilcoxon {
        n,
        zeros: d.len() - n,
        w_plus,
        w_minus: total - w_plus,
        ties,
        exact,
        z: corrected_z(w_plus, mu, var),
        alternative,
        p_value: p.get(alternative),
        p,
        pseudo_median: quantile_sorted(&walsh, 0.5, QuantileMethod::Linear),
    })
}

/// Wilcoxon signed-rank test of paired samples, on `xᵢ − yᵢ` against `delta0`.
pub fn wilcoxon_paired(
    x: &[f64],
    y: &[f64],
    delta0: f64,
    method: RankMethod,
    alternative: Alternative,
) -> Option<Wilcoxon> {
    wilcoxon_signed_rank(
        &crate::hypothesis::differences(x, y),
        delta0,
        method,
        alternative,
    )
}

/// `Σ (t³ − t)` over the groups of tied values.
fn tie_sum(xs: &[f64]) -> f64 {
    let mut s = xs.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let mut total = 0.0;
    let mut i = 0;
    while i < s.len() {
        let mut j = i;
        while j + 1 < s.len() && s[j + 1] == s[i] {
            j += 1;
        }
        let t = (j - i + 1) as f64;
        total += t * t * t - t;
        i = j + 1;
    }
    total
}

/// `(s − μ)/σ` moved 0.5 toward zero (continuity correction).
fn corrected_z(s: f64, mu: f64, var: f64) -> f64 {
    if var.is_nan() || var <= 0.0 {
        return f64::NAN;
    }
    let d = s - mu;
    d.signum() * (d.abs() - 0.5).max(0.0) / var.sqrt()
}

/// Normal-approximation p-values of a statistic `s` with continuity correction.
fn normal_tails(s: f64, mu: f64, var: f64) -> PValues {
    if var.is_nan() || var <= 0.0 {
        return PValues {
            two_sided: f64::NAN,
            less: f64::NAN,
            greater: f64::NAN,
        };
    }
    let sd = var.sqrt();
    let less = norm_cdf((s - mu + 0.5) / sd);
    let greater = 1.0 - norm_cdf((s - mu - 0.5) / sd);
    PValues {
        two_sided: (2.0 * less.min(greater)).min(1.0),
        less,
        greater,
    }
}

/// Exact p-values of the value `s` of a statistic with the null probabilities `dist`
/// (indexed by the value, symmetric).
fn tails(dist: &[f64], s: usize) -> PValues {
    let s = s.min(dist.len() - 1);
    let less: f64 = dist[..=s].iter().sum();
    let greater: f64 = dist[s..].iter().sum();
    PValues {
        two_sided: (2.0 * less.min(greater)).min(1.0),
        less: less.min(1.0),
        greater: greater.min(1.0),
    }
}

/// Null probabilities `P(U = u)`, `u = 0..=n₁n₂`, of the Mann-Whitney statistic without
/// ties: the coefficients of `∏_{i=1}^{n₁} (1 − q^{n₂+i}) / (1 − q^i)` over
/// `C(n₁+n₂, n₁)`. Empty when `n₁ + n₂ > EXACT_LIMIT`.
pub fn mann_whitney_distribution(n1: usize, n2: usize) -> Vec<f64> {
    if n1 + n2 > EXACT_LIMIT {
        return Vec::new();
    }
    let deg = n1 * n2;
    let mut c = vec![0i128; deg + 1];
    c[0] = 1;
    for i in 1..=n1 {
        // Multiply by (1 − q^{n₂+i}), from the top so the old values are used.
        let a = n2 + i;
        for k in (a..=deg).rev() {
            c[k] -= c[k - a];
        }
        // Divide by (1 − q^i): running sum with step i (exact, the quotient is a
        // polynomial).
        for k in i..=deg {
            c[k] += c[k - i];
        }
    }
    let total: i128 = c.iter().sum();
    c.iter().map(|&v| v as f64 / total as f64).collect()
}

/// Null probabilities `P(W⁺ = w)`, `w = 0..=n(n+1)/2`, of the signed-rank statistic
/// without ties: the coefficients of `∏_{i=1}^{n} (1 + q^i)` over `2ⁿ`. Empty when
/// `n > EXACT_LIMIT`.
pub fn signed_rank_distribution(n: usize) -> Vec<f64> {
    if n > EXACT_LIMIT {
        return Vec::new();
    }
    let deg = n * (n + 1) / 2;
    let mut c = vec![0u128; deg + 1];
    c[0] = 1;
    for i in 1..=n {
        for k in (i..=deg).rev() {
            c[k] += c[k - i];
        }
    }
    let total = 2f64.powi(n as i32);
    c.iter().map(|&v| v as f64 / total).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol * (1.0 + b.abs()), "{a} vs {b}");
    }

    /// Exact null distribution of `U` by enumerating every way of choosing the ranks
    /// of the first sample.
    fn brute_mann_whitney(n1: usize, n2: usize) -> Vec<f64> {
        let n = n1 + n2;
        let mut counts = vec![0u64; n1 * n2 + 1];
        let mut total = 0u64;
        for mask in 0u32..(1 << n) {
            if mask.count_ones() as usize != n1 {
                continue;
            }
            let rank_sum: usize = (0..n).filter(|i| mask & (1 << i) != 0).map(|i| i + 1).sum();
            counts[rank_sum - n1 * (n1 + 1) / 2] += 1;
            total += 1;
        }
        counts.iter().map(|&c| c as f64 / total as f64).collect()
    }

    #[test]
    fn exact_distributions_match_enumeration() {
        for (n1, n2) in [(1, 1), (2, 3), (4, 5), (3, 7), (6, 6)] {
            let fast = mann_whitney_distribution(n1, n2);
            let slow = brute_mann_whitney(n1, n2);
            assert_eq!(fast.len(), slow.len());
            for (a, b) in fast.iter().zip(&slow) {
                close(*a, *b, 1e-15);
            }
        }
        for n in 1..=12 {
            let fast = signed_rank_distribution(n);
            let mut counts = vec![0u64; n * (n + 1) / 2 + 1];
            for mask in 0u32..(1 << n) {
                let w: usize = (0..n).filter(|i| mask & (1 << i) != 0).map(|i| i + 1).sum();
                counts[w] += 1;
            }
            for (a, c) in fast.iter().zip(&counts) {
                close(*a, *c as f64 / (1u64 << n) as f64, 1e-15);
            }
        }
        assert!(mann_whitney_distribution(40, 30).is_empty());
    }

    /// Critical values of the published tables: Mann-Whitney with n₁ = n₂ = 10 at 5%
    /// two-sided is U = 23 (rank sum 78); Wilcoxon signed rank with n = 20 at 5%
    /// two-sided is 52 (Montgomery and Runger, appendix tables of the Wilcoxon tests).
    #[test]
    fn exact_distributions_reproduce_the_tables() {
        let mw = mann_whitney_distribution(10, 10);
        let cdf = |d: &[f64], k: usize| d[..=k].iter().sum::<f64>();
        assert!(cdf(&mw, 23) <= 0.025 && cdf(&mw, 24) > 0.025);
        let sr = signed_rank_distribution(20);
        assert!(cdf(&sr, 52) <= 0.025 && cdf(&sr, 53) > 0.025);
        // n = 10 at 5% two-sided: 8.
        let sr10 = signed_rank_distribution(10);
        assert!(cdf(&sr10, 8) <= 0.025 && cdf(&sr10, 9) > 0.025);
    }

    /// Montgomery and Runger (Wilcoxon rank-sum example of the axial stress of two
    /// aluminium alloys): W₁ = 99 and W₂ = 111 with n₁ = n₂ = 10; both above the
    /// critical 78, so no difference at 5%.
    #[test]
    fn mann_whitney_aluminium_alloys() {
        let a1 = [
            3238.0, 3195.0, 3246.0, 3190.0, 3204.0, 3254.0, 3229.0, 3225.0, 3217.0, 3241.0,
        ];
        let a2 = [
            3261.0, 3187.0, 3209.0, 3212.0, 3258.0, 3248.0, 3215.0, 3226.0, 3240.0, 3234.0,
        ];
        let m = mann_whitney(&a1, &a2, RankMethod::Auto, Alternative::TwoSided).unwrap();
        assert_eq!(m.rank_sum, 99.0);
        assert_eq!((m.u1, m.u2), (44.0, 56.0));
        assert!(m.exact && !m.ties);
        assert!(m.p_value > 0.05);
        let dist = mann_whitney_distribution(10, 10);
        close(m.p.less, dist[..=44].iter().sum(), 1e-15);
        // The normal approximation is close at this size.
        let n = mann_whitney(&a1, &a2, RankMethod::Normal, Alternative::TwoSided).unwrap();
        assert!(!n.exact);
        assert!((n.p_value - m.p_value).abs() < 0.02);
        // Swapping the samples swaps U and the one-sided p-values.
        let s = mann_whitney(&a2, &a1, RankMethod::Auto, Alternative::TwoSided).unwrap();
        assert_eq!(s.u1, m.u2);
        close(s.p.greater, m.p.less, 1e-15);
        close(s.shift, -m.shift, 1e-15);
    }

    #[test]
    fn mann_whitney_ties_by_hand() {
        // x = 1, 2, 2; y = 2, 3. Mid-ranks: 1, 3, 3 | 3, 5; R₁ = 7, U₁ = 1.
        // One tie group of 3: Σ(t³ − t) = 24; variance 6/12 · (6 − 24/20) = 2.4.
        let m = mann_whitney(
            &[1.0, 2.0, 2.0],
            &[2.0, 3.0],
            RankMethod::Auto,
            Alternative::Less,
        )
        .unwrap();
        assert!(m.ties && !m.exact);
        assert_eq!((m.rank_sum, m.u1), (7.0, 1.0));
        close(m.p.less, norm_cdf((1.0 - 3.0 + 0.5) / 2.4f64.sqrt()), 1e-15);
        close(m.z, -1.5 / 2.4f64.sqrt(), 1e-15);
        assert!(mann_whitney(&[], &[1.0], RankMethod::Auto, Alternative::Less).is_none());
    }

    /// Montgomery and Runger (Wilcoxon signed-rank example of the shear strength of a
    /// propellant, μ₀ = 2000 psi, n = 20): W⁺ = 150, W⁻ = 60; the critical value at 5%
    /// two-sided is 52, so H₀ is not rejected.
    #[test]
    fn wilcoxon_propellant_shear_strength() {
        let x = [
            2158.70, 1678.15, 2316.00, 2061.30, 2207.50, 1708.30, 1784.70, 2575.10, 2357.90,
            2256.70, 2165.20, 2399.55, 1779.80, 2336.75, 1765.30, 2053.50, 2414.40, 2200.50,
            2654.20, 1753.70,
        ];
        let w = wilcoxon_signed_rank(&x, 2000.0, RankMethod::Auto, Alternative::TwoSided).unwrap();
        assert_eq!((w.n, w.zeros), (20, 0));
        assert_eq!((w.w_plus, w.w_minus), (150.0, 60.0));
        assert!(w.exact);
        assert!(w.p_value > 0.05 && w.p_value < 0.2, "{}", w.p_value);
        let dist = signed_rank_distribution(20);
        close(w.p.greater, dist[150..].iter().sum(), 1e-15);
        close(w.p.two_sided, 2.0 * dist[..=60].iter().sum::<f64>(), 1e-14);
    }

    #[test]
    fn wilcoxon_zeros_ties_and_pairs() {
        // d = 0 (dropped), 1, −1, 2: |d| ranks 1.5, 1.5, 3; W⁺ = 4.5, one tie pair.
        let w = wilcoxon_signed_rank(
            &[5.0, 6.0, 4.0, 7.0],
            5.0,
            RankMethod::Auto,
            Alternative::Greater,
        )
        .unwrap();
        assert_eq!((w.n, w.zeros), (3, 1));
        assert_eq!((w.w_plus, w.w_minus), (4.5, 1.5));
        assert!(w.ties && !w.exact);
        // Variance 3·4·7/24 − 6/48 = 3.375.
        close(
            w.p.greater,
            1.0 - norm_cdf((4.5 - 3.0 - 0.5) / 3.375f64.sqrt()),
            1e-15,
        );
        // Paired is the one-sample test on the differences.
        let x = [1.2, 3.4, 2.2, 5.1, 4.4];
        let y = [1.0, 3.0, 2.5, 4.0, 4.1];
        let p = wilcoxon_paired(&x, &y, 0.0, RankMethod::Auto, Alternative::TwoSided).unwrap();
        let d: Vec<f64> = x.iter().zip(&y).map(|(a, b)| a - b).collect();
        let o = wilcoxon_signed_rank(&d, 0.0, RankMethod::Auto, Alternative::TwoSided).unwrap();
        assert_eq!(p, o);
        assert!(
            wilcoxon_signed_rank(&[2.0, 2.0], 2.0, RankMethod::Auto, Alternative::Less).is_none()
        );
        // Pseudo-median of 1, 2, 3: Walsh averages 1, 1.5, 2, 2, 2.5, 3 → 2.
        let h = wilcoxon_signed_rank(
            &[1.0, 2.0, 3.0],
            0.0,
            RankMethod::Auto,
            Alternative::TwoSided,
        )
        .unwrap();
        close(h.pseudo_median, 2.0, 1e-15);
    }
}
