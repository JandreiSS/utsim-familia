//! Families with a bounded support: Uniform, Triangular and Beta.

use utsim_core::Dist;

use super::common::{Law, fitted, summarize};
use crate::family::{Family, Fitted};
use crate::special::{beta_inc, digamma, ln_beta, ln_gamma, trigamma};

struct UniformLaw {
    a: f64,
    b: f64,
}

impl Law for UniformLaw {
    fn cdf(&self, x: f64) -> f64 {
        ((x - self.a) / (self.b - self.a)).clamp(0.0, 1.0)
    }

    fn pdf(&self, x: f64) -> f64 {
        if x < self.a || x > self.b {
            0.0
        } else {
            1.0 / (self.b - self.a)
        }
    }

    fn quantile(&self, p: f64) -> f64 {
        self.a + p * (self.b - self.a)
    }
}

/// Uniform on `[min, max]`; the MLE is the sample minimum and maximum.
pub struct Uniform;

impl Family for Uniform {
    fn name(&self) -> &'static str {
        "Uniforme"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        Ok(fitted(
            self.name(),
            vec![("mínimo", s.min), ("máximo", s.max)],
            UniformLaw { a: s.min, b: s.max },
            Dist::Uniform {
                min: s.min,
                max: s.max,
            },
        ))
    }
}

struct TriangularLaw {
    a: f64,
    m: f64,
    c: f64,
}

impl Law for TriangularLaw {
    fn cdf(&self, x: f64) -> f64 {
        let (a, m, c) = (self.a, self.m, self.c);
        if x <= a {
            0.0
        } else if x <= m {
            (x - a).powi(2) / ((c - a) * (m - a))
        } else if x < c {
            1.0 - (c - x).powi(2) / ((c - a) * (c - m))
        } else {
            1.0
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        let (a, m, c) = (self.a, self.m, self.c);
        if x < a || x > c {
            0.0
        } else if x <= m {
            2.0 * (x - a) / ((c - a) * (m - a))
        } else {
            2.0 * (c - x) / ((c - a) * (c - m))
        }
    }

    fn quantile(&self, p: f64) -> f64 {
        let (a, m, c) = (self.a, self.m, self.c);
        if p < (m - a) / (c - a) {
            a + (p * (c - a) * (m - a)).sqrt()
        } else {
            c - ((1.0 - p) * (c - a) * (c - m)).sqrt()
        }
    }
}

/// Triangular `(min, mode, max)`.
///
/// Method: the support is the sample range widened by `range / n` on each side (the
/// likelihood of the exact range is degenerate at the extremes, and the extremes of a
/// sample always fall slightly inside the true support). For fixed `a` and `c` the
/// log-likelihood in the mode `m` is `const - j ln(m - a) - (n - j) ln(c - m)`, where
/// `j` is the number of observations below `m`. Between two consecutive observations
/// `j` is constant and this is convex in `m`, so the maximum is always at an
/// observation. We therefore evaluate every observation as a candidate mode in `O(n)`
/// (after sorting, with prefix sums of `ln(x - a)` and suffix sums of `ln(c - x)`) and
/// keep the best. This is exact, with no grid or refinement needed.
pub struct Triangular;

impl Family for Triangular {
    fn name(&self) -> &'static str {
        "Triangular"
    }

    fn n_params(&self) -> usize {
        3
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        let eps = (s.max - s.min) / s.n as f64;
        let (a, c) = (s.min - eps, s.max + eps);
        let mut xs = data.to_vec();
        xs.sort_by(f64::total_cmp);
        let n = xs.len();
        // pre[j] = sum_{i<j} ln(x_i - a); suf[j] = sum_{i>=j} ln(c - x_i)
        let mut pre = vec![0.0; n + 1];
        for i in 0..n {
            pre[i + 1] = pre[i] + (xs[i] - a).ln();
        }
        let mut suf = vec![0.0; n + 1];
        for i in (0..n).rev() {
            suf[i] = suf[i + 1] + (c - xs[i]).ln();
        }
        let mut best = (f64::NEG_INFINITY, xs[0]);
        for j in 0..n {
            // First index holding this value, so that `j` counts the values strictly below.
            if j > 0 && xs[j] == xs[j - 1] {
                continue;
            }
            let m = xs[j];
            let ll = pre[j] + suf[j] - j as f64 * (m - a).ln() - (n - j) as f64 * (c - m).ln();
            if ll > best.0 {
                best = (ll, m);
            }
        }
        let m = best.1;
        Ok(fitted(
            self.name(),
            vec![("mínimo", a), ("moda", m), ("máximo", c)],
            TriangularLaw { a, m, c },
            Dist::Triangular {
                min: a,
                mode: m,
                max: c,
            },
        ))
    }
}

/// Maximum-likelihood Beta(α, β) on (0, 1) from `g1 = mean ln y` and
/// `g2 = mean ln(1 - y)`. The log-likelihood is concave in (α, β), so Newton's
/// method with step halving (on the likelihood) always converges; the start is the
/// geometric-mean approximation `α ≈ ½ + G₁/(2(1 - G₁ - G₂))`.
pub(super) fn beta_mle(g1: f64, g2: f64) -> Result<(f64, f64), String> {
    let ll = |a: f64, b: f64| {
        ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + (a - 1.0) * g1 + (b - 1.0) * g2
    };
    let (e1, e2) = (g1.exp(), g2.exp());
    let den = 1.0 - e1 - e2;
    let (mut a, mut b) = if den > 0.05 {
        (0.5 + e1 / (2.0 * den), 0.5 + e2 / (2.0 * den))
    } else {
        (1.0, 1.0)
    };
    let mut cur = ll(a, b);
    for _ in 0..200 {
        let (psi_s, tri_s) = (digamma(a + b), trigamma(a + b));
        let (ga, gb) = (psi_s - digamma(a) + g1, psi_s - digamma(b) + g2);
        // Negative Hessian [[h11, h12], [h12, h22]] is positive definite.
        let (h11, h12, h22) = (trigamma(a) - tri_s, -tri_s, trigamma(b) - tri_s);
        let det = h11 * h22 - h12 * h12;
        if det.is_nan() || det <= 0.0 {
            return Err("não convergiu".into());
        }
        let (da, db) = ((h22 * ga - h12 * gb) / det, (h11 * gb - h12 * ga) / det);
        let mut t = 1.0;
        let mut moved = false;
        for _ in 0..60 {
            let (na, nb) = (a + t * da, b + t * db);
            if na > 0.0 && nb > 0.0 {
                let v = ll(na, nb);
                if v >= cur {
                    let small = (na - a).abs() < 1e-11 * a && (nb - b).abs() < 1e-11 * b;
                    (a, b, cur, moved) = (na, nb, v, true);
                    if small {
                        return Ok((a, b));
                    }
                    break;
                }
            }
            t /= 2.0;
        }
        if !moved {
            // No further improvement possible at machine precision.
            return Ok((a, b));
        }
        if a > 1e8 || b > 1e8 {
            return Err("não convergiu (forma degenerada)".into());
        }
    }
    Err("não convergiu".into())
}

struct BetaLaw {
    a: f64,
    b: f64,
    lo: f64,
    hi: f64,
}

impl Law for BetaLaw {
    fn cdf(&self, x: f64) -> f64 {
        beta_inc(self.a, self.b, (x - self.lo) / (self.hi - self.lo))
    }

    fn pdf(&self, x: f64) -> f64 {
        let y = (x - self.lo) / (self.hi - self.lo);
        if y <= 0.0 || y >= 1.0 {
            return 0.0;
        }
        ((self.a - 1.0) * y.ln() + (self.b - 1.0) * (-y).ln_1p() - ln_beta(self.a, self.b)).exp()
            / (self.hi - self.lo)
    }
}

/// Beta rescaled to `[min, max]`. The bounds are the sample range widened by
/// `range / n` on each side (so that no observation sits exactly on a bound, where the
/// density may be zero or infinite); then α and β are the maximum-likelihood
/// estimates for the rescaled data (see [`beta_mle`]). All four values count as
/// estimated parameters.
pub struct Beta;

impl Family for Beta {
    fn name(&self) -> &'static str {
        "Beta"
    }

    fn n_params(&self) -> usize {
        4
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        let eps = (s.max - s.min) / s.n as f64;
        let (lo, hi) = (s.min - eps, s.max + eps);
        let w = hi - lo;
        let n = s.n as f64;
        let g1 = data.iter().map(|x| ((x - lo) / w).ln()).sum::<f64>() / n;
        let g2 = data.iter().map(|x| ((hi - x) / w).ln()).sum::<f64>() / n;
        let (a, b) = beta_mle(g1, g2)?;
        Ok(fitted(
            self.name(),
            vec![("alfa", a), ("beta", b), ("mínimo", lo), ("máximo", hi)],
            BetaLaw { a, b, lo, hi },
            Dist::Beta {
                alpha: a,
                beta: b,
                min: lo,
                max: hi,
            },
        ))
    }
}
