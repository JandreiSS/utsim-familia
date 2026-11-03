//! Remaining time of an activity already under way (the residual time), used when a run
//! starts from a given state (see [`crate::init`]).
//!
//! An activity whose duration `X` follows a distribution has already run for `a` time
//! units. The time left is drawn from `X - a` conditioned on `X > a`, whose survival
//! function is `S(a + r) / S(a)`. Every draw comes from the stream passed in (the
//! block's own), so the same seed and the same state give the same numbers.
//!
//! Method by distribution:
//!
//! | distribution | method |
//! |---|---|
//! | `Exp` | memoryless: an ordinary draw |
//! | `Const(c)` | `max(c - a, 0)`, no draw |
//! | `Uniform` | uniform on `[max(a, min), max]` |
//! | `Weibull` | closed form: `scale ((a/scale)^k - ln(1-u))^(1/k)` |
//! | `LogLogistic` | closed form from `S(x) = (1-u) S(a)` |
//! | `Triangular`, `Empirical` | inverse CDF at `1 - (1-u) S(a)` (closed form) |
//! | `Normal`, `LogNormal`, `Gamma`, `Erlang`, `Beta`, `PearsonV`, `PearsonVI` | numeric inversion of `S(x) = (1-u) S(a)` by bracketing and bisection on the exact survival function |
//! | `Discrete` | the values above `a`, with their weights renormalised |
//! | `Geometric` | memoryless on the integers: `m + X'` with `m` the first integer above `a` |
//! | `Poisson`, `Binomial`, `NegBinomial` | the probability mass from the first integer above `a`, renormalised (relative to the first term, so no underflow) |
//! | `Shifted` | the inner distribution with `a - offset` |
//!
//! When `a` is at or below the lower end of the support the condition says nothing and
//! the draw is an ordinary one. When `a` is at or past the upper end (`Const`,
//! `Uniform`, `Triangular`, `Beta`, `Empirical`, `Discrete`, `Binomial`) the activity
//! should already have ended: the residual is 0 and [`Dist::can_exceed`] is false, so the
//! model can warn about inconsistent data.

use rand::Rng;

use crate::dist::Dist;
use crate::rng::SimRng;
use crate::special::{beta_inc, gamma_p, gamma_q, ln_gamma, norm_sf};

fn step(cond: bool) -> f64 {
    if cond { 1.0 } else { 0.0 }
}

/// `mu` and `sigma` of the underlying normal of a `LogNormal { mean, sd }`.
fn lognormal_params(mean: f64, sd: f64) -> (f64, f64) {
    let s2 = (1.0 + (sd / mean).powi(2)).ln();
    (mean.ln() - s2 / 2.0, s2.sqrt())
}

impl Dist {
    /// `P(X > x)`.
    pub fn survival(&self, x: f64) -> f64 {
        let s = match self {
            Dist::Const(c) => step(x < *c),
            Dist::Uniform { min, max } => {
                if x < *min {
                    1.0
                } else if x >= *max {
                    0.0
                } else {
                    (max - x) / (max - min)
                }
            }
            Dist::Exp { mean } => {
                if x <= 0.0 {
                    1.0
                } else {
                    (-x / mean).exp()
                }
            }
            Dist::Triangular { min, mode, max } => {
                if max <= min {
                    step(x < *min)
                } else if x <= *min {
                    1.0
                } else if x >= *max {
                    0.0
                } else if x <= *mode {
                    1.0 - (x - min).powi(2) / ((max - min) * (mode - min))
                } else {
                    (max - x).powi(2) / ((max - min) * (max - mode))
                }
            }
            Dist::Normal { mean, sd } => {
                if *sd == 0.0 {
                    step(x < *mean)
                } else {
                    norm_sf((x - mean) / sd)
                }
            }
            Dist::LogNormal { mean, sd } => {
                if x <= 0.0 {
                    1.0
                } else if *sd == 0.0 {
                    step(x < *mean)
                } else {
                    let (mu, sigma) = lognormal_params(*mean, *sd);
                    norm_sf((x.ln() - mu) / sigma)
                }
            }
            Dist::Gamma { shape, scale } => {
                if x <= 0.0 {
                    1.0
                } else {
                    gamma_q(*shape, x / scale)
                }
            }
            Dist::Erlang { k, mean } => {
                let k = f64::from(*k);
                if x <= 0.0 {
                    1.0
                } else {
                    gamma_q(k, x * k / mean)
                }
            }
            Dist::Weibull { shape, scale } => {
                if x <= 0.0 {
                    1.0
                } else {
                    (-(x / scale).powf(*shape)).exp()
                }
            }
            Dist::Discrete(table) => {
                let total: f64 = table.iter().map(|(_, w)| w).sum();
                table
                    .iter()
                    .filter(|(v, _)| *v > x)
                    .map(|(_, w)| w)
                    .sum::<f64>()
                    / total
            }
            Dist::Beta {
                alpha,
                beta,
                min,
                max,
            } => {
                if x < *min {
                    1.0
                } else if x >= *max {
                    0.0
                } else {
                    let y = (x - min) / (max - min);
                    beta_inc(*beta, *alpha, 1.0 - y)
                }
            }
            Dist::LogLogistic { shape, scale } => {
                if x <= 0.0 {
                    1.0
                } else {
                    1.0 / (1.0 + (x / scale).powf(*shape))
                }
            }
            // X = scale / G: X > x when G < scale / x.
            Dist::PearsonV { shape, scale } => {
                if x <= 0.0 {
                    1.0
                } else {
                    gamma_p(*shape, scale / x)
                }
            }
            // X / scale = B / (1 - B) with B ~ Beta(shape1, shape2).
            Dist::PearsonVI {
                shape1,
                shape2,
                scale,
            } => {
                if x <= 0.0 {
                    1.0
                } else {
                    let y = x / scale;
                    beta_inc(*shape2, *shape1, 1.0 / (1.0 + y))
                }
            }
            Dist::Shifted { offset, dist } => dist.survival(x - offset),
            Dist::Geometric { p } => {
                if x < 0.0 {
                    1.0
                } else if *p >= 1.0 {
                    0.0
                } else {
                    (1.0 - p).powf(x.floor() + 1.0)
                }
            }
            Dist::Poisson { .. } | Dist::Binomial { .. } | Dist::NegBinomial { .. } => {
                if x < 0.0 {
                    1.0
                } else {
                    let first = x.floor() + 1.0;
                    let (terms, _) = self.integer_tail(first);
                    let ln0 = self.ln_pmf(first);
                    if ln0 == f64::NEG_INFINITY {
                        0.0
                    } else {
                        terms.iter().sum::<f64>() * ln0.exp()
                    }
                }
            }
            Dist::Empirical(xs) => {
                let n = xs.len();
                if n == 1 {
                    step(x < xs[0])
                } else if x < xs[0] {
                    1.0
                } else if x >= xs[n - 1] {
                    0.0
                } else {
                    // Last point at or below x; its right neighbour is above x.
                    let i = xs[..n - 1].iter().rposition(|&v| v <= x).unwrap_or(0);
                    let f = (i as f64 + (x - xs[i]) / (xs[i + 1] - xs[i])) / (n - 1) as f64;
                    1.0 - f
                }
            }
        };
        s.clamp(0.0, 1.0)
    }

    /// Largest value the distribution can take (`f64::INFINITY` when unbounded).
    pub fn support_max(&self) -> f64 {
        match self {
            Dist::Const(c) => *c,
            Dist::Uniform { max, .. } | Dist::Triangular { max, .. } | Dist::Beta { max, .. } => {
                *max
            }
            Dist::Normal { mean, sd } | Dist::LogNormal { mean, sd } if *sd == 0.0 => *mean,
            Dist::Discrete(table) => table
                .iter()
                .filter(|(_, w)| *w > 0.0)
                .map(|(v, _)| *v)
                .fold(f64::NEG_INFINITY, f64::max),
            Dist::Empirical(xs) => xs.last().copied().unwrap_or(0.0),
            Dist::Binomial { n, p } => {
                if *p > 0.0 {
                    f64::from(*n)
                } else {
                    0.0
                }
            }
            Dist::Geometric { p } | Dist::NegBinomial { p, .. } if *p >= 1.0 => 0.0,
            Dist::Shifted { offset, dist } => offset + dist.support_max(),
            _ => f64::INFINITY,
        }
    }

    /// Whether an activity with this duration that already ran for `elapsed` can still be
    /// running (`X > elapsed` is possible). False means the data is inconsistent: the
    /// residual is then 0.
    pub fn can_exceed(&self, elapsed: f64) -> bool {
        elapsed <= 0.0 || elapsed < self.support_max()
    }

    /// Time left of an activity with this duration that already ran for `elapsed`: a
    /// draw of `X - elapsed` given `X > elapsed` (see the module docs for the method of
    /// each distribution). `elapsed <= 0` is an ordinary duration (negative draws clamped
    /// to zero, as in `Ctx::sample_duration`).
    pub fn sample_residual(&self, elapsed: f64, rng: &mut SimRng) -> f64 {
        if elapsed.is_nan() || elapsed <= 0.0 {
            return self.sample(rng).max(0.0);
        }
        self.excess(elapsed, rng).max(0.0)
    }

    /// A draw of `X - a` given `X > a`, for any `a`; 0 when `X > a` is impossible.
    fn excess(&self, a: f64, rng: &mut SimRng) -> f64 {
        match self {
            Dist::Exp { .. } if a >= 0.0 => self.sample(rng),
            Dist::Exp { .. } => self.sample(rng) - a,
            Dist::Const(c) => (c - a).max(0.0),
            Dist::Uniform { min, max } => {
                if a < *min {
                    self.sample(rng) - a
                } else if a >= *max {
                    0.0
                } else {
                    (max - a) * rng.random::<f64>()
                }
            }
            Dist::Weibull { shape, scale } => {
                if a <= 0.0 {
                    return self.sample(rng) - a;
                }
                let h = (a / scale).powf(*shape);
                let e = -(1.0 - rng.random::<f64>()).ln();
                scale * (h + e).powf(1.0 / shape) - a
            }
            Dist::LogLogistic { shape, scale } => {
                if a <= 0.0 {
                    return self.sample(rng) - a;
                }
                let target = (1.0 - rng.random::<f64>()) * self.survival(a);
                if target <= 0.0 {
                    return 0.0;
                }
                scale * (1.0 / target - 1.0).powf(1.0 / shape) - a
            }
            Dist::Triangular { min, mode, max } => {
                if a <= *min {
                    return self.sample(rng) - a;
                }
                let s = self.survival(a);
                if s <= 0.0 || max <= min {
                    return 0.0;
                }
                // q = 1 - p, kept apart so the upper tail keeps its precision.
                let q = (1.0 - rng.random::<f64>()) * s;
                let range = max - min;
                let x = if 1.0 - q < (mode - min) / range {
                    min + (range * (mode - min) * (1.0 - q)).sqrt()
                } else {
                    max - (range * (max - mode) * q).sqrt()
                };
                (x - a).max(0.0)
            }
            Dist::Empirical(xs) => {
                let n = xs.len();
                if n == 1 || a < xs[0] {
                    return self.sample(rng) - a;
                }
                let s = self.survival(a);
                if s <= 0.0 {
                    return 0.0;
                }
                let p = 1.0 - (1.0 - rng.random::<f64>()) * s;
                let pos = p * (n - 1) as f64;
                let i = (pos.floor() as usize).min(n - 2);
                let x = xs[i] + (pos - i as f64) * (xs[i + 1] - xs[i]);
                (x - a).max(0.0)
            }
            Dist::Discrete(table) => {
                let above: Vec<(f64, f64)> = table
                    .iter()
                    .copied()
                    .filter(|(v, w)| *v > a && *w > 0.0)
                    .collect();
                let total: f64 = above.iter().map(|(_, w)| w).sum();
                if above.is_empty() || total <= 0.0 {
                    return 0.0;
                }
                let mut x = rng.random::<f64>() * total;
                for (v, w) in &above {
                    if x < *w {
                        return v - a;
                    }
                    x -= w;
                }
                above.last().map_or(0.0, |(v, _)| v - a)
            }
            Dist::Shifted { offset, dist } => dist.excess(a - offset, rng),
            Dist::Geometric { p } => {
                if a < 0.0 {
                    self.sample(rng) - a
                } else if *p >= 1.0 {
                    0.0
                } else {
                    a.floor() + 1.0 + self.sample(rng) - a
                }
            }
            Dist::Poisson { .. } | Dist::Binomial { .. } | Dist::NegBinomial { .. } => {
                if a < 0.0 {
                    return self.sample(rng) - a;
                }
                let first = a.floor() + 1.0;
                let (terms, total) = self.integer_tail(first);
                if total <= 0.0 {
                    return 0.0;
                }
                let mut x = rng.random::<f64>() * total;
                for (i, w) in terms.iter().enumerate() {
                    if x < *w {
                        return first + i as f64 - a;
                    }
                    x -= w;
                }
                first + (terms.len().saturating_sub(1)) as f64 - a
            }
            Dist::Normal { mean, sd } | Dist::LogNormal { mean, sd } if *sd == 0.0 => {
                (mean - a).max(0.0)
            }
            Dist::Normal { .. } => self.invert(a, rng),
            Dist::LogNormal { .. }
            | Dist::Gamma { .. }
            | Dist::Erlang { .. }
            | Dist::PearsonV { .. }
            | Dist::PearsonVI { .. } => {
                if a <= 0.0 {
                    self.sample(rng) - a
                } else {
                    self.invert(a, rng)
                }
            }
            Dist::Beta { min, .. } => {
                if a <= *min {
                    self.sample(rng) - a
                } else {
                    self.invert(a, rng)
                }
            }
        }
    }

    /// Numeric inversion: the `x > a` with `S(x) = (1 - u) S(a)`, found by doubling a
    /// bracket and bisecting (the survival function is monotone, so this always converges).
    fn invert(&self, a: f64, rng: &mut SimRng) -> f64 {
        let u = rng.random::<f64>();
        let sa = self.survival(a);
        if sa <= 0.0 {
            return 0.0;
        }
        let target = (1.0 - u) * sa;
        if target >= sa {
            return 0.0;
        }
        let hint = match self {
            Dist::Normal { sd, .. } => *sd,
            _ => self.mean(),
        };
        let mut width = if hint.is_finite() && hint.abs() > 0.0 {
            hint.abs()
        } else {
            1.0
        };
        let mut lo = a;
        let mut hi = a + width;
        let mut rounds = 0;
        while self.survival(hi) > target {
            lo = hi;
            width *= 2.0;
            hi = a + width;
            rounds += 1;
            if rounds > 2000 || !hi.is_finite() {
                return lo - a;
            }
        }
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if mid <= lo || mid >= hi {
                break;
            }
            if self.survival(mid) > target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi) - a
    }

    /// `ln P(X = k)` of an integer family (`-inf` outside the support).
    fn ln_pmf(&self, k: f64) -> f64 {
        if k < 0.0 {
            return f64::NEG_INFINITY;
        }
        match self {
            Dist::Poisson { mean } => k * mean.ln() - mean - ln_gamma(k + 1.0),
            Dist::Binomial { n, p } => {
                let n = f64::from(*n);
                if k > n {
                    return f64::NEG_INFINITY;
                }
                let choose = ln_gamma(n + 1.0) - ln_gamma(k + 1.0) - ln_gamma(n - k + 1.0);
                let (lp, lq) = (p.ln(), (-p).ln_1p());
                // 0 * ln 0 counts as 0 (p = 0 or p = 1).
                let a = if k == 0.0 { 0.0 } else { k * lp };
                let b = if k == n { 0.0 } else { (n - k) * lq };
                choose + a + b
            }
            Dist::NegBinomial { r, p } => {
                if *p >= 1.0 {
                    return if k == 0.0 { 0.0 } else { f64::NEG_INFINITY };
                }
                ln_gamma(k + r) - ln_gamma(*r) - ln_gamma(k + 1.0) + r * p.ln() + k * (-p).ln_1p()
            }
            _ => f64::NEG_INFINITY,
        }
    }

    /// Probabilities of `first, first + 1, ...` relative to `P(X = first)` (so far
    /// tails do not underflow), until the rest is negligible, and their sum.
    fn integer_tail(&self, first: f64) -> (Vec<f64>, f64) {
        let ln0 = self.ln_pmf(first);
        if ln0 == f64::NEG_INFINITY {
            return (Vec::new(), 0.0);
        }
        let mode = match self {
            Dist::Poisson { mean } => *mean,
            Dist::Binomial { n, p } => f64::from(*n) * p,
            Dist::NegBinomial { r, p } => (r - 1.0).max(0.0) * (1.0 - p) / p,
            _ => 0.0,
        };
        let mut terms = Vec::new();
        let mut total = 0.0;
        let mut k = first;
        while terms.len() < 10_000_000 {
            let ln = self.ln_pmf(k);
            if ln == f64::NEG_INFINITY {
                break;
            }
            let w = (ln - ln0).exp();
            terms.push(w);
            total += w;
            if k > mode && w < 1e-17 * total {
                break;
            }
            k += 1.0;
        }
        (terms, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::stream;

    /// Mean residual life `E[X - a | X > a] = ∫_a^∞ S(x) dx / S(a)`, by Simpson's rule
    /// on `[a, a + span]` (the tail beyond is negligible for the cases below).
    fn mean_residual(d: &Dist, a: f64, span: f64) -> f64 {
        let n = 20_000;
        let h = span / n as f64;
        let mut sum = d.survival(a) + d.survival(a + span);
        for i in 1..n {
            let w = if i % 2 == 1 { 4.0 } else { 2.0 };
            sum += w * d.survival(a + i as f64 * h);
        }
        sum * h / 3.0 / d.survival(a)
    }

    fn sample_mean_residual(d: &Dist, a: f64, n: usize) -> f64 {
        let mut rng = stream(11, 0, 3);
        (0..n).map(|_| d.sample_residual(a, &mut rng)).sum::<f64>() / n as f64
    }

    #[test]
    fn residual_means_match_numeric_integration() {
        let cases = [
            (
                Dist::Weibull {
                    shape: 2.0,
                    scale: 10.0,
                },
                6.0,
                80.0,
            ),
            (
                Dist::Gamma {
                    shape: 3.0,
                    scale: 2.0,
                },
                5.0,
                120.0,
            ),
            (
                Dist::Triangular {
                    min: 2.0,
                    mode: 5.0,
                    max: 12.0,
                },
                6.0,
                6.0,
            ),
            (
                Dist::Normal {
                    mean: 10.0,
                    sd: 2.0,
                },
                11.0,
                30.0,
            ),
            (Dist::LogNormal { mean: 8.0, sd: 4.0 }, 9.0, 400.0),
            (
                Dist::Beta {
                    alpha: 2.0,
                    beta: 3.0,
                    min: 1.0,
                    max: 9.0,
                },
                4.0,
                5.0,
            ),
            (Dist::Erlang { k: 3, mean: 6.0 }, 4.0, 120.0),
            (
                Dist::LogLogistic {
                    shape: 4.0,
                    scale: 5.0,
                },
                6.0,
                4000.0,
            ),
            (
                Dist::PearsonV {
                    shape: 5.0,
                    scale: 20.0,
                },
                6.0,
                3000.0,
            ),
            (
                Dist::PearsonVI {
                    shape1: 3.0,
                    shape2: 6.0,
                    scale: 4.0,
                },
                3.0,
                3000.0,
            ),
            (Dist::Empirical(vec![1.0, 3.0, 4.0, 9.0]), 3.5, 6.0),
            (Dist::Uniform { min: 2.0, max: 8.0 }, 5.0, 3.0),
        ];
        for (d, a, span) in &cases {
            d.validate().unwrap();
            let exact = mean_residual(d, *a, *span);
            let sampled = sample_mean_residual(d, *a, 100_000);
            let rel = (sampled - exact).abs() / exact;
            assert!(
                rel < 0.02,
                "{d:?} a={a}: amostra {sampled} vs integral {exact}"
            );
        }
    }

    #[test]
    fn exponential_is_memoryless_and_const_is_exact() {
        let exp = Dist::Exp { mean: 4.0 };
        let m = sample_mean_residual(&exp, 30.0, 100_000);
        assert!((m - 4.0).abs() < 0.08, "{m}");
        let mut rng = stream(1, 0, 0);
        assert_eq!(Dist::Const(10.0).sample_residual(3.0, &mut rng), 7.0);
        assert_eq!(Dist::Const(10.0).sample_residual(12.0, &mut rng), 0.0);
        assert!(!Dist::Const(10.0).can_exceed(12.0));
        assert!(Dist::Exp { mean: 1.0 }.can_exceed(1e6));
    }

    #[test]
    fn residual_stays_inside_the_support() {
        let mut rng = stream(5, 0, 1);
        let tri = Dist::Triangular {
            min: 1.0,
            mode: 2.0,
            max: 6.0,
        };
        for _ in 0..2000 {
            let r = tri.sample_residual(4.0, &mut rng);
            assert!((0.0..=2.0).contains(&r), "{r}");
        }
        // Past the end of the support: zero and flagged.
        assert_eq!(tri.sample_residual(7.0, &mut rng), 0.0);
        assert!(!tri.can_exceed(7.0));
        let disc = Dist::Discrete(vec![(2.0, 1.0), (5.0, 1.0), (9.0, 2.0)]);
        for _ in 0..500 {
            let r = disc.sample_residual(4.0, &mut rng);
            assert!(r == 1.0 || r == 5.0, "{r}");
        }
        let shifted = Dist::Shifted {
            offset: 3.0,
            dist: Box::new(Dist::Exp { mean: 2.0 }),
        };
        // Before the offset nothing happened yet: the full shifted duration minus a.
        let m: f64 = (0..50_000)
            .map(|_| shifted.sample_residual(1.0, &mut rng))
            .sum::<f64>()
            / 50_000.0;
        assert!((m - 4.0).abs() < 0.06, "{m}");
    }

    #[test]
    fn integer_families_condition_exactly() {
        // E[X - a | X > a] by direct summation of the mass function.
        let cases = [
            Dist::Poisson { mean: 4.0 },
            Dist::Binomial { n: 12, p: 0.4 },
            Dist::NegBinomial { r: 2.5, p: 0.3 },
            Dist::Geometric { p: 0.25 },
        ];
        for d in &cases {
            let a = 3.5;
            let mut num = 0.0;
            let mut den = 0.0;
            for k in 4..400 {
                let p = d.pmf_any(k as f64);
                num += (k as f64 - a) * p;
                den += p;
            }
            let exact = num / den;
            let m = sample_mean_residual(d, a, 100_000);
            assert!((m - exact).abs() / exact < 0.02, "{d:?}: {m} vs {exact}");
            assert!((d.survival(a) - den).abs() < 1e-9, "{d:?}");
        }
    }

    #[test]
    fn residual_draws_are_reproducible() {
        let d = Dist::Gamma {
            shape: 2.0,
            scale: 3.0,
        };
        let draw = |seed| {
            let mut rng = stream(seed, 0, 7);
            (0..5)
                .map(|_| d.sample_residual(4.0, &mut rng))
                .collect::<Vec<_>>()
        };
        assert_eq!(draw(1), draw(1));
        assert_ne!(draw(1), draw(2));
    }

    impl Dist {
        /// Probability mass of the integer families, geometric included (tests only).
        fn pmf_any(&self, k: f64) -> f64 {
            match self {
                Dist::Geometric { p } => p * (1.0 - p).powf(k),
                _ => self.ln_pmf(k).exp(),
            }
        }
    }
}
