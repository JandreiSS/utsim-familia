//! Descriptive statistics of one sample and the data behind its graphs.
//!
//! - [`describe`]: n, mean, standard deviation, variance, standard error, coefficient
//!   of variation, minimum, quartiles, median, maximum, range, IQR, skewness, kurtosis,
//!   mode, and the confidence intervals of the mean (Student t), of the standard
//!   deviation (χ²) and of the median (order statistics, distribution free).
//! - [`histogram`]: classes by the Freedman-Diaconis or Sturges rule (the rule used is
//!   part of the result), or a given number of classes.
//! - [`boxplot`]: the five numbers, the fences at 1.5 IQR, the whiskers (the most
//!   extreme values inside the fences) and the outliers.
//! - [`normal_probability_plot`]: ordered values against normal scores, with the line
//!   `mean + sd · z`.
//! - [`run_chart`]: the series with its median, the runs about the median (clustering,
//!   mixtures) and the runs up and down (trends, oscillation).
//!
//! **Definitions, declared because they change the numbers.** Quartiles: by default
//! [`QuantileMethod::Weibull`], position `(n + 1) p` (type 6 of Hyndman and Fan, 1996,
//! "Sample quantiles in statistical packages", *The American Statistician* 50, 361-365),
//! the usual one in quality engineering; [`QuantileMethod::Linear`], position
//! `(n − 1) p + 1` (type 7), is the other usual choice. Skewness and kurtosis: the
//! adjusted estimators `G1 = √(n(n−1))/(n−2) · g1` and
//! `G2 = (n−1)/((n−2)(n−3)) · ((n+1) g2 + 6)` (excess kurtosis, 0 for the normal), with
//! `g1 = m3 / m2^{3/2}` and `g2 = m4 / m2² − 3` from the central moments (Joanes and
//! Gill, 1998, "Comparing measures of sample skewness and kurtosis", *The
//! Statistician* 47, 183-189, type 2).
//!
//! **Accuracy.** The mean is the plain mean corrected by the mean of the residuals
//! (`m = m₀ + Σ(x − m₀)/n`) and the variance is the two-pass sum of squared deviations
//! with the compensation term of Björck; on the NIST StRD univariate datasets
//! (`tests/nist.rs`) both reach the certified values to at least 13 digits on the
//! lower-difficulty sets, 9 on the average and 8 on the higher-difficulty one.

use serde::Serialize;

use crate::special::{chi2_ppf, ln_choose, norm_cdf, norm_ppf, t_ppf};

/// Where a sample quantile sits between the ordered values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum QuantileMethod {
    /// Position `(n + 1) p`, clamped to the extremes (Hyndman and Fan type 6).
    #[default]
    Weibull,
    /// Position `(n − 1) p + 1` (Hyndman and Fan type 7).
    Linear,
}

/// Quantile `p` (in `[0, 1]`) of values already sorted in increasing order, by
/// linear interpolation at the position of `method`. NaN for an empty slice.
pub fn quantile_sorted(sorted: &[f64], p: f64, method: QuantileMethod) -> f64 {
    let n = sorted.len();
    if n == 0 || !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    // 1-based position h; below 1 or above n the extreme value is taken.
    let h = match method {
        QuantileMethod::Weibull => (n as f64 + 1.0) * p,
        QuantileMethod::Linear => (n as f64 - 1.0) * p + 1.0,
    };
    if h <= 1.0 {
        return sorted[0];
    }
    if h >= n as f64 {
        return sorted[n - 1];
    }
    let lo = h.floor();
    let frac = h - lo;
    let i = lo as usize - 1;
    sorted[i] + frac * (sorted[i + 1] - sorted[i])
}

/// Sorted copy of the values (NaN sorts last; callers drop NaN first).
fn sorted(xs: &[f64]) -> Vec<f64> {
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    v
}

/// Mean with one refinement step: `m₀ + Σ(x − m₀)/n`. NaN for an empty slice.
pub fn mean(xs: &[f64]) -> f64 {
    let n = xs.len() as f64;
    if xs.is_empty() {
        return f64::NAN;
    }
    let m0 = xs.iter().sum::<f64>() / n;
    m0 + xs.iter().map(|x| x - m0).sum::<f64>() / n
}

/// Sample variance (denominator `n − 1`) by the corrected two-pass algorithm:
/// `(Σ d² − (Σ d)²/n) / (n − 1)` with `d = x − mean`. NaN with fewer than two values.
pub fn variance(xs: &[f64]) -> f64 {
    let n = xs.len();
    if n < 2 {
        return f64::NAN;
    }
    let m = mean(xs);
    let (mut s1, mut s2) = (0.0, 0.0);
    for x in xs {
        let d = x - m;
        s1 += d;
        s2 += d * d;
    }
    ((s2 - s1 * s1 / n as f64) / (n - 1) as f64).max(0.0)
}

/// Sample standard deviation (denominator `n − 1`).
pub fn std_dev(xs: &[f64]) -> f64 {
    variance(xs).sqrt()
}

/// A confidence interval: estimate, limits and the level they hold.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Interval {
    pub low: f64,
    pub high: f64,
    /// Confidence level the interval holds (for the median, the achieved level of the
    /// order statistics, at least the requested one).
    pub level: f64,
}

/// Descriptive summary of one sample.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Descriptive {
    pub n: usize,
    pub mean: f64,
    /// Sample standard deviation (denominator `n − 1`).
    pub sd: f64,
    pub variance: f64,
    /// Standard error of the mean, `sd / √n`.
    pub se_mean: f64,
    /// Coefficient of variation `sd / |mean|` (NaN when the mean is zero).
    pub cv: f64,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    pub range: f64,
    pub iqr: f64,
    pub quantile_method: QuantileMethod,
    /// Adjusted skewness `G1` (NaN for `n < 3` or no spread).
    pub skewness: f64,
    /// Adjusted excess kurtosis `G2` (NaN for `n < 4` or no spread).
    pub kurtosis: f64,
    /// The most frequent values, in increasing order; empty when every value appears
    /// once (no mode).
    pub modes: Vec<f64>,
    /// How many times each mode appears.
    pub mode_count: usize,
    /// Confidence level of the intervals below.
    pub level: f64,
    /// `mean ± t · se` (None for `n < 2`).
    pub mean_ci: Option<Interval>,
    /// `[sd √((n−1)/χ²_{1−α/2}), sd √((n−1)/χ²_{α/2})]`, valid for normal data (None
    /// for `n < 2`).
    pub sd_ci: Option<Interval>,
    /// `[x_(k), x_(n−k+1)]` with the largest `k` whose binomial coverage reaches the
    /// level (None when even `[min, max]` does not).
    pub median_ci: Option<Interval>,
}

/// Descriptive summary of `xs` (NaN values are dropped) with intervals at `level`.
/// `None` when no value is left.
pub fn describe(xs: &[f64], level: f64, method: QuantileMethod) -> Option<Descriptive> {
    let xs: Vec<f64> = xs.iter().copied().filter(|x| !x.is_nan()).collect();
    let n = xs.len();
    if n == 0 {
        return None;
    }
    let s = sorted(&xs);
    let nf = n as f64;
    let m = mean(&xs);
    let var = variance(&xs);
    let sd = var.sqrt();
    let (skewness, kurtosis) = shape(&xs, m);
    let (modes, mode_count) = modes(&s);
    let q = |p| quantile_sorted(&s, p, method);
    let (q1, q3) = (q(0.25), q(0.75));
    let alpha = 1.0 - level;
    let valid_level = level > 0.0 && level < 1.0;
    let mean_ci = (n >= 2 && valid_level).then(|| {
        let hw = t_ppf(1.0 - alpha / 2.0, nf - 1.0) * sd / nf.sqrt();
        Interval {
            low: m - hw,
            high: m + hw,
            level,
        }
    });
    let sd_ci = (n >= 2 && valid_level).then(|| {
        let df = nf - 1.0;
        Interval {
            low: sd * (df / chi2_ppf(1.0 - alpha / 2.0, df)).sqrt(),
            high: sd * (df / chi2_ppf(alpha / 2.0, df)).sqrt(),
            level,
        }
    });
    Some(Descriptive {
        n,
        mean: m,
        sd,
        variance: var,
        se_mean: sd / nf.sqrt(),
        cv: if m != 0.0 { sd / m.abs() } else { f64::NAN },
        min: s[0],
        q1,
        median: quantile_sorted(&s, 0.5, QuantileMethod::Linear),
        q3,
        max: s[n - 1],
        range: s[n - 1] - s[0],
        iqr: q3 - q1,
        quantile_method: method,
        skewness,
        kurtosis,
        modes,
        mode_count,
        level,
        mean_ci,
        sd_ci,
        median_ci: if valid_level {
            median_interval(&s, level)
        } else {
            None
        },
    })
}

/// Adjusted skewness `G1` and excess kurtosis `G2` (see the module notes).
fn shape(xs: &[f64], m: f64) -> (f64, f64) {
    let n = xs.len() as f64;
    let (mut m2, mut m3, mut m4) = (0.0, 0.0, 0.0);
    for x in xs {
        let d = x - m;
        let d2 = d * d;
        m2 += d2;
        m3 += d2 * d;
        m4 += d2 * d2;
    }
    m2 /= n;
    m3 /= n;
    m4 /= n;
    if m2.is_nan() || m2 <= 0.0 {
        return (f64::NAN, f64::NAN);
    }
    let g1 = m3 / m2.powf(1.5);
    let g2 = m4 / (m2 * m2) - 3.0;
    let skew = if n >= 3.0 {
        (n * (n - 1.0)).sqrt() / (n - 2.0) * g1
    } else {
        f64::NAN
    };
    let kurt = if n >= 4.0 {
        (n - 1.0) / ((n - 2.0) * (n - 3.0)) * ((n + 1.0) * g2 + 6.0)
    } else {
        f64::NAN
    };
    (skew, kurt)
}

/// The most frequent values of a sorted slice and their count (empty when all are
/// distinct).
fn modes(s: &[f64]) -> (Vec<f64>, usize) {
    let mut best = 0;
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let mut j = i;
        while j + 1 < s.len() && s[j + 1] == s[i] {
            j += 1;
        }
        let c = j - i + 1;
        if c > best {
            best = c;
            out.clear();
        }
        if c == best {
            out.push(s[i]);
        }
        i = j + 1;
    }
    if best <= 1 {
        (Vec::new(), 1.min(s.len()))
    } else {
        (out, best)
    }
}

/// `P(B ≤ k)` for `B ~ Binomial(n, 1/2)`, summed from the probabilities.
fn binom_half_cdf(k: usize, n: usize) -> f64 {
    let nf = n as f64;
    (0..=k.min(n))
        .map(|i| (ln_choose(nf, i as f64) - nf * std::f64::consts::LN_2).exp())
        .sum()
}

/// Distribution-free interval of the median by order statistics: `[x_(k), x_(n−k+1)]`
/// covers the median with probability `1 − 2 P(B ≤ k − 1)`, `B ~ Binomial(n, 1/2)`;
/// the largest `k` that reaches `level` is taken.
fn median_interval(s: &[f64], level: f64) -> Option<Interval> {
    let n = s.len();
    let mut best = None;
    for k in 1..=n.div_ceil(2) {
        let coverage = 1.0 - 2.0 * binom_half_cdf(k - 1, n);
        if coverage + 1e-12 >= level {
            best = Some(Interval {
                low: s[k - 1],
                high: s[n - k],
                level: coverage.min(1.0),
            });
        } else {
            break;
        }
    }
    best
}

/// Rule for the classes of a histogram.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum BinRule {
    /// Width `2 · IQR · n^{−1/3}` (Freedman and Diaconis, 1981); falls back to Sturges
    /// when the IQR is zero.
    FreedmanDiaconis,
    /// `⌈log₂ n⌉ + 1` classes (Sturges, 1926).
    Sturges,
    /// This number of classes.
    Count(usize),
}

/// Classes of a histogram.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Histogram {
    /// The rule that produced the classes (Sturges when Freedman-Diaconis fell back).
    pub rule: BinRule,
    pub width: f64,
    /// `counts.len() + 1` class limits; class `i` is `[edges[i], edges[i+1])`, the last
    /// one closed on the right.
    pub edges: Vec<f64>,
    pub counts: Vec<usize>,
}

/// Histogram of `xs` (NaN dropped). `None` for an empty sample. A sample with no
/// spread gives one class of width 1 centred on the value.
pub fn histogram(xs: &[f64], rule: BinRule) -> Option<Histogram> {
    let s = sorted(
        &xs.iter()
            .copied()
            .filter(|x| !x.is_nan())
            .collect::<Vec<_>>(),
    );
    let n = s.len();
    if n == 0 {
        return None;
    }
    let (lo, hi) = (s[0], s[n - 1]);
    if hi <= lo {
        return Some(Histogram {
            rule,
            width: 1.0,
            edges: vec![lo - 0.5, lo + 0.5],
            counts: vec![n],
        });
    }
    let sturges = || (n as f64).log2().ceil() as usize + 1;
    let (rule, k) = match rule {
        BinRule::Count(k) => (rule, k.max(1)),
        BinRule::Sturges => (rule, sturges()),
        BinRule::FreedmanDiaconis => {
            let iqr = quantile_sorted(&s, 0.75, QuantileMethod::Weibull)
                - quantile_sorted(&s, 0.25, QuantileMethod::Weibull);
            if iqr > 0.0 {
                let w = 2.0 * iqr / (n as f64).cbrt();
                (rule, (((hi - lo) / w).ceil() as usize).max(1))
            } else {
                (BinRule::Sturges, sturges())
            }
        }
    };
    let width = (hi - lo) / k as f64;
    let edges: Vec<f64> = (0..=k).map(|i| lo + width * i as f64).collect();
    let mut counts = vec![0; k];
    for x in &s {
        let i = (((x - lo) / width).floor() as usize).min(k - 1);
        counts[i] += 1;
    }
    Some(Histogram {
        rule,
        width,
        edges,
        counts,
    })
}

/// Data of a box plot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BoxPlot {
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    /// `q1 − 1.5 IQR` and `q3 + 1.5 IQR`.
    pub lower_fence: f64,
    pub upper_fence: f64,
    /// The most extreme values inside the fences.
    pub lower_whisker: f64,
    pub upper_whisker: f64,
    /// Values outside the fences, in increasing order.
    pub outliers: Vec<f64>,
}

/// Box plot of `xs` (NaN dropped) with the quartiles of `method`. `None` when empty.
pub fn boxplot(xs: &[f64], method: QuantileMethod) -> Option<BoxPlot> {
    let s = sorted(
        &xs.iter()
            .copied()
            .filter(|x| !x.is_nan())
            .collect::<Vec<_>>(),
    );
    let n = s.len();
    if n == 0 {
        return None;
    }
    let q1 = quantile_sorted(&s, 0.25, method);
    let q3 = quantile_sorted(&s, 0.75, method);
    let iqr = q3 - q1;
    let (lf, uf) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
    let inside = || s.iter().copied().filter(|&x| x >= lf && x <= uf);
    Some(BoxPlot {
        min: s[0],
        q1,
        median: quantile_sorted(&s, 0.5, QuantileMethod::Linear),
        q3,
        max: s[n - 1],
        lower_fence: lf,
        upper_fence: uf,
        lower_whisker: inside().next().unwrap_or(q1),
        upper_whisker: inside().next_back().unwrap_or(q3),
        outliers: s.iter().copied().filter(|&x| x < lf || x > uf).collect(),
    })
}

/// Plotting position of the `i`-th of `n` ordered values (1-based).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum PlottingPosition {
    /// `(i − 0.3)/(n + 0.4)`, Benard's approximation of the median rank.
    #[default]
    Benard,
    /// `(i − 3/8)/(n + 1/4)` (Blom, 1958), nearly unbiased for normal scores.
    Blom,
}

impl PlottingPosition {
    pub fn probability(self, i: usize, n: usize) -> f64 {
        let (i, n) = (i as f64, n as f64);
        match self {
            PlottingPosition::Benard => (i - 0.3) / (n + 0.4),
            PlottingPosition::Blom => (i - 0.375) / (n + 0.25),
        }
    }
}

/// One point of a normal probability plot.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ProbabilityPoint {
    /// The ordered value.
    pub value: f64,
    /// Its plotting position.
    pub probability: f64,
    /// Normal score `Φ⁻¹(probability)`.
    pub z: f64,
}

/// Data of a normal probability plot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProbabilityPlot {
    pub position: PlottingPosition,
    pub points: Vec<ProbabilityPoint>,
    /// The reference line `value = mean + sd · z`.
    pub mean: f64,
    pub sd: f64,
    /// Correlation between the values and their normal scores (near 1 for normal data).
    pub correlation: f64,
}

/// Normal probability plot of `xs` (NaN dropped). `None` with fewer than two values.
pub fn normal_probability_plot(xs: &[f64], position: PlottingPosition) -> Option<ProbabilityPlot> {
    let s = sorted(
        &xs.iter()
            .copied()
            .filter(|x| !x.is_nan())
            .collect::<Vec<_>>(),
    );
    let n = s.len();
    if n < 2 {
        return None;
    }
    let points: Vec<ProbabilityPoint> = s
        .iter()
        .enumerate()
        .map(|(i, &value)| {
            let probability = position.probability(i + 1, n);
            ProbabilityPoint {
                value,
                probability,
                z: norm_ppf(probability),
            }
        })
        .collect();
    let z: Vec<f64> = points.iter().map(|p| p.z).collect();
    Some(ProbabilityPlot {
        position,
        mean: mean(&s),
        sd: std_dev(&s),
        correlation: crate::correlation::pearson(&s, &z),
        points,
    })
}

/// A runs count with its normal approximation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RunsCount {
    pub runs: usize,
    pub expected: f64,
    pub sd: f64,
    /// `(runs − expected)/sd`.
    pub z: f64,
    /// `P(fewer runs)`: small for clustering (about the median) or trends (up and down).
    pub p_fewer: f64,
    /// `P(more runs)`: small for mixtures (about the median) or oscillation.
    pub p_more: f64,
}

impl RunsCount {
    fn new(runs: usize, expected: f64, var: f64) -> RunsCount {
        let sd = var.max(0.0).sqrt();
        let z = if sd > 0.0 {
            (runs as f64 - expected) / sd
        } else {
            0.0
        };
        RunsCount {
            runs,
            expected,
            sd,
            z,
            p_fewer: norm_cdf(z),
            p_more: 1.0 - norm_cdf(z),
        }
    }
}

/// Data and tests of a run chart.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunChart {
    /// The series in time order.
    pub values: Vec<f64>,
    pub median: f64,
    /// Points above and below the median (those equal to it are not counted).
    pub above: usize,
    pub below: usize,
    /// Runs about the median (Wald and Wolfowitz, 1940): expected
    /// `1 + 2 n₁ n₂ / N`, variance `2 n₁ n₂ (2 n₁ n₂ − N) / (N² (N − 1))`. None when one
    /// side is empty.
    pub about_median: Option<RunsCount>,
    /// Longest run on one side of the median.
    pub longest_run_median: usize,
    /// Runs up and down (Levene and Wolfowitz, 1944): expected `(2m − 1)/3`, variance
    /// `(16m − 29)/90`, with `m` the number of values after dropping repeated
    /// neighbours. None with fewer than three such values.
    pub up_down: Option<RunsCount>,
    /// Longest run up or down (number of steps in the same direction).
    pub longest_run_up_down: usize,
}

/// Run chart of the series `xs` (NaN dropped). `None` when empty.
pub fn run_chart(xs: &[f64]) -> Option<RunChart> {
    let values: Vec<f64> = xs.iter().copied().filter(|x| !x.is_nan()).collect();
    if values.is_empty() {
        return None;
    }
    let median = quantile_sorted(&sorted(&values), 0.5, QuantileMethod::Linear);
    let signs: Vec<bool> = values
        .iter()
        .filter(|&&x| x != median)
        .map(|&x| x > median)
        .collect();
    let above = signs.iter().filter(|&&s| s).count();
    let below = signs.len() - above;
    let (runs, longest) = count_runs(&signs);
    let about_median = (above > 0 && below > 0).then(|| {
        let (n1, n2) = (above as f64, below as f64);
        let nn = n1 + n2;
        let expected = 1.0 + 2.0 * n1 * n2 / nn;
        let var = 2.0 * n1 * n2 * (2.0 * n1 * n2 - nn) / (nn * nn * (nn - 1.0));
        RunsCount::new(runs, expected, var)
    });
    let steps: Vec<bool> = values
        .windows(2)
        .filter(|w| w[1] != w[0])
        .map(|w| w[1] > w[0])
        .collect();
    let (ud_runs, ud_longest) = count_runs(&steps);
    let m = steps.len() + 1;
    let up_down = (m >= 3).then(|| {
        let mf = m as f64;
        RunsCount::new(ud_runs, (2.0 * mf - 1.0) / 3.0, (16.0 * mf - 29.0) / 90.0)
    });
    Some(RunChart {
        values,
        median,
        above,
        below,
        about_median,
        longest_run_median: longest,
        up_down,
        longest_run_up_down: ud_longest,
    })
}

/// Number of runs of equal booleans and the longest one.
fn count_runs(s: &[bool]) -> (usize, usize) {
    if s.is_empty() {
        return (0, 0);
    }
    let (mut runs, mut longest, mut current) = (1, 1, 1);
    for w in s.windows(2) {
        if w[1] == w[0] {
            current += 1;
        } else {
            runs += 1;
            current = 1;
        }
        longest = longest.max(current);
    }
    (runs, longest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol * (1.0 + b.abs()), "{a} vs {b}");
    }

    #[test]
    fn quantile_definitions_by_hand() {
        let s = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        // Type 6: position (n+1)p = 2.25 → 2 + 0.25; 6.75 → 6.75.
        close(
            quantile_sorted(&s, 0.25, QuantileMethod::Weibull),
            2.25,
            1e-15,
        );
        close(
            quantile_sorted(&s, 0.75, QuantileMethod::Weibull),
            6.75,
            1e-15,
        );
        // Type 7: position (n−1)p + 1 = 2.75 and 6.25.
        close(
            quantile_sorted(&s, 0.25, QuantileMethod::Linear),
            2.75,
            1e-15,
        );
        close(
            quantile_sorted(&s, 0.75, QuantileMethod::Linear),
            6.25,
            1e-15,
        );
        assert_eq!(quantile_sorted(&s, 0.01, QuantileMethod::Weibull), 1.0);
        assert_eq!(quantile_sorted(&s, 1.0, QuantileMethod::Weibull), 8.0);
        assert!(quantile_sorted(&[], 0.5, QuantileMethod::Weibull).is_nan());
    }

    #[test]
    fn summary_by_hand() {
        // Deviations from the mean 5: −3, −1, −1, −1, 0, 0, 2, 4; m2 = 32/8 = 4,
        // m3 = 42/8, m4 = (81 + 1 + 1 + 1 + 16 + 256)/8 = 44.5.
        let xs = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let d = describe(&xs, 0.95, QuantileMethod::Weibull).unwrap();
        assert_eq!(d.n, 8);
        close(d.mean, 5.0, 1e-15);
        close(d.variance, 32.0 / 7.0, 1e-15);
        close(d.se_mean, (32.0f64 / 7.0).sqrt() / 8f64.sqrt(), 1e-15);
        let g1 = 5.25 / 8.0;
        close(d.skewness, (56.0f64).sqrt() / 6.0 * g1, 1e-14);
        let g2 = 44.5 / 16.0 - 3.0;
        close(d.kurtosis, 7.0 / 30.0 * (9.0 * g2 + 6.0), 1e-14);
        assert_eq!(d.modes, vec![4.0]);
        assert_eq!(d.mode_count, 3);
        // (n+1)p = 2.25: 4 + 0.25·0 = 4; 6.75: 5 + 0.75·2 = 6.5.
        close(d.q1, 4.0, 1e-15);
        close(d.q3, 6.5, 1e-15);
        close(d.median, 4.5, 1e-15);
        close(d.iqr, 2.5, 1e-15);
        assert_eq!((d.min, d.max, d.range), (2.0, 9.0, 7.0));
        // Mean interval with t(0.975; 7) = 2.365 from the t table.
        let ci = d.mean_ci.unwrap();
        close(ci.high - d.mean, 2.365 * d.se_mean, 2e-4);
        // Standard deviation interval with χ²(0.975; 7) = 16.013 and χ²(0.025; 7) = 1.690.
        let s = d.sd_ci.unwrap();
        close(s.low, d.sd * (7.0f64 / 16.013).sqrt(), 1e-4);
        close(s.high, d.sd * (7.0f64 / 1.690).sqrt(), 2e-4);
        // Median by order statistics, n = 8: k = 2 covers 1 − 2·9/256 = 0.9297 < 0.95,
        // so k = 1, [min, max], with 1 − 2/256 = 0.9922.
        let m = d.median_ci.unwrap();
        assert_eq!((m.low, m.high), (2.0, 9.0));
        close(m.level, 1.0 - 2.0 / 256.0, 1e-14);
        let m90 = describe(&xs, 0.90, QuantileMethod::Weibull)
            .unwrap()
            .median_ci
            .unwrap();
        assert_eq!((m90.low, m90.high), (4.0, 7.0));
        close(m90.level, 1.0 - 18.0 / 256.0, 1e-14);
    }

    #[test]
    fn degenerate_samples() {
        assert!(describe(&[], 0.95, QuantileMethod::Weibull).is_none());
        let one = describe(&[3.0, f64::NAN], 0.95, QuantileMethod::Weibull).unwrap();
        assert_eq!(one.n, 1);
        assert!(one.sd.is_nan() && one.mean_ci.is_none() && one.median_ci.is_none());
        let flat = describe(&[2.0; 5], 0.95, QuantileMethod::Weibull).unwrap();
        assert_eq!(flat.sd, 0.0);
        assert!(flat.skewness.is_nan());
        assert_eq!(flat.modes, vec![2.0]);
        let distinct = describe(&[1.0, 2.0, 3.0], 0.95, QuantileMethod::Weibull).unwrap();
        assert!(distinct.modes.is_empty());
        // Symmetric data: no skewness.
        let sym = describe(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.95, QuantileMethod::Weibull).unwrap();
        close(sym.skewness, 0.0, 1e-15);
    }

    #[test]
    fn histogram_rules() {
        let xs: Vec<f64> = (0..100).map(|i| i as f64).collect();
        // Sturges: ⌈log₂ 100⌉ + 1 = 8 classes.
        let h = histogram(&xs, BinRule::Sturges).unwrap();
        assert_eq!(h.counts.len(), 8);
        assert_eq!(h.counts.iter().sum::<usize>(), 100);
        close(h.width, 99.0 / 8.0, 1e-15);
        assert_eq!(h.edges.len(), 9);
        // Freedman-Diaconis: IQR (type 6) = 75.75 − 24.25 = 51.5, width 2·51.5/100^{1/3}
        // = 22.19, so ⌈99/22.19⌉ = 5 classes.
        let fd = histogram(&xs, BinRule::FreedmanDiaconis).unwrap();
        assert_eq!(fd.rule, BinRule::FreedmanDiaconis);
        assert_eq!(fd.counts.len(), 5);
        assert_eq!(fd.counts.iter().sum::<usize>(), 100);
        // The maximum falls in the last class (closed on the right).
        let c = histogram(&[0.0, 1.0, 2.0, 3.0], BinRule::Count(3)).unwrap();
        assert_eq!(c.counts, vec![1, 1, 2]);
        // No IQR: Freedman-Diaconis falls back to Sturges and says so.
        let tied = histogram(
            &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 9.0],
            BinRule::FreedmanDiaconis,
        )
        .unwrap();
        assert_eq!(tied.rule, BinRule::Sturges);
        let flat = histogram(&[4.0, 4.0], BinRule::Sturges).unwrap();
        assert_eq!(flat.counts, vec![2]);
        assert!(histogram(&[], BinRule::Sturges).is_none());
    }

    #[test]
    fn boxplot_fences_and_outliers() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 30.0];
        let b = boxplot(&xs, QuantileMethod::Weibull).unwrap();
        // Positions (n + 1)p = 2.25 and 6.75: Q1 = 2 + 0.25, Q3 = 6 + 0.75.
        close(b.q1, 2.25, 1e-15);
        close(b.q3, 6.75, 1e-15);
        close(b.upper_fence, 6.75 + 1.5 * 4.5, 1e-15);
        close(b.lower_fence, 2.25 - 1.5 * 4.5, 1e-15);
        assert_eq!(b.outliers, vec![30.0]);
        assert_eq!((b.lower_whisker, b.upper_whisker), (1.0, 7.0));
        assert_eq!(b.max, 30.0);
    }

    #[test]
    fn probability_plot_positions() {
        let p = normal_probability_plot(&[3.0, 1.0, 2.0], PlottingPosition::Benard).unwrap();
        // (1 − 0.3)/3.4, (2 − 0.3)/3.4 = 0.5, (3 − 0.3)/3.4.
        close(p.points[0].probability, 0.7 / 3.4, 1e-15);
        close(p.points[1].probability, 0.5, 1e-15);
        close(p.points[1].z, 0.0, 1e-12);
        close(p.points[2].z, -p.points[0].z, 1e-12);
        assert_eq!(p.points[0].value, 1.0);
        close(p.correlation, 1.0, 1e-12);
        close(
            PlottingPosition::Blom.probability(1, 10),
            0.625 / 10.25,
            1e-15,
        );
        assert!(normal_probability_plot(&[1.0], PlottingPosition::Blom).is_none());
    }

    #[test]
    fn run_chart_counts_by_hand() {
        // Median 4.5; signs − − − − + + + + : two runs, longest 4.
        let r = run_chart(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]).unwrap();
        close(r.median, 4.5, 1e-15);
        assert_eq!((r.above, r.below), (4, 4));
        let a = r.about_median.unwrap();
        assert_eq!(a.runs, 2);
        // Expected 1 + 2·16/8 = 5, variance 2·16·(32 − 8)/(64·7) = 12/7.
        close(a.expected, 5.0, 1e-15);
        close(a.sd, (12.0f64 / 7.0).sqrt(), 1e-15);
        assert!(a.p_fewer < 0.05);
        assert_eq!(r.longest_run_median, 4);
        // Always up: one run of 7 steps; expected (2·8 − 1)/3 = 5, variance (128 − 29)/90.
        let u = r.up_down.unwrap();
        assert_eq!(u.runs, 1);
        close(u.expected, 5.0, 1e-15);
        close(u.sd, (99.0f64 / 90.0).sqrt(), 1e-15);
        assert_eq!(r.longest_run_up_down, 7);
        // Alternating series: every step changes direction, many runs (oscillation).
        let o = run_chart(&[1.0, 9.0, 2.0, 8.0, 3.0, 7.0, 4.0, 6.0, 5.0]).unwrap();
        assert_eq!(o.up_down.unwrap().runs, 8);
        assert!(o.up_down.unwrap().p_more < 0.05);
        // The value equal to the median is not counted.
        assert_eq!(o.above + o.below, 8);
        assert!(run_chart(&[]).is_none());
    }
}
