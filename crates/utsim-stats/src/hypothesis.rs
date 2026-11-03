//! Hypothesis tests of the Analyse phase: Student t (one sample, two samples by Welch
//! or pooled, paired), one-way ANOVA with Tukey, χ² of independence and of goodness of
//! fit, Fisher's exact test of a 2 × 2 table, and tests of one and two proportions.
//!
//! Every test gives the three p-values (two-sided, `H₁: less`, `H₁: greater`), the one
//! of the chosen [`Alternative`] in `p_value`, and an interval at `level` on the same
//! side (one-sided intervals have an infinite or 0/1 limit).
//!
//! Formulas (Montgomery and Runger, *Applied Statistics and Probability for Engineers*,
//! chapters 8 to 10; Agresti, *An Introduction to Categorical Data Analysis*, chapter 2):
//! - one sample: `t = (x̄ − μ₀)/(s/√n)`, `n − 1` df;
//! - two samples, Welch (default): `t = (x̄₁ − x̄₂ − Δ₀)/√(s₁²/n₁ + s₂²/n₂)` with the
//!   Satterthwaite df `ν = (v₁ + v₂)² / (v₁²/(n₁−1) + v₂²/(n₂−1))`, `vᵢ = sᵢ²/nᵢ`;
//!   pooled: `s_p² = ((n₁−1)s₁² + (n₂−1)s₂²)/(n₁+n₂−2)`, `t = (x̄₁ − x̄₂ − Δ₀)/(s_p
//!   √(1/n₁ + 1/n₂))`, `n₁ + n₂ − 2` df;
//! - paired: the one-sample test on the differences `xᵢ − yᵢ`;
//! - effect size (Cohen's d): `(estimate − hypothesized)/s` with `s` the sample sd
//!   (one sample), the sd of the differences (paired, `d_z`) or the pooled sd (two
//!   samples, whichever variance assumption the test uses);
//! - χ² of independence: `X² = Σ (O − E)²/E`, `E = rᵢ cⱼ / N`, `(r−1)(c−1)` df, with the
//!   likelihood-ratio `G² = 2 Σ O ln(O/E)`, Pearson residuals `(O − E)/√E` and adjusted
//!   residuals `(O − E)/√(E (1 − rᵢ/N)(1 − cⱼ/N))`; the approximation is flagged when
//!   more than 20% of the expected counts are below 5 or any is below 1 (Cochran's rule);
//! - proportions: exact binomial (two-sided p-value: the sum of the probabilities not
//!   above that of the observed count) with the Clopper-Pearson interval, or the normal
//!   approximation `z = (p̂ − p₀)/√(p₀(1−p₀)/n)` with the Wald interval; the Wilson
//!   interval is given in both; two proportions: `z` with the pooled proportion and the
//!   Wald interval of the difference; Fisher: hypergeometric enumeration.

use serde::Serialize;

use crate::compare::{Anova, Tukey, anova, tukey};
use crate::descriptive::{Interval, mean, variance};
use crate::special::{beta_ppf, chi2_sf, ln_choose, norm_cdf, norm_ppf, t_cdf, t_ppf};

/// Alternative hypothesis, about the parameter (mean, difference, proportion) against
/// its hypothesized value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Alternative {
    /// `H₁: θ ≠ θ₀`.
    #[default]
    TwoSided,
    /// `H₁: θ < θ₀`.
    Less,
    /// `H₁: θ > θ₀`.
    Greater,
}

/// The three p-values of a test.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PValues {
    pub two_sided: f64,
    /// For `H₁: θ < θ₀`.
    pub less: f64,
    /// For `H₁: θ > θ₀`.
    pub greater: f64,
}

impl PValues {
    pub fn get(&self, alternative: Alternative) -> f64 {
        match alternative {
            Alternative::TwoSided => self.two_sided,
            Alternative::Less => self.less,
            Alternative::Greater => self.greater,
        }
    }

    /// From a statistic whose CDF under `H₀` is `cdf` and that is symmetric about 0.
    fn symmetric(stat: f64, cdf: impl Fn(f64) -> f64) -> PValues {
        if stat.is_nan() {
            return PValues {
                two_sided: f64::NAN,
                less: f64::NAN,
                greater: f64::NAN,
            };
        }
        let less = cdf(stat);
        let greater = cdf(-stat);
        PValues {
            two_sided: (2.0 * less.min(greater)).min(1.0),
            less,
            greater,
        }
    }

    fn of_t(t: f64, df: f64) -> PValues {
        PValues::symmetric(t, |x| t_cdf(x, df))
    }

    fn of_z(z: f64) -> PValues {
        PValues::symmetric(z, norm_cdf)
    }
}

/// Interval for an estimate with standard error `se` and quantile function `q(p)` of
/// the pivot, on the side of `alternative`.
fn interval(
    estimate: f64,
    se: f64,
    level: f64,
    alternative: Alternative,
    q: impl Fn(f64) -> f64,
) -> Interval {
    let alpha = 1.0 - level;
    let (low, high) = match alternative {
        Alternative::TwoSided => {
            let hw = q(1.0 - alpha / 2.0) * se;
            (estimate - hw, estimate + hw)
        }
        Alternative::Less => (f64::NEG_INFINITY, estimate + q(1.0 - alpha) * se),
        Alternative::Greater => (estimate - q(1.0 - alpha) * se, f64::INFINITY),
    };
    Interval { low, high, level }
}

/// Which t test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TTestKind {
    OneSample,
    /// Two independent samples, unequal variances (Satterthwaite df).
    Welch,
    /// Two independent samples, equal variances (pooled sd).
    Pooled,
    /// Paired samples (one-sample test on the differences).
    Paired,
}

/// Result of a t test.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TTest {
    pub kind: TTestKind,
    /// Sizes, means and sds of the samples (one entry for one sample and for the
    /// differences of a paired test, two for independent samples).
    pub sizes: Vec<usize>,
    pub means: Vec<f64>,
    pub sds: Vec<f64>,
    /// Mean, or difference of means `x̄₁ − x̄₂`, or mean difference.
    pub estimate: f64,
    /// `μ₀` or `Δ₀`.
    pub hypothesized: f64,
    pub se: f64,
    pub t: f64,
    pub df: f64,
    pub alternative: Alternative,
    /// The p-value of `alternative`.
    pub p_value: f64,
    pub p: PValues,
    /// Interval of the mean or difference at `level`, on the side of `alternative`.
    pub ci: Interval,
    /// Cohen's d (see the module notes).
    pub effect_size: f64,
}

// The pieces of every t test, gathered in one place (hence the many arguments).
#[allow(clippy::too_many_arguments)]
fn finish(
    kind: TTestKind,
    stats: (Vec<usize>, Vec<f64>, Vec<f64>),
    estimate: f64,
    hypothesized: f64,
    se: f64,
    df: f64,
    sd_effect: f64,
    level: f64,
    alternative: Alternative,
) -> TTest {
    let t = if se > 0.0 {
        (estimate - hypothesized) / se
    } else if estimate != hypothesized {
        (estimate - hypothesized).signum() * f64::INFINITY
    } else {
        f64::NAN
    };
    let p = PValues::of_t(t, df);
    TTest {
        kind,
        sizes: stats.0,
        means: stats.1,
        sds: stats.2,
        estimate,
        hypothesized,
        se,
        t,
        df,
        alternative,
        p_value: p.get(alternative),
        p,
        ci: interval(estimate, se, level, alternative, |pr| t_ppf(pr, df)),
        effect_size: (estimate - hypothesized) / sd_effect,
    }
}

fn clean(xs: &[f64]) -> Vec<f64> {
    xs.iter().copied().filter(|x| !x.is_nan()).collect()
}

/// One-sample t test of `H₀: μ = mu0` (NaN dropped). `None` with fewer than two values.
pub fn t_one_sample(xs: &[f64], mu0: f64, level: f64, alternative: Alternative) -> Option<TTest> {
    let xs = clean(xs);
    let n = xs.len();
    if n < 2 {
        return None;
    }
    let (m, sd) = (mean(&xs), variance(&xs).sqrt());
    Some(finish(
        TTestKind::OneSample,
        (vec![n], vec![m], vec![sd]),
        m,
        mu0,
        sd / (n as f64).sqrt(),
        (n - 1) as f64,
        sd,
        level,
        alternative,
    ))
}

/// Two-sample t test of `H₀: μ₁ − μ₂ = delta0`, by Welch (`pooled = false`, the default
/// choice) or with the pooled variance. `None` when a sample has fewer than two values.
pub fn t_two_sample(
    x: &[f64],
    y: &[f64],
    delta0: f64,
    pooled: bool,
    level: f64,
    alternative: Alternative,
) -> Option<TTest> {
    let (x, y) = (clean(x), clean(y));
    let (n1, n2) = (x.len(), y.len());
    if n1 < 2 || n2 < 2 {
        return None;
    }
    let (m1, m2) = (mean(&x), mean(&y));
    let (v1, v2) = (variance(&x), variance(&y));
    let (f1, f2) = (n1 as f64, n2 as f64);
    let sp2 = ((f1 - 1.0) * v1 + (f2 - 1.0) * v2) / (f1 + f2 - 2.0);
    let (kind, se, df) = if pooled {
        (
            TTestKind::Pooled,
            (sp2 * (1.0 / f1 + 1.0 / f2)).sqrt(),
            f1 + f2 - 2.0,
        )
    } else {
        let (a, b) = (v1 / f1, v2 / f2);
        let df = if a + b > 0.0 {
            (a + b).powi(2) / (a * a / (f1 - 1.0) + b * b / (f2 - 1.0))
        } else {
            f1 + f2 - 2.0
        };
        (TTestKind::Welch, (a + b).sqrt(), df)
    };
    Some(finish(
        kind,
        (vec![n1, n2], vec![m1, m2], vec![v1.sqrt(), v2.sqrt()]),
        m1 - m2,
        delta0,
        se,
        df,
        sp2.sqrt(),
        level,
        alternative,
    ))
}

/// Paired t test of `H₀: μ_d = delta0` on `xᵢ − yᵢ` (pairs with a NaN dropped; the
/// shorter length is used). `None` with fewer than two pairs.
pub fn t_paired(
    x: &[f64],
    y: &[f64],
    delta0: f64,
    level: f64,
    alternative: Alternative,
) -> Option<TTest> {
    let d = differences(x, y);
    let mut t = t_one_sample(&d, delta0, level, alternative)?;
    t.kind = TTestKind::Paired;
    Some(t)
}

/// `xᵢ − yᵢ` over the common length, pairs with a NaN dropped.
pub fn differences(x: &[f64], y: &[f64]) -> Vec<f64> {
    x.iter()
        .zip(y)
        .map(|(a, b)| a - b)
        .filter(|d| !d.is_nan())
        .collect()
}

/// One-way ANOVA with Tukey's comparisons (both from [`crate::compare`]) and the fit
/// summaries of the NIST StRD ANOVA certificates.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OneWay {
    pub anova: Anova,
    pub tukey: Tukey,
    /// `SS_between / SS_total`.
    pub r_squared: f64,
    /// `√MS_within`.
    pub residual_sd: f64,
}

/// One-way ANOVA of `groups` and Tukey's HSD at `level`. `None` as for
/// [`crate::compare::anova`].
pub fn one_way_anova(groups: &[&[f64]], level: f64) -> Option<OneWay> {
    let a = anova(groups)?;
    let t = tukey(&a, level);
    Some(OneWay {
        r_squared: a.ss_between / a.ss_total(),
        residual_sd: a.ms_within.sqrt(),
        tukey: t,
        anova: a,
    })
}

/// χ² test of independence of a contingency table.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChiSquareTable {
    pub observed: Vec<Vec<f64>>,
    pub expected: Vec<Vec<f64>>,
    pub row_totals: Vec<f64>,
    pub col_totals: Vec<f64>,
    pub total: f64,
    /// Pearson's `X²`.
    pub statistic: f64,
    pub df: usize,
    pub p_value: f64,
    /// Likelihood ratio `G²` and its p-value (same df).
    pub g_statistic: f64,
    pub g_p_value: f64,
    /// `(O − E)/√E`.
    pub residuals: Vec<Vec<f64>>,
    /// `(O − E)/√(E (1 − rᵢ/N)(1 − cⱼ/N))`, standard normal under independence.
    pub adjusted_residuals: Vec<Vec<f64>>,
    /// Cells with an expected count below 5, and the smallest expected count.
    pub cells_below_5: usize,
    pub min_expected: f64,
    /// More than 20% of the expected counts below 5 or any below 1: the χ²
    /// approximation is doubtful (merge categories, or use Fisher's test in 2 × 2).
    pub low_expected_warning: bool,
}

/// χ² test of independence of `table` (rows of counts, all rows the same length).
/// `None` for fewer than two rows or columns, ragged rows, negative counts, or a row
/// or column with zero total.
pub fn chi_square_independence(table: &[Vec<f64>]) -> Option<ChiSquareTable> {
    let r = table.len();
    let c = table.first()?.len();
    if r < 2 || c < 2 || table.iter().any(|row| row.len() != c) {
        return None;
    }
    if table.iter().flatten().any(|&v| v.is_nan() || v < 0.0) {
        return None;
    }
    let row_totals: Vec<f64> = table.iter().map(|row| row.iter().sum()).collect();
    let col_totals: Vec<f64> = (0..c)
        .map(|j| table.iter().map(|row| row[j]).sum())
        .collect();
    if row_totals.iter().chain(&col_totals).any(|&t| t <= 0.0) {
        return None;
    }
    let total: f64 = row_totals.iter().sum();
    let expected: Vec<Vec<f64>> = row_totals
        .iter()
        .map(|ri| col_totals.iter().map(|cj| ri * cj / total).collect())
        .collect();
    let (mut x2, mut g2) = (0.0, 0.0);
    let mut residuals = vec![vec![0.0; c]; r];
    let mut adjusted = vec![vec![0.0; c]; r];
    for i in 0..r {
        for j in 0..c {
            let (o, e) = (table[i][j], expected[i][j]);
            x2 += (o - e).powi(2) / e;
            if o > 0.0 {
                g2 += 2.0 * o * (o / e).ln();
            }
            residuals[i][j] = (o - e) / e.sqrt();
            let v = e * (1.0 - row_totals[i] / total) * (1.0 - col_totals[j] / total);
            adjusted[i][j] = (o - e) / v.sqrt();
        }
    }
    let df = (r - 1) * (c - 1);
    let flat = || expected.iter().flatten().copied();
    let cells_below_5 = flat().filter(|&e| e < 5.0).count();
    let min_expected = flat().fold(f64::INFINITY, f64::min);
    Some(ChiSquareTable {
        observed: table.to_vec(),
        statistic: x2,
        df,
        p_value: chi2_sf(x2, df as f64),
        g_statistic: g2,
        g_p_value: chi2_sf(g2, df as f64),
        residuals,
        adjusted_residuals: adjusted,
        cells_below_5,
        min_expected,
        low_expected_warning: cells_below_5 as f64 > 0.2 * (r * c) as f64 || min_expected < 1.0,
        expected,
        row_totals,
        col_totals,
        total,
    })
}

/// χ² goodness-of-fit test.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChiSquareFit {
    pub observed: Vec<f64>,
    pub expected: Vec<f64>,
    pub statistic: f64,
    /// `k − 1 − estimated parameters`.
    pub df: usize,
    pub p_value: f64,
    /// `(O − E)/√E` per category.
    pub residuals: Vec<f64>,
    pub cells_below_5: usize,
    pub min_expected: f64,
    pub low_expected_warning: bool,
}

/// χ² goodness of fit of the counts `observed` to the category `proportions` (any
/// positive weights, normalised; equal when `None`), with `estimated` parameters taken
/// from the same data. `None` for fewer than two categories, mismatched lengths, a
/// non-positive weight, negative counts or no degree of freedom left.
pub fn chi_square_goodness_of_fit(
    observed: &[f64],
    proportions: Option<&[f64]>,
    estimated: usize,
) -> Option<ChiSquareFit> {
    let k = observed.len();
    if k < 2 || observed.iter().any(|&o| o.is_nan() || o < 0.0) || k <= 1 + estimated {
        return None;
    }
    let weights: Vec<f64> = match proportions {
        Some(p) if p.len() == k && p.iter().all(|&w| w > 0.0) => p.to_vec(),
        Some(_) => return None,
        None => vec![1.0; k],
    };
    let wsum: f64 = weights.iter().sum();
    let n: f64 = observed.iter().sum();
    if n <= 0.0 {
        return None;
    }
    let expected: Vec<f64> = weights.iter().map(|w| n * w / wsum).collect();
    let residuals: Vec<f64> = observed
        .iter()
        .zip(&expected)
        .map(|(o, e)| (o - e) / e.sqrt())
        .collect();
    let statistic = residuals.iter().map(|r| r * r).sum();
    let df = k - 1 - estimated;
    let cells_below_5 = expected.iter().filter(|&&e| e < 5.0).count();
    let min_expected = expected.iter().copied().fold(f64::INFINITY, f64::min);
    Some(ChiSquareFit {
        observed: observed.to_vec(),
        statistic,
        df,
        p_value: chi2_sf(statistic, df as f64),
        residuals,
        cells_below_5,
        min_expected,
        low_expected_warning: cells_below_5 as f64 > 0.2 * k as f64 || min_expected < 1.0,
        expected,
    })
}

/// Fisher's exact test of a 2 × 2 table `[[a, b], [c, d]]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Fisher {
    pub table: [[u64; 2]; 2],
    /// Sample odds ratio `a d / (b c)` (infinite or NaN with zero cells).
    pub odds_ratio: f64,
    /// `less` and `greater` are about the odds ratio (`a` small or large given the
    /// margins); two-sided sums the tables no more probable than the observed one.
    pub p: PValues,
}

/// Fisher's exact test by enumeration of the hypergeometric distribution of `a` given
/// the margins. `None` for an empty table.
pub fn fisher_exact(table: [[u64; 2]; 2]) -> Option<Fisher> {
    let [[a, b], [c, d]] = table;
    let (r1, r2, c1) = (a + b, c + d, a + c);
    let n = r1 + r2;
    if n == 0 {
        return None;
    }
    let lo = c1.saturating_sub(r2);
    let hi = r1.min(c1);
    let ln_den = ln_choose(n as f64, c1 as f64);
    let pmf = |k: u64| {
        (ln_choose(r1 as f64, k as f64) + ln_choose(r2 as f64, (c1 - k) as f64) - ln_den).exp()
    };
    let probs: Vec<(u64, f64)> = (lo..=hi).map(|k| (k, pmf(k))).collect();
    let observed = pmf(a);
    let sum = |f: &dyn Fn(u64, f64) -> bool| -> f64 {
        probs
            .iter()
            .filter(|(k, p)| f(*k, *p))
            .map(|(_, p)| p)
            .sum::<f64>()
            .min(1.0)
    };
    let p = PValues {
        less: sum(&|k, _| k <= a),
        greater: sum(&|k, _| k >= a),
        two_sided: sum(&|_, p| p <= observed * (1.0 + 1e-7)),
    };
    let (num, den) = ((a * d) as f64, (b * c) as f64);
    Some(Fisher {
        table,
        odds_ratio: if den > 0.0 {
            num / den
        } else if num > 0.0 {
            f64::INFINITY
        } else {
            f64::NAN
        },
        p,
    })
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
    let hi = if x >= n {
        1.0
    } else {
        beta_ppf(1.0 - a, xf + 1.0, nf - xf)
    };
    (lo, hi)
}

/// Wilson score interval for `x` successes in `n` trials at `level`.
pub fn wilson_interval(x: usize, n: usize, level: f64) -> (f64, f64) {
    if n == 0 {
        return (f64::NAN, f64::NAN);
    }
    let z = norm_ppf(1.0 - (1.0 - level) / 2.0);
    let nf = n as f64;
    let p = x as f64 / nf;
    let z2 = z * z;
    let centre = (p + z2 / (2.0 * nf)) / (1.0 + z2 / nf);
    let hw = z * (p * (1.0 - p) / nf + z2 / (4.0 * nf * nf)).sqrt() / (1.0 + z2 / nf);
    ((centre - hw).max(0.0), (centre + hw).min(1.0))
}

/// Binomial probability `P(X = k)`, `X ~ Binomial(n, p)`.
pub fn binomial_pmf(k: u64, n: u64, p: f64) -> f64 {
    if k > n {
        return 0.0;
    }
    if p <= 0.0 {
        return if k == 0 { 1.0 } else { 0.0 };
    }
    if p >= 1.0 {
        return if k == n { 1.0 } else { 0.0 };
    }
    let (k, n) = (k as f64, n as f64);
    (ln_choose(n, k) + k * p.ln() + (n - k) * (-p).ln_1p()).exp()
}

/// How a proportion is tested.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum ProportionMethod {
    /// Exact binomial test, Clopper-Pearson interval.
    #[default]
    Exact,
    /// Normal approximation (score statistic), Wald interval.
    Normal,
}

/// Test of one proportion.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ProportionTest {
    pub x: u64,
    pub n: u64,
    pub p_hat: f64,
    pub p0: f64,
    pub method: ProportionMethod,
    /// `z` of the normal approximation (also given for the exact method).
    pub z: f64,
    pub alternative: Alternative,
    pub p_value: f64,
    pub p: PValues,
    /// Clopper-Pearson (exact) or Wald (normal) interval, on the side of `alternative`.
    pub ci: Interval,
    /// Two-sided Wilson interval at the same level.
    pub wilson: Interval,
}

/// Test of `H₀: p = p0` for `x` successes in `n` trials. `None` for `n = 0`, `x > n`
/// or `p0` outside `(0, 1)`.
pub fn one_proportion(
    x: u64,
    n: u64,
    p0: f64,
    method: ProportionMethod,
    level: f64,
    alternative: Alternative,
) -> Option<ProportionTest> {
    if n == 0 || x > n || !(p0 > 0.0 && p0 < 1.0) {
        return None;
    }
    let nf = n as f64;
    let p_hat = x as f64 / nf;
    let z = (p_hat - p0) / (p0 * (1.0 - p0) / nf).sqrt();
    let (p, ci) = match method {
        ProportionMethod::Normal => {
            let se = (p_hat * (1.0 - p_hat) / nf).sqrt();
            let mut ci = interval(p_hat, se, level, alternative, norm_ppf);
            ci.low = ci.low.max(0.0);
            ci.high = ci.high.min(1.0);
            (PValues::of_z(z), ci)
        }
        ProportionMethod::Exact => {
            let probs: Vec<f64> = (0..=n).map(|k| binomial_pmf(k, n, p0)).collect();
            let observed = probs[x as usize];
            let less: f64 = probs[..=x as usize].iter().sum();
            let greater: f64 = probs[x as usize..].iter().sum();
            let two: f64 = probs
                .iter()
                .filter(|&&q| q <= observed * (1.0 + 1e-7))
                .sum();
            let p = PValues {
                two_sided: two.min(1.0),
                less: less.min(1.0),
                greater: greater.min(1.0),
            };
            let (lo, hi) = match alternative {
                Alternative::TwoSided => clopper_pearson(x as usize, n as usize, level),
                // A one-sided bound at `level` is the two-sided bound at `2 level − 1`.
                Alternative::Less => (
                    0.0,
                    clopper_pearson(x as usize, n as usize, 2.0 * level - 1.0).1,
                ),
                Alternative::Greater => (
                    clopper_pearson(x as usize, n as usize, 2.0 * level - 1.0).0,
                    1.0,
                ),
            };
            (
                p,
                Interval {
                    low: lo,
                    high: hi,
                    level,
                },
            )
        }
    };
    let (wl, wh) = wilson_interval(x as usize, n as usize, level);
    Some(ProportionTest {
        x,
        n,
        p_hat,
        p0,
        method,
        z,
        alternative,
        p_value: p.get(alternative),
        p,
        ci,
        wilson: Interval {
            low: wl,
            high: wh,
            level,
        },
    })
}

/// Test of two proportions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TwoProportions {
    pub x: [u64; 2],
    pub n: [u64; 2],
    pub p_hat: [f64; 2],
    /// Pooled proportion `(x₁ + x₂)/(n₁ + n₂)`.
    pub pooled: f64,
    /// `p̂₁ − p̂₂`.
    pub difference: f64,
    /// `(p̂₁ − p̂₂)/√(p̂ (1 − p̂)(1/n₁ + 1/n₂))`.
    pub z: f64,
    pub alternative: Alternative,
    pub p_value: f64,
    pub p: PValues,
    /// Wald interval of the difference, `√(p̂₁(1−p̂₁)/n₁ + p̂₂(1−p̂₂)/n₂)`.
    pub ci: Interval,
}

/// Normal-approximation test of `H₀: p₁ = p₂`. `None` for an empty sample or
/// `x > n`. For small counts use [`fisher_exact`].
pub fn two_proportions(
    x1: u64,
    n1: u64,
    x2: u64,
    n2: u64,
    level: f64,
    alternative: Alternative,
) -> Option<TwoProportions> {
    if n1 == 0 || n2 == 0 || x1 > n1 || x2 > n2 {
        return None;
    }
    let (f1, f2) = (n1 as f64, n2 as f64);
    let (p1, p2) = (x1 as f64 / f1, x2 as f64 / f2);
    let pooled = (x1 + x2) as f64 / (f1 + f2);
    let d = p1 - p2;
    let se0 = (pooled * (1.0 - pooled) * (1.0 / f1 + 1.0 / f2)).sqrt();
    let z = if se0 > 0.0 { d / se0 } else { f64::NAN };
    let p = PValues::of_z(z);
    let se = (p1 * (1.0 - p1) / f1 + p2 * (1.0 - p2) / f2).sqrt();
    Some(TwoProportions {
        x: [x1, x2],
        n: [n1, n2],
        p_hat: [p1, p2],
        pooled,
        difference: d,
        z,
        alternative,
        p_value: p.get(alternative),
        p,
        ci: interval(d, se, level, alternative, norm_ppf),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol * (1.0 + b.abs()), "{a} vs {b}");
    }

    /// Montgomery and Runger, *Applied Statistics and Probability for Engineers*
    /// (example of the coefficient of restitution of golf club drivers, one-sample t):
    /// n = 15, x̄ = 0.83725, s = 0.02456, μ₀ = 0.82, t₀ = 2.72 with 14 df, upper-tail
    /// P between 0.005 and 0.01.
    #[test]
    fn one_sample_t_golf_clubs() {
        let xs = [
            0.8411, 0.8191, 0.8182, 0.8125, 0.8750, 0.8580, 0.8532, 0.8483, 0.8276, 0.7983, 0.8042,
            0.8730, 0.8282, 0.8359, 0.8660,
        ];
        let t = t_one_sample(&xs, 0.82, 0.95, Alternative::Greater).unwrap();
        close(t.estimate, 0.83725, 2e-5);
        close(t.sds[0], 0.02456, 2e-4);
        close(t.t, 2.72, 2e-3);
        assert_eq!(t.df, 14.0);
        assert!(t.p_value > 0.005 && t.p_value < 0.01, "{}", t.p_value);
        // The p-values agree with t_cdf and with each other.
        close(t.p.greater, 1.0 - t_cdf(t.t, 14.0), 1e-14);
        close(t.p.two_sided, 2.0 * t.p.greater, 1e-14);
        close(t.p.less + t.p.greater, 1.0, 1e-14);
        // One-sided lower bound: x̄ − t(0.95; 14)·s/√n, t(0.95; 14) = 1.761 from the table.
        assert!(t.ci.high.is_infinite());
        close(t.ci.low, t.estimate - 1.761 * t.se, 1e-4);
        close(t.effect_size, (t.estimate - 0.82) / t.sds[0], 1e-15);
    }

    /// Montgomery and Runger (example of the catalyst yields, pooled two-sample t):
    /// x̄₁ = 92.255, s₁ = 2.39, x̄₂ = 92.733, s₂ = 2.98, s_p = 2.70, t₀ = −0.35 with
    /// 14 df, P > 0.7 (two-sided).
    #[test]
    fn pooled_t_catalyst_yields() {
        let c1 = [91.50, 94.18, 92.18, 95.39, 91.79, 89.07, 94.72, 89.21];
        let c2 = [89.19, 90.95, 90.46, 93.21, 97.19, 97.04, 91.07, 92.75];
        let t = t_two_sample(&c1, &c2, 0.0, true, 0.95, Alternative::TwoSided).unwrap();
        assert_eq!(t.kind, TTestKind::Pooled);
        close(t.means[0], 92.255, 1e-12);
        close(t.means[1], 92.7325, 1e-12);
        close(t.sds[0], 2.39, 2e-3);
        close(t.sds[1], 2.98, 2e-3);
        close(t.effect_size * 2.70, t.estimate, 2e-3);
        close(t.t, -0.35, 2e-2);
        assert_eq!(t.df, 14.0);
        assert!(t.p_value > 0.7 && t.p_value < 0.75, "{}", t.p_value);
        // The interval contains 0 exactly when the test does not reject.
        assert!(t.ci.low < 0.0 && t.ci.high > 0.0);
    }

    /// Montgomery and Runger (example of arsenic in drinking water, metropolitan Phoenix
    /// against rural Arizona, unequal variances): x̄₁ = 12.5, s₁ = 7.63, x̄₂ = 27.5,
    /// s₂ = 15.3, t₀ = −2.77, ν = 13.2 (the book rounds down to 13), H₀ rejected at 5%.
    #[test]
    fn welch_t_arsenic() {
        let metro = [3.0, 7.0, 25.0, 10.0, 15.0, 6.0, 12.0, 25.0, 15.0, 7.0];
        let rural = [48.0, 44.0, 40.0, 38.0, 33.0, 21.0, 20.0, 12.0, 1.0, 18.0];
        let t = t_two_sample(&metro, &rural, 0.0, false, 0.95, Alternative::TwoSided).unwrap();
        assert_eq!(t.kind, TTestKind::Welch);
        close(t.estimate, -15.0, 1e-13);
        close(t.sds[0], 7.63, 1e-3);
        close(t.sds[1], 15.3, 5e-3);
        close(t.t, -2.77, 2e-3);
        close(t.df, 13.2, 5e-3);
        assert!(t.p_value < 0.05 && t.p_value > 0.01);
        assert!(t.ci.high < 0.0);
        // Welch's se by hand.
        let se = ((7.633_988_f64.powi(2) + 15.349_629_f64.powi(2)) / 10.0).sqrt();
        close(t.se, se, 1e-6);
    }

    /// Student (1908), sleep data of Cushny and Peebles: paired t = 4.062 with 9 df,
    /// two-sided p = 0.00283, 95% interval 0.700 to 2.460.
    #[test]
    fn paired_t_is_the_one_sample_test_on_differences() {
        let drug1 = [0.7, -1.6, -0.2, -1.2, -0.1, 3.4, 3.7, 0.8, 0.0, 2.0];
        let drug2 = [1.9, 0.8, 1.1, 0.1, -0.1, 4.4, 5.5, 1.6, 4.6, 3.4];
        let t = t_paired(&drug2, &drug1, 0.0, 0.95, Alternative::TwoSided).unwrap();
        assert_eq!(t.kind, TTestKind::Paired);
        close(t.estimate, 1.58, 1e-13);
        close(t.t, 4.062, 1e-4);
        close(t.p_value, 0.00283, 2e-3);
        close(t.ci.low, 0.700, 5e-4);
        close(t.ci.high, 2.460, 5e-4);
        let one = t_one_sample(
            &differences(&drug2, &drug1),
            0.0,
            0.95,
            Alternative::TwoSided,
        )
        .unwrap();
        assert_eq!(one.t.to_bits(), t.t.to_bits());
        assert_eq!(one.ci, t.ci);
        // Same as the Bonferroni pair of `compare` with a single comparison.
        let b = crate::compare::bonferroni_paired(&[&drug2, &drug1], 0.95).unwrap();
        close(b.pairs[0].p_value, t.p_value, 1e-12);
        close(b.pairs[0].low(), t.ci.low, 1e-12);
    }

    #[test]
    fn t_degenerate_cases() {
        assert!(t_one_sample(&[1.0], 0.0, 0.95, Alternative::TwoSided).is_none());
        assert!(
            t_two_sample(&[1.0, 2.0], &[3.0], 0.0, false, 0.95, Alternative::TwoSided).is_none()
        );
        let flat = t_one_sample(&[2.0, 2.0, 2.0], 1.0, 0.95, Alternative::TwoSided).unwrap();
        assert!(flat.t.is_infinite() && flat.p_value == 0.0);
    }

    /// Agresti, *An Introduction to Categorical Data Analysis*, 2nd ed., table 2.5
    /// (party identification by gender, General Social Survey): X² = 30.07 and
    /// G² = 30.02 with 2 df; expected counts 703.7, 319.6, 533.7 / 542.3, 246.4, 411.3;
    /// adjusted residuals 4.50, 0.70, −5.32 for females.
    #[test]
    fn chi_square_party_identification() {
        let t = vec![vec![762.0, 327.0, 468.0], vec![484.0, 239.0, 477.0]];
        let c = chi_square_independence(&t).unwrap();
        assert_eq!(c.df, 2);
        close(c.statistic, 30.07, 2e-4);
        close(c.g_statistic, 30.02, 2e-4);
        close(c.expected[0][0], 703.7, 1e-4);
        close(c.expected[1][2], 411.3, 1e-4);
        close(c.adjusted_residuals[0][0], 4.50, 1e-3);
        close(c.adjusted_residuals[0][1], 0.70, 1e-2);
        close(c.adjusted_residuals[0][2], -5.32, 1e-3);
        close(c.adjusted_residuals[1][2], 5.32, 1e-3);
        // χ²₂ tail is e^{−x/2} exactly.
        close(c.p_value, (-c.statistic / 2.0).exp(), 1e-12);
        assert!(!c.low_expected_warning);
        // Expected counts keep the margins.
        for (i, row) in c.expected.iter().enumerate() {
            close(row.iter().sum::<f64>(), c.row_totals[i], 1e-12);
        }
    }

    #[test]
    fn chi_square_flags_small_expected_counts() {
        let c = chi_square_independence(&[vec![3.0, 1.0], vec![1.0, 3.0]]).unwrap();
        assert_eq!(c.cells_below_5, 4);
        assert!(c.low_expected_warning);
        assert!(chi_square_independence(&[vec![1.0, 2.0]]).is_none());
        assert!(chi_square_independence(&[vec![1.0, 2.0], vec![3.0]]).is_none());
        assert!(chi_square_independence(&[vec![0.0, 2.0], vec![0.0, 3.0]]).is_none());
    }

    /// Mendel's dihybrid cross: 315 round yellow, 108 round green, 101 wrinkled yellow
    /// and 32 wrinkled green against 9:3:3:1; X² = 0.470 with 3 df, p = 0.925 (worked in
    /// most introductory texts, e.g. Agresti, chapter 1).
    #[test]
    fn goodness_of_fit_mendel() {
        let g = chi_square_goodness_of_fit(
            &[315.0, 108.0, 101.0, 32.0],
            Some(&[9.0, 3.0, 3.0, 1.0]),
            0,
        )
        .unwrap();
        assert_eq!(g.df, 3);
        close(g.statistic, 0.470, 1e-3);
        close(g.p_value, 0.925, 1e-3);
        close(g.expected[0], 312.75, 1e-14);
        // Uniform by default; an estimated parameter takes a degree of freedom.
        let u = chi_square_goodness_of_fit(&[10.0, 10.0, 10.0], None, 1).unwrap();
        assert_eq!((u.df, u.statistic), (1, 0.0));
        assert_eq!(u.p_value, 1.0);
        assert!(chi_square_goodness_of_fit(&[1.0, 2.0], None, 1).is_none());
        assert!(chi_square_goodness_of_fit(&[1.0, 2.0], Some(&[1.0]), 0).is_none());
    }

    /// Fisher's tea-tasting experiment (Fisher, 1935; Agresti, section 2.6): table
    /// [[3, 1], [1, 3]]; the hypergeometric probabilities of a = 0..4 are 1, 16, 36, 16,
    /// 1 over 70, so P(a ≥ 3) = 17/70 and the two-sided p-value is 34/70.
    #[test]
    fn fisher_tea_tasting() {
        let f = fisher_exact([[3, 1], [1, 3]]).unwrap();
        close(f.p.greater, 17.0 / 70.0, 1e-13);
        close(f.p.less, 69.0 / 70.0, 1e-13);
        close(f.p.two_sided, 34.0 / 70.0, 1e-13);
        close(f.odds_ratio, 9.0, 1e-15);
        let all = fisher_exact([[4, 0], [0, 4]]).unwrap();
        close(all.p.greater, 1.0 / 70.0, 1e-13);
        assert!(all.odds_ratio.is_infinite());
        assert!(fisher_exact([[0, 0], [0, 0]]).is_none());
    }

    /// Exact binomial test against the probabilities summed by hand: 9 successes in 10
    /// trials with p₀ = 1/2, P(X ≥ 9) = 11/1024 and the two-sided p-value is 22/1024.
    #[test]
    fn exact_binomial_by_hand() {
        let t = one_proportion(
            9,
            10,
            0.5,
            ProportionMethod::Exact,
            0.95,
            Alternative::TwoSided,
        )
        .unwrap();
        close(t.p.greater, 11.0 / 1024.0, 1e-13);
        close(t.p.less, 1.0 - 1.0 / 1024.0, 1e-13);
        close(t.p.two_sided, 22.0 / 1024.0, 1e-13);
        // Clopper-Pearson contains the Wilson interval at small n (the plan's property)
        // and holds the estimate.
        assert!(t.ci.low <= t.wilson.low && t.wilson.high <= t.ci.high);
        assert!(t.ci.low < 0.9 && 0.9 < t.ci.high);
        // 0 of 10: upper Clopper-Pearson limit 1 − 0.025^{1/10}.
        let (lo, hi) = clopper_pearson(0, 10, 0.95);
        assert_eq!(lo, 0.0);
        close(hi, 1.0 - 0.025f64.powf(0.1), 1e-9);
        // One-sided bound: 0 of 10 at 95%: 1 − 0.05^{1/10}.
        let l =
            one_proportion(0, 10, 0.5, ProportionMethod::Exact, 0.95, Alternative::Less).unwrap();
        close(l.ci.high, 1.0 - 0.05f64.powf(0.1), 1e-9);
        // Asymmetric p₀: the two-sided p-value sums the probabilities not above P(X = 1).
        let a = one_proportion(
            1,
            10,
            0.3,
            ProportionMethod::Exact,
            0.95,
            Alternative::TwoSided,
        )
        .unwrap();
        let pm: Vec<f64> = (0..=10).map(|k| binomial_pmf(k, 10, 0.3)).collect();
        let want: f64 = pm.iter().filter(|&&q| q <= pm[1] * (1.0 + 1e-7)).sum();
        close(a.p.two_sided, want, 1e-14);
        close(pm.iter().sum::<f64>(), 1.0, 1e-14);
    }

    /// Montgomery and Runger (example of the semiconductor process fallout): 4 defective
    /// of 200, H₀: p = 0.05 against p < 0.05; z₀ = −1.95 and P = 0.0256.
    #[test]
    fn one_proportion_normal_semiconductor() {
        let t = one_proportion(
            4,
            200,
            0.05,
            ProportionMethod::Normal,
            0.95,
            Alternative::Less,
        )
        .unwrap();
        close(t.z, -1.95, 2e-3);
        close(t.p_value, 0.0256, 2e-2);
        close(t.p_value, norm_cdf(t.z), 1e-15);
        assert_eq!(t.ci.low, 0.0);
    }

    /// Montgomery and Runger (example of St. John's Wort against placebo): 27 of 100
    /// against 19 of 100; p̂ = 0.23, z₀ = 1.34, two-sided P = 0.18.
    #[test]
    fn two_proportions_st_johns_wort() {
        let t = two_proportions(27, 100, 19, 100, 0.95, Alternative::TwoSided).unwrap();
        close(t.pooled, 0.23, 1e-15);
        close(t.z, 1.34, 5e-3);
        close(t.p_value, 0.18, 2e-2);
        assert!(t.ci.low < 0.0 && t.ci.high > 0.0);
        assert!(two_proportions(1, 0, 1, 2, 0.95, Alternative::TwoSided).is_none());
    }

    #[test]
    fn one_way_wires_anova_and_tukey() {
        // Montgomery, *Design and Analysis of Experiments*, etch rate example (also in
        // `compare`): F₀ = 66.80; R² = 66870.55 / 72209.75.
        let g: [&[f64]; 4] = [
            &[575.0, 542.0, 530.0, 539.0, 570.0],
            &[565.0, 593.0, 590.0, 579.0, 610.0],
            &[600.0, 651.0, 610.0, 637.0, 629.0],
            &[725.0, 700.0, 715.0, 685.0, 710.0],
        ];
        let o = one_way_anova(&g, 0.95).unwrap();
        close(o.anova.f, 66.80, 5e-3);
        close(o.r_squared, 66870.55 / 72209.75, 1e-6);
        close(o.residual_sd, (5339.20f64 / 16.0).sqrt(), 1e-6);
        assert_eq!(o.tukey.pairs.len(), 6);
    }
}
