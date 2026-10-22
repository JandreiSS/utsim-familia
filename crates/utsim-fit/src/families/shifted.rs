//! Shifted (three-parameter) variants: a location parameter added to a base family.

use utsim_core::Dist;

use super::common::{ShiftModel, summarize};
use super::{Exponential, ExponentialModel, Gamma, Lognormal, Weibull};
use crate::family::{Family, Fitted};
use crate::special::golden_min;

/// A base family plus a location `γ`, so that `X - γ` follows the base family.
///
/// Method: profile likelihood over the distance `δ = min(x) - γ > 0`. For each `δ` the
/// base family is fitted by maximum likelihood to `x - γ` and the log-likelihood is
/// recorded; `ln δ` is scanned on a grid of 25 points in `[ln(range/n), ln(10 range)]`
/// and the best cell is refined by golden-section search.
///
/// The lower limit `range / n` (the expected gap between the true lower bound and the
/// sample minimum) is a deliberate regularisation: for shapes below 1 (gamma, Weibull)
/// and for the lognormal, the three-parameter likelihood is unbounded as `γ → min(x)`,
/// so the unrestricted maximum is degenerate. With the limit, `γ = min - small epsilon`
/// is returned in those cases, which is the standard practical estimator.
pub struct ShiftedFamily {
    name: &'static str,
    base: Box<dyn Family>,
}

impl ShiftedFamily {
    pub fn weibull() -> Self {
        ShiftedFamily {
            name: "Weibull deslocada",
            base: Box::new(Weibull),
        }
    }

    pub fn gamma() -> Self {
        ShiftedFamily {
            name: "Gama deslocada",
            base: Box::new(Gamma),
        }
    }

    pub fn lognormal() -> Self {
        ShiftedFamily {
            name: "Lognormal deslocada",
            base: Box::new(Lognormal),
        }
    }

    /// Fits the base family at offset `min - delta`; returns the fit and its log-likelihood.
    fn at(&self, data: &[f64], min: f64, delta: f64) -> Option<(Fitted, f64)> {
        let y: Vec<f64> = data.iter().map(|x| x - (min - delta)).collect();
        let f = self.base.fit(&y).ok()?;
        let ll = f.log_likelihood(&y);
        ll.is_finite().then_some((f, ll))
    }
}

/// Wraps a base fit shifted by `offset`.
fn shifted_fit(name: &'static str, offset: f64, base: Fitted) -> Result<Fitted, String> {
    let inner = base.dist.ok_or("distribuição base sem Dist")?;
    let mut params = vec![("deslocamento", offset)];
    params.extend(base.params);
    Ok(Fitted {
        family: name,
        n_params: params.len(),
        params,
        model: Box::new(ShiftModel {
            offset,
            inner: base.model,
        }),
        dist: Some(Dist::Shifted {
            offset,
            dist: Box::new(inner),
        }),
    })
}

impl Family for ShiftedFamily {
    fn name(&self) -> &'static str {
        self.name
    }

    fn n_params(&self) -> usize {
        self.base.n_params() + 1
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        let range = s.max - s.min;
        let (lo, hi) = ((range / s.n as f64).ln(), (10.0 * range).ln());
        let steps = 24;
        let grid: Vec<f64> = (0..=steps)
            .map(|i| lo + (hi - lo) * i as f64 / steps as f64)
            .collect();
        let eval = |ld: f64| {
            self.at(data, s.min, ld.exp())
                .map_or(f64::NEG_INFINITY, |p| p.1)
        };
        let lls: Vec<f64> = grid.iter().map(|&ld| eval(ld)).collect();
        let (bi, best) = lls
            .iter()
            .cloned()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("grid is not empty");
        if !best.is_finite() {
            return Err("a família base não se ajusta aos dados deslocados".into());
        }
        let a = grid[bi.saturating_sub(1)];
        let b = grid[(bi + 1).min(steps)];
        let refined = golden_min(|ld| -eval(ld), a, b, 25);
        let ld = if eval(refined) > best {
            refined
        } else {
            grid[bi]
        };
        let delta = ld.exp();
        let (base, _) = self
            .at(data, s.min, delta)
            .ok_or("a família base não se ajusta aos dados deslocados")?;
        shifted_fit(self.name, s.min - delta, base)
    }
}

/// Exponential with a location. Closed form: the minimum-variance unbiased estimators
/// `θ = n (mean - min) / (n - 1)` and `γ = min - θ / n` (the sample minimum overshoots
/// the true location by `θ / n` on average).
pub struct ShiftedExponential;

impl Family for ShiftedExponential {
    fn name(&self) -> &'static str {
        "Exponencial deslocada"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        let n = s.n as f64;
        let theta = n * (s.mean - s.min) / (n - 1.0);
        let offset = s.min - theta / n;
        let base = Fitted {
            family: Exponential.name(),
            params: vec![("média", theta)],
            n_params: 1,
            model: Box::new(ExponentialModel { mean: theta }),
            dist: Some(Dist::Exp { mean: theta }),
        };
        shifted_fit(self.name(), offset, base)
    }
}
