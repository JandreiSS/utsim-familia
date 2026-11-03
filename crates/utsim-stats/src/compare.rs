//! Comparing several scenarios: one-way ANOVA, Tukey's HSD (Tukey-Kramer for unequal
//! group sizes) and paired comparisons with the Bonferroni correction.
//!
//! - [`anova`] asks "do the scenarios differ at all?": the F statistic of the
//!   between-group over the within-group mean square, with its p-value from the
//!   F distribution ([`crate::special::f_sf`]).
//! - [`tukey`] asks "which pairs differ?", keeping the family-wise confidence at the
//!   chosen level for every pair at once, with the quantile of the studentized range
//!   ([`crate::range::qtukey`]). It assumes independent groups.
//! - [`bonferroni_paired`] is the version for **common random numbers**: when every
//!   scenario runs with the same seeds, replication `r` of one scenario is paired with
//!   replication `r` of the other, the paired differences remove the shared noise, and
//!   the `m` intervals are each built at `1 - (1 - level) / m` so that all hold together.

use serde::Serialize;

use crate::range::{ptukey, qtukey};
use crate::special::{f_sf, t_cdf, t_ppf};

/// One-way analysis of variance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Anova {
    /// Number of groups.
    pub groups: usize,
    /// Total number of observations.
    pub n: usize,
    pub means: Vec<f64>,
    pub sizes: Vec<usize>,
    pub grand_mean: f64,
    /// Between-group (treatment) sum of squares and degrees of freedom.
    pub ss_between: f64,
    pub df_between: usize,
    /// Within-group (error) sum of squares and degrees of freedom.
    pub ss_within: f64,
    pub df_within: usize,
    pub ms_between: f64,
    pub ms_within: f64,
    /// `ms_between / ms_within` (infinite when the groups have no spread at all).
    pub f: f64,
    /// `P(F > f)`.
    pub p_value: f64,
}

impl Anova {
    pub fn ss_total(&self) -> f64 {
        self.ss_between + self.ss_within
    }
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

/// One-way ANOVA of `groups`. `None` with fewer than two groups, an empty group or no
/// degree of freedom left for the error (every group with a single value).
pub fn anova(groups: &[&[f64]]) -> Option<Anova> {
    let k = groups.len();
    if k < 2 || groups.iter().any(|g| g.is_empty()) {
        return None;
    }
    let n: usize = groups.iter().map(|g| g.len()).sum();
    if n <= k {
        return None;
    }
    let means: Vec<f64> = groups.iter().map(|g| mean(g)).collect();
    let grand = groups.iter().flat_map(|g| g.iter()).sum::<f64>() / n as f64;
    let ss_between: f64 = groups
        .iter()
        .zip(&means)
        .map(|(g, m)| g.len() as f64 * (m - grand).powi(2))
        .sum();
    let ss_within: f64 = groups
        .iter()
        .zip(&means)
        .map(|(g, m)| g.iter().map(|x| (x - m).powi(2)).sum::<f64>())
        .sum();
    let (df_b, df_w) = (k - 1, n - k);
    let ms_b = ss_between / df_b as f64;
    let ms_w = ss_within / df_w as f64;
    let (f, p) = if ms_w > 0.0 {
        let f = ms_b / ms_w;
        (f, f_sf(f, df_b as f64, df_w as f64))
    } else if ms_b > 0.0 {
        (f64::INFINITY, 0.0)
    } else {
        (f64::NAN, f64::NAN)
    };
    Some(Anova {
        groups: k,
        n,
        sizes: groups.iter().map(|g| g.len()).collect(),
        means,
        grand_mean: grand,
        ss_between,
        df_between: df_b,
        ss_within,
        df_within: df_w,
        ms_between: ms_b,
        ms_within: ms_w,
        f,
        p_value: p,
    })
}

/// One pair of a multiple comparison: `mean_i - mean_j` with its simultaneous interval.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PairComparison {
    pub i: usize,
    pub j: usize,
    /// `mean_i - mean_j` (mean of the paired differences for Bonferroni).
    pub difference: f64,
    /// Half-width of the simultaneous interval.
    pub half_width: f64,
    /// Adjusted p-value (family-wise): from the studentized range for Tukey,
    /// `min(1, m · p)` for Bonferroni.
    pub p_value: f64,
    /// Whether the interval excludes zero.
    pub significant: bool,
}

impl PairComparison {
    pub fn low(&self) -> f64 {
        self.difference - self.half_width
    }
    pub fn high(&self) -> f64 {
        self.difference + self.half_width
    }
}

/// Tukey's honestly significant difference over the groups of an [`Anova`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Tukey {
    pub level: f64,
    /// Quantile `q(level; k, df_within)` of the studentized range.
    pub q_critical: f64,
    pub pairs: Vec<PairComparison>,
}

/// Every pair `i < j` in a fixed order: (0,1), (0,2), …, (1,2), …
fn pairs(k: usize) -> impl Iterator<Item = (usize, usize)> {
    (0..k).flat_map(move |i| (i + 1..k).map(move |j| (i, j)))
}

/// Tukey's HSD at confidence `level` (Tukey-Kramer when the group sizes differ):
/// for each pair, `|mean_i - mean_j| ± q · √(MS_within / 2 · (1/n_i + 1/n_j))`.
pub fn tukey(a: &Anova, level: f64) -> Tukey {
    let k = a.groups;
    let df = a.df_within as f64;
    let q = qtukey(level, k, df);
    let pairs = pairs(k)
        .map(|(i, j)| {
            let d = a.means[i] - a.means[j];
            let se =
                (a.ms_within / 2.0 * (1.0 / a.sizes[i] as f64 + 1.0 / a.sizes[j] as f64)).sqrt();
            let hw = q * se;
            let p = if se > 0.0 {
                1.0 - ptukey(d.abs() / se, k, df)
            } else if d != 0.0 {
                0.0
            } else {
                1.0
            };
            PairComparison {
                i,
                j,
                difference: d,
                half_width: hw,
                p_value: p.clamp(0.0, 1.0),
                significant: d.abs() > hw,
            }
        })
        .collect();
    Tukey {
        level,
        q_critical: q,
        pairs,
    }
}

/// Paired comparisons of every pair of scenarios with the Bonferroni correction.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Bonferroni {
    pub level: f64,
    /// Number of comparisons `m = k (k - 1) / 2`.
    pub comparisons: usize,
    /// Confidence of each single interval, `1 - (1 - level) / m`.
    pub level_each: f64,
    /// Replications paired in every comparison (the shortest group).
    pub n: usize,
    pub pairs: Vec<PairComparison>,
}

/// Bonferroni-corrected paired comparisons (common random numbers: value `r` of every
/// group comes from the same seeds). Only the first `min(len)` values are paired.
/// `None` with fewer than two groups or fewer than two pairs of values.
pub fn bonferroni_paired(groups: &[&[f64]], level: f64) -> Option<Bonferroni> {
    let k = groups.len();
    let n = groups.iter().map(|g| g.len()).min()?;
    if k < 2 || n < 2 || !(level > 0.0 && level < 1.0) {
        return None;
    }
    let m = k * (k - 1) / 2;
    let each = 1.0 - (1.0 - level) / m as f64;
    let df = (n - 1) as f64;
    let t = t_ppf(1.0 - (1.0 - each) / 2.0, df);
    let pairs = pairs(k)
        .map(|(i, j)| {
            let d: Vec<f64> = (0..n).map(|r| groups[i][r] - groups[j][r]).collect();
            let md = mean(&d);
            let sd = (d.iter().map(|x| (x - md).powi(2)).sum::<f64>() / df).sqrt();
            let se = sd / (n as f64).sqrt();
            let p = if se > 0.0 {
                2.0 * (1.0 - t_cdf((md / se).abs(), df))
            } else if md != 0.0 {
                0.0
            } else {
                1.0
            };
            let hw = t * se;
            PairComparison {
                i,
                j,
                difference: md,
                half_width: hw,
                p_value: (m as f64 * p).min(1.0),
                significant: md.abs() > hw,
            }
        })
        .collect();
    Some(Bonferroni {
        level,
        comparisons: m,
        level_each: each,
        n,
        pairs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    /// NIST/SEMATECH e-Handbook of Statistical Methods, section 7.4.3.3 (one-way ANOVA
    /// example) and 7.4.7.1 (Tukey's method on the same data). [conferir]
    const NIST: [[f64; 5]; 3] = [
        [6.9, 5.4, 5.8, 4.6, 4.0],
        [8.3, 6.8, 7.8, 9.2, 6.5],
        [8.0, 10.5, 8.1, 6.9, 9.3],
    ];

    #[test]
    fn anova_of_the_nist_example() {
        let g: Vec<&[f64]> = NIST.iter().map(|g| g.as_slice()).collect();
        let a = anova(&g).unwrap();
        // Published table: treatments SS 27.897 (2 df, MS 13.949), error SS 17.452
        // (12 df, MS 1.454), total SS 45.349 (14 df), F = 9.59.
        close(a.ss_between, 27.897, 5e-4);
        close(a.ss_within, 17.452, 5e-4);
        close(a.ss_total(), 45.349, 5e-4);
        assert_eq!((a.df_between, a.df_within, a.n), (2, 12, 15));
        close(a.ms_between, 13.949, 5e-4);
        close(a.ms_within, 1.454, 5e-4);
        close(a.f, 9.59, 5e-3);
        // F(2, 12) = 9.59 lies between the 1% (6.93) and 0.5% (8.51) points: p ≈ 0.003.
        assert!(a.p_value > 0.001 && a.p_value < 0.005, "{}", a.p_value);
        close(a.means[0], 5.34, 1e-12);
    }

    #[test]
    fn tukey_of_the_nist_example() {
        let g: Vec<&[f64]> = NIST.iter().map(|g| g.as_slice()).collect();
        let a = anova(&g).unwrap();
        let t = tukey(&a, 0.95);
        // q(0.05; 3, 12) = 3.77 in the handbook; half-width 3.77·√(1.454/5) = 2.03.
        close(t.q_critical, 3.77, 0.005);
        let p01 = &t.pairs[0];
        assert_eq!((p01.i, p01.j), (0, 1));
        close(p01.difference, -2.38, 1e-12);
        close(p01.half_width, 2.033, 0.005);
        // Published conclusion: 1 differs from 2 and from 3; 2 and 3 do not differ.
        assert!(t.pairs[0].significant && t.pairs[1].significant);
        assert!(!t.pairs[2].significant);
        assert!(t.pairs[2].p_value > 0.05 && t.pairs[1].p_value < 0.01);
    }

    /// Montgomery, *Design and Analysis of Experiments*, example 3.1 (etch rate against
    /// RF power, four levels, five replicates). [conferir]
    #[test]
    fn anova_and_tukey_of_the_etch_rate_example() {
        let g: [&[f64]; 4] = [
            &[575.0, 542.0, 530.0, 539.0, 570.0],
            &[565.0, 593.0, 590.0, 579.0, 610.0],
            &[600.0, 651.0, 610.0, 637.0, 629.0],
            &[725.0, 700.0, 715.0, 685.0, 710.0],
        ];
        let a = anova(&g).unwrap();
        // Published: SS treatments 66870.55, SS error 5339.20, F0 = 66.80.
        close(a.ss_between, 66870.55, 0.01);
        close(a.ss_within, 5339.20, 0.01);
        close(a.f, 66.80, 0.005);
        assert!(a.p_value < 1e-8);
        let t = tukey(&a, 0.95);
        // q(0.05; 4, 16) = 4.05 and T = 4.05·√(333.70/5) = 33.09: every pair differs.
        close(t.q_critical, 4.05, 0.005);
        close(t.pairs[0].half_width, 33.09, 0.05);
        assert!(t.pairs.iter().all(|p| p.significant));
    }

    #[test]
    fn tukey_kramer_for_unequal_sizes() {
        let g: [&[f64]; 3] = [&[1.0, 2.0, 3.0], &[2.0, 3.0, 4.0, 5.0], &[9.0, 10.0]];
        let a = anova(&g).unwrap();
        assert_eq!((a.df_between, a.df_within), (2, 6));
        // Pooled variance: (2 + 5 + 0.5) / 6 = 1.25; pair (0, 1): √(1.25/2 · (1/3 + 1/4)).
        close(a.ms_within, 1.25, 1e-12);
        let t = tukey(&a, 0.95);
        let se = (1.25f64 / 2.0 * (1.0 / 3.0 + 0.25)).sqrt();
        close(t.pairs[0].half_width, t.q_critical * se, 1e-12);
        assert!(!t.pairs[0].significant && t.pairs[1].significant);
    }

    #[test]
    fn degenerate_groups() {
        assert!(anova(&[&[1.0, 2.0]]).is_none());
        assert!(anova(&[&[1.0], &[2.0]]).is_none());
        assert!(anova(&[&[1.0, 2.0], &[]]).is_none());
        // No spread inside, different means: infinitely significant.
        let a = anova(&[&[1.0, 1.0], &[2.0, 2.0]]).unwrap();
        assert!(a.f.is_infinite() && a.p_value == 0.0);
    }

    /// Paired t test of the sleep data of Student (1908) / Cushny and Peebles (1905):
    /// extra hours of sleep with two drugs on the same ten patients. Published:
    /// t = 4.062 with 9 df, p = 0.00283, 95% interval 0.700 to 2.460. [conferir]
    #[test]
    fn single_paired_comparison_is_the_paired_t_test() {
        let drug1 = [0.7, -1.6, -0.2, -1.2, -0.1, 3.4, 3.7, 0.8, 0.0, 2.0];
        let drug2 = [1.9, 0.8, 1.1, 0.1, -0.1, 4.4, 5.5, 1.6, 4.6, 3.4];
        let b = bonferroni_paired(&[&drug2, &drug1], 0.95).unwrap();
        assert_eq!((b.comparisons, b.n), (1, 10));
        close(b.level_each, 0.95, 1e-15);
        let p = &b.pairs[0];
        close(p.difference, 1.58, 1e-12);
        close(p.low(), 0.700, 5e-4);
        close(p.high(), 2.460, 5e-4);
        close(p.p_value, 0.002833, 5e-6);
        assert!(p.significant);
    }

    #[test]
    fn bonferroni_widens_with_more_comparisons() {
        let a = [10.0, 11.0, 12.0, 11.5, 10.5, 11.2];
        let b = [10.4, 11.6, 12.3, 12.1, 10.9, 11.8];
        let c = [12.0, 13.1, 14.2, 13.4, 12.6, 13.0];
        let two = bonferroni_paired(&[&a, &b], 0.95).unwrap();
        let three = bonferroni_paired(&[&a, &b, &c], 0.95).unwrap();
        assert_eq!(three.comparisons, 3);
        close(three.level_each, 1.0 - 0.05 / 3.0, 1e-15);
        // Same pair (0, 1): same mean difference, wider interval, p-value times 3.
        close(two.pairs[0].difference, three.pairs[0].difference, 1e-15);
        assert!(three.pairs[0].half_width > two.pairs[0].half_width);
        close(
            three.pairs[0].p_value,
            (3.0 * two.pairs[0].p_value).min(1.0),
            1e-12,
        );
        assert!(three.pairs[1].significant && three.pairs[2].significant);
        assert!(bonferroni_paired(&[&a], 0.95).is_none());
        assert!(bonferroni_paired(&[&[1.0], &[2.0]], 0.95).is_none());
    }
}
