//! Plumbing shared by the families: the [`Law`] helper trait, data checks and
//! the generic quantile search.

use utsim_core::Dist;
use utsim_core::rng::SimRng;

use crate::family::{Fitted, FittedModel};

/// Distribution function and density of a fitted continuous law. Sampling is
/// delegated to the equivalent [`Dist`], so the fit and the model file always agree.
pub(super) trait Law: Send + Sync {
    fn cdf(&self, x: f64) -> f64;
    fn pdf(&self, x: f64) -> f64;
    /// Inverse CDF. Default: bisection on [`Law::cdf`]; override when closed form exists.
    fn quantile(&self, p: f64) -> f64 {
        bisect_quantile(&|x| self.cdf(x), p)
    }
}

/// A [`Law`] plus the [`Dist`] used to sample it.
pub(super) struct Model<L> {
    pub law: L,
    pub dist: Dist,
}

impl<L: Law> FittedModel for Model<L> {
    fn cdf(&self, x: f64) -> f64 {
        self.law.cdf(x)
    }

    fn pdf(&self, x: f64) -> f64 {
        self.law.pdf(x)
    }

    fn sample(&self, rng: &mut SimRng) -> f64 {
        self.dist.sample(rng)
    }

    fn quantile(&self, p: f64) -> f64 {
        self.law.quantile(p)
    }
}

/// Builds the [`Fitted`] of a continuous law.
pub(super) fn fitted<L: Law + 'static>(
    family: &'static str,
    params: Vec<(&'static str, f64)>,
    law: L,
    dist: Dist,
) -> Fitted {
    Fitted {
        family,
        n_params: params.len(),
        params,
        model: Box::new(Model {
            law,
            dist: dist.clone(),
        }),
        dist: Some(dist),
    }
}

/// Inverse of a CDF by bracketing then bisection. `p` is clamped away from 0 and 1.
pub(super) fn bisect_quantile(cdf: &dyn Fn(f64) -> f64, p: f64) -> f64 {
    let p = p.clamp(1e-15, 1.0 - 1e-15);
    let (mut lo, mut hi) = (-1.0, 1.0);
    for _ in 0..2000 {
        if cdf(lo) <= p {
            break;
        }
        lo *= 2.0;
    }
    for _ in 0..2000 {
        if cdf(hi) >= p {
            break;
        }
        hi *= 2.0;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if cdf(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-15 * mid.abs().max(1e-300) {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Basic statistics of a sample (population variance).
pub(super) struct Summary {
    pub n: usize,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub var: f64,
}

/// Checks that the sample is usable by any family and summarises it.
pub(super) fn summarize(data: &[f64]) -> Result<Summary, String> {
    if data.is_empty() {
        return Err("sem dados".into());
    }
    if data.iter().any(|x| !x.is_finite()) {
        return Err("há valores não finitos (NaN ou infinito)".into());
    }
    if data.len() < 3 {
        return Err("poucos dados (menos de 3 observações)".into());
    }
    let n = data.len();
    let mean = data.iter().sum::<f64>() / n as f64;
    let var = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if max <= min {
        return Err("todos os valores são iguais".into());
    }
    Ok(Summary {
        n,
        mean,
        min,
        max,
        var,
    })
}

/// Support check for families defined on `(0, ∞)`.
pub(super) fn require_positive(s: &Summary) -> Result<(), String> {
    if s.min <= 0.0 {
        Err("exige valores estritamente positivos (há valores ≤ 0)".into())
    } else {
        Ok(())
    }
}

/// Mean of `ln x` for a positive sample.
pub(super) fn mean_ln(data: &[f64]) -> f64 {
    data.iter().map(|x| x.ln()).sum::<f64>() / data.len() as f64
}

/// A model that shifts another one by a constant location.
pub(super) struct ShiftModel {
    pub offset: f64,
    pub inner: Box<dyn FittedModel>,
}

impl FittedModel for ShiftModel {
    fn cdf(&self, x: f64) -> f64 {
        self.inner.cdf(x - self.offset)
    }

    fn pdf(&self, x: f64) -> f64 {
        self.inner.pdf(x - self.offset)
    }

    fn sample(&self, rng: &mut SimRng) -> f64 {
        self.offset + self.inner.sample(rng)
    }

    fn quantile(&self, p: f64) -> f64 {
        self.offset + self.inner.quantile(p)
    }
}
