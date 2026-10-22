//! Weibull family.

use utsim_core::Dist;

use super::common::{Law, fitted, require_positive, summarize};
use crate::family::{Family, Fitted};
use crate::special::brent;

struct WeibullLaw {
    shape: f64,
    scale: f64,
}

impl Law for WeibullLaw {
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            -(-(x / self.scale).powf(self.shape)).exp_m1()
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let z = x / self.scale;
        self.shape / self.scale * z.powf(self.shape - 1.0) * (-z.powf(self.shape)).exp()
    }

    fn quantile(&self, p: f64) -> f64 {
        self.scale * (-(-p).ln_1p()).powf(1.0 / self.shape)
    }
}

/// Maximum-likelihood Weibull `(shape, scale)`: the shape is the root of
/// `Σ xᵏ ln x / Σ xᵏ - 1/k - mean(ln x) = 0` (increasing in `k`), found with Brent
/// after bracketing around the moment estimate `1.28 / sd(ln x)`; then
/// `scale = (mean xᵏ)^(1/k)`. Data are divided by their maximum to avoid overflow.
pub(super) fn weibull_mle(data: &[f64]) -> Result<(f64, f64), String> {
    let n = data.len() as f64;
    let xmax = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let l: Vec<f64> = data.iter().map(|x| (x / xmax).ln()).collect();
    let mean_l = l.iter().sum::<f64>() / n;
    let sd_l = (l.iter().map(|v| (v - mean_l).powi(2)).sum::<f64>() / n).sqrt();
    if sd_l < 1e-10 {
        return Err("dados praticamente constantes".into());
    }
    let g = |k: f64| {
        let (mut sw, mut swl) = (0.0, 0.0);
        for &v in &l {
            let w = (k * v).exp();
            sw += w;
            swl += w * v;
        }
        swl / sw - 1.0 / k - mean_l
    };
    let k0 = 1.2825 / sd_l;
    let (mut lo, mut hi) = (k0, k0);
    while g(lo) > 0.0 {
        lo /= 2.0;
        if lo < 1e-6 {
            return Err("não convergiu".into());
        }
    }
    while g(hi) < 0.0 {
        hi *= 2.0;
        if hi > 1e6 {
            return Err("não convergiu".into());
        }
    }
    let k = brent(g, lo, hi, 1e-11 * k0).ok_or("não convergiu")?;
    let mean_w = l.iter().map(|v| (k * v).exp()).sum::<f64>() / n;
    Ok((k, xmax * mean_w.powf(1.0 / k)))
}

pub struct Weibull;

impl Family for Weibull {
    fn name(&self) -> &'static str {
        "Weibull"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        require_positive(&s)?;
        let (shape, scale) = weibull_mle(data)?;
        Ok(fitted(
            self.name(),
            vec![("forma", shape), ("escala", scale)],
            WeibullLaw { shape, scale },
            Dist::Weibull { shape, scale },
        ))
    }
}
