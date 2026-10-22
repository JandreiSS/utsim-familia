//! Discrete families: Poisson, Geometric, Binomial and Negative Binomial.
//!
//! All of them require non-negative integer data and report `Err` otherwise.
//! `pdf` is the probability mass function (zero at non-integers).

use std::collections::BTreeMap;

use utsim_core::Dist;
use utsim_core::rng::SimRng;

use crate::family::{Family, Fitted, FittedModel};
use crate::special::{beta_inc, brent, digamma, gamma_q, ln_choose, ln_gamma};

/// Mass function and CDF over `k = 0, 1, 2...`.
trait Pmf: Send + Sync {
    fn ln_pmf(&self, k: u64) -> f64;
    fn cdf_k(&self, k: u64) -> f64;
}

struct DiscreteModel<L> {
    law: L,
    dist: Dist,
}

impl<L: Pmf> FittedModel for DiscreteModel<L> {
    fn cdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            0.0
        } else {
            self.law.cdf_k(x.floor() as u64).clamp(0.0, 1.0)
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        if x < 0.0 || x.fract() != 0.0 {
            0.0
        } else {
            self.law.ln_pmf(x as u64).exp()
        }
    }

    fn sample(&self, rng: &mut SimRng) -> f64 {
        self.dist.sample(rng)
    }

    /// Smallest integer `k` with `P(X <= k) >= p`.
    fn quantile(&self, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0 - 1e-15);
        let mut hi = 1_u64;
        while self.law.cdf_k(hi) < p && hi < (1 << 50) {
            hi *= 2;
        }
        let mut lo = 0_u64;
        if self.law.cdf_k(0) >= p {
            return 0.0;
        }
        // cdf(lo) < p <= cdf(hi)
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if self.law.cdf_k(mid) < p {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi as f64
    }
}

fn build<L: Pmf + 'static>(
    family: &'static str,
    params: Vec<(&'static str, f64)>,
    law: L,
    dist: Dist,
) -> Fitted {
    Fitted {
        family,
        n_params: params.len(),
        params,
        model: Box::new(DiscreteModel {
            law,
            dist: dist.clone(),
        }),
        dist: Some(dist),
    }
}

/// Sample statistics of a non-negative integer sample.
struct Counts {
    n: usize,
    mean: f64,
    var: f64,
    max: u64,
    /// Histogram: value -> frequency.
    hist: BTreeMap<u64, usize>,
}

fn counts(data: &[f64]) -> Result<Counts, String> {
    if data.is_empty() {
        return Err("sem dados".into());
    }
    if data
        .iter()
        .any(|&x| !x.is_finite() || x < 0.0 || x.fract() != 0.0)
    {
        return Err("exige números inteiros não negativos".into());
    }
    if data.len() < 3 {
        return Err("poucos dados (menos de 3 observações)".into());
    }
    let n = data.len();
    let mean = data.iter().sum::<f64>() / n as f64;
    let var = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let mut hist = BTreeMap::new();
    for &x in data {
        *hist.entry(x as u64).or_insert(0) += 1;
    }
    let max = *hist.keys().next_back().expect("data is not empty");
    if hist.len() == 1 {
        return Err("todos os valores são iguais".into());
    }
    Ok(Counts {
        n,
        mean,
        var,
        max,
        hist,
    })
}

struct PoissonLaw {
    mean: f64,
}

impl Pmf for PoissonLaw {
    fn ln_pmf(&self, k: u64) -> f64 {
        let k = k as f64;
        k * self.mean.ln() - self.mean - ln_gamma(k + 1.0)
    }

    fn cdf_k(&self, k: u64) -> f64 {
        gamma_q(k as f64 + 1.0, self.mean)
    }
}

/// Poisson; the MLE is the sample mean.
pub struct Poisson;

impl Family for Poisson {
    fn name(&self) -> &'static str {
        "Poisson"
    }

    fn n_params(&self) -> usize {
        1
    }

    fn is_discrete(&self) -> bool {
        true
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let c = counts(data)?;
        Ok(build(
            self.name(),
            vec![("média", c.mean)],
            PoissonLaw { mean: c.mean },
            Dist::Poisson { mean: c.mean },
        ))
    }
}

struct GeometricLaw {
    p: f64,
}

impl Pmf for GeometricLaw {
    fn ln_pmf(&self, k: u64) -> f64 {
        self.p.ln() + k as f64 * (-self.p).ln_1p()
    }

    fn cdf_k(&self, k: u64) -> f64 {
        -((k as f64 + 1.0) * (-self.p).ln_1p()).exp_m1()
    }
}

/// Geometric (failures before the first success, support 0, 1, 2...);
/// MLE `p = 1 / (1 + mean)`.
pub struct Geometric;

impl Family for Geometric {
    fn name(&self) -> &'static str {
        "Geométrica"
    }

    fn n_params(&self) -> usize {
        1
    }

    fn is_discrete(&self) -> bool {
        true
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let c = counts(data)?;
        let p = 1.0 / (1.0 + c.mean);
        Ok(build(
            self.name(),
            vec![("p", p)],
            GeometricLaw { p },
            Dist::Geometric { p },
        ))
    }
}

struct BinomialLaw {
    n: u64,
    p: f64,
}

impl Pmf for BinomialLaw {
    fn ln_pmf(&self, k: u64) -> f64 {
        if k > self.n {
            return f64::NEG_INFINITY;
        }
        ln_choose(self.n as f64, k as f64)
            + k as f64 * self.p.ln()
            + (self.n - k) as f64 * (-self.p).ln_1p()
    }

    fn cdf_k(&self, k: u64) -> f64 {
        if k >= self.n {
            1.0
        } else {
            beta_inc((self.n - k) as f64, k as f64 + 1.0, 1.0 - self.p)
        }
    }
}

/// Binomial `(n, p)`.
///
/// Method: profile likelihood over the number of trials `n >= max(data)`. For each `n`
/// the MLE of `p` is `mean / n`; `n` is scanned upwards from `max(data)` until the
/// profile has dropped for 20 consecutive values (it is unimodal in practice), using the
/// histogram so each step costs one `ln C(n, k)` per distinct value. The scan is capped
/// at `20 * max + 200`; if the likelihood is still rising there, the data are
/// indistinguishable from Poisson and the fit reports `Err`. Data with variance greater
/// than or equal to the mean are rejected up front for the same reason.
pub struct Binomial;

impl Family for Binomial {
    fn name(&self) -> &'static str {
        "Binomial"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn is_discrete(&self) -> bool {
        true
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let c = counts(data)?;
        if c.var >= c.mean {
            return Err("variância maior ou igual à média: não há subdispersão".into());
        }
        let total = c.mean * c.n as f64; // sum of the data
        let nn = c.n as f64;
        let ll = |n: u64| -> f64 {
            let nf = n as f64;
            let p = total / (nf * nn);
            let lc: f64 = c
                .hist
                .iter()
                .map(|(&k, &f)| f as f64 * ln_choose(nf, k as f64))
                .sum();
            lc + total * p.ln() + (nf * nn - total) * (-p).ln_1p()
        };
        let start = c.max.max(1);
        let cap = (20 * c.max + 200).min(u64::from(u32::MAX));
        let (mut best_n, mut best) = (start, ll(start));
        let mut n = start + 1;
        while n <= cap && n - best_n <= 20 {
            let v = ll(n);
            if v > best {
                (best_n, best) = (n, v);
            }
            n += 1;
        }
        if n > cap {
            return Err("não convergiu: os dados se parecem com uma Poisson".into());
        }
        let p = total / (best_n as f64 * nn);
        Ok(build(
            self.name(),
            vec![("n", best_n as f64), ("p", p)],
            BinomialLaw { n: best_n, p },
            Dist::Binomial {
                n: best_n as u32,
                p,
            },
        ))
    }
}

struct NegBinomialLaw {
    r: f64,
    p: f64,
}

impl Pmf for NegBinomialLaw {
    fn ln_pmf(&self, k: u64) -> f64 {
        let k = k as f64;
        ln_gamma(k + self.r) - ln_gamma(self.r) - ln_gamma(k + 1.0)
            + self.r * self.p.ln()
            + k * (-self.p).ln_1p()
    }

    fn cdf_k(&self, k: u64) -> f64 {
        beta_inc(self.r, k as f64 + 1.0, self.p)
    }
}

/// Negative binomial `(r, p)`: failures before the `r`-th success, `r` real.
///
/// Method: for a given `r` the MLE of `p` is `r / (r + mean)`. Substituting, the
/// score equation for `r` is `Σ [ψ(xᵢ + r) - ψ(r)] + n ln(r / (r + mean)) = 0`, solved
/// with Brent starting from the moment estimate `mean² / (var - mean)`. It has a
/// finite root only when the data are overdispersed (`var > mean`); otherwise the MLE
/// tends to the Poisson (`r → ∞`) and the fit reports `Err`.
pub struct NegBinomial;

impl Family for NegBinomial {
    fn name(&self) -> &'static str {
        "Binomial negativa"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn is_discrete(&self) -> bool {
        true
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let c = counts(data)?;
        if c.var <= c.mean {
            return Err("variância menor ou igual à média: não há superdispersão".into());
        }
        let nn = c.n as f64;
        let score = |r: f64| -> f64 {
            let psi_r = digamma(r);
            let s: f64 = c
                .hist
                .iter()
                .map(|(&k, &f)| f as f64 * (digamma(k as f64 + r) - psi_r))
                .sum();
            s + nn * (r / (r + c.mean)).ln()
        };
        let r0 = c.mean * c.mean / (c.var - c.mean);
        // The score is positive for small r and negative for large r.
        let (mut lo, mut hi) = (r0, r0);
        while score(lo) < 0.0 {
            lo /= 2.0;
            if lo < 1e-6 {
                return Err("não convergiu".into());
            }
        }
        while score(hi) > 0.0 {
            hi *= 2.0;
            if hi > 1e8 {
                return Err("não convergiu: os dados se parecem com uma Poisson".into());
            }
        }
        let r = brent(score, lo, hi, 1e-10 * r0).ok_or("não convergiu")?;
        let p = r / (r + c.mean);
        Ok(build(
            self.name(),
            vec![("r", r), ("p", p)],
            NegBinomialLaw { r, p },
            Dist::NegBinomial { r, p },
        ))
    }
}
