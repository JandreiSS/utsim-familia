//! Gamma-like families: Erlang, Gamma and Pearson V (inverse gamma).

use utsim_core::Dist;

use super::common::{Law, Summary, fitted, mean_ln, require_positive, summarize};
use crate::family::{Family, Fitted};
use crate::special::{digamma, gamma_p, gamma_q, ln_gamma, trigamma};

/// Maximum-likelihood gamma fit from the sample mean and the mean of `ln x`.
/// Solves `ln k - ψ(k) = ln(mean) - mean(ln x)` for the shape `k` by Newton's
/// method started at Minka's closed-form approximation (2-3 iterations are typical).
/// Returns `(shape, scale)`.
pub(super) fn gamma_mle(mean: f64, mean_ln: f64) -> Result<(f64, f64), String> {
    let s = mean.ln() - mean_ln;
    if s <= 1e-10 {
        return Err("dados praticamente constantes".into());
    }
    let mut k = (3.0 - s + ((s - 3.0).powi(2) + 24.0 * s).sqrt()) / (12.0 * s);
    for _ in 0..100 {
        let f = k.ln() - digamma(k) - s;
        let df = 1.0 / k - trigamma(k);
        let mut step = f / df;
        while k - step <= 0.0 {
            step /= 2.0;
        }
        let next = k - step;
        let done = (next - k).abs() < 1e-10 * k;
        k = next;
        if k > 1e8 {
            break;
        }
        if done {
            return Ok((k, mean / k));
        }
    }
    Err("não convergiu".into())
}

pub(super) struct GammaLaw {
    pub k: f64,
    pub theta: f64,
}

impl Law for GammaLaw {
    fn cdf(&self, x: f64) -> f64 {
        gamma_p(self.k, x / self.theta)
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        ((self.k - 1.0) * x.ln() - x / self.theta - ln_gamma(self.k) - self.k * self.theta.ln())
            .exp()
    }
}

/// Gamma with integer shape. The shape `k` is chosen by profile likelihood over
/// `k = 1..=50`: for each `k` the scale MLE is `mean / k`, and the `k` with the highest
/// log-likelihood wins. `k` counts as an estimated parameter.
pub struct Erlang;

impl Family for Erlang {
    fn name(&self) -> &'static str {
        "Erlang"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        require_positive(&s)?;
        let ml = mean_ln(data);
        // Per-observation log-likelihood at the scale MLE theta = mean / k.
        let ll = |k: f64| -k * (s.mean / k).ln() - ln_gamma(k) + (k - 1.0) * ml - k;
        let k = (1..=50)
            .max_by(|&a, &b| ll(f64::from(a)).total_cmp(&ll(f64::from(b))))
            .expect("range is not empty");
        let kf = f64::from(k);
        Ok(fitted(
            self.name(),
            vec![("k", kf), ("média", s.mean)],
            GammaLaw {
                k: kf,
                theta: s.mean / kf,
            },
            Dist::Erlang { k, mean: s.mean },
        ))
    }
}

/// Gamma(shape, scale) by maximum likelihood (see [`gamma_mle`]).
pub struct Gamma;

impl Family for Gamma {
    fn name(&self) -> &'static str {
        "Gama"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        require_positive(&s)?;
        let (k, theta) = gamma_mle(s.mean, mean_ln(data))?;
        Ok(fitted(
            self.name(),
            vec![("forma", k), ("escala", theta)],
            GammaLaw { k, theta },
            Dist::Gamma {
                shape: k,
                scale: theta,
            },
        ))
    }
}

struct PearsonVLaw {
    shape: f64,
    scale: f64,
}

impl Law for PearsonVLaw {
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            gamma_q(self.shape, self.scale / x)
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        (self.shape * self.scale.ln()
            - ln_gamma(self.shape)
            - (self.shape + 1.0) * x.ln()
            - self.scale / x)
            .exp()
    }
}

/// Pearson type V: the reciprocal `1/X` is gamma-distributed, so the MLE is the gamma
/// MLE of `1/x` (shape unchanged, `scale = 1 / gamma scale`). The Jacobian `1/x²`
/// does not depend on the parameters, so maximising the gamma likelihood of `1/x`
/// maximises that of `x`.
pub struct PearsonV;

impl Family for PearsonV {
    fn name(&self) -> &'static str {
        "Pearson V"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s: Summary = summarize(data)?;
        require_positive(&s)?;
        let inv: Vec<f64> = data.iter().map(|x| 1.0 / x).collect();
        let mean_inv = inv.iter().sum::<f64>() / inv.len() as f64;
        let (k, theta) = gamma_mle(mean_inv, mean_ln(&inv))?;
        let scale = 1.0 / theta;
        Ok(fitted(
            self.name(),
            vec![("forma", k), ("escala", scale)],
            PearsonVLaw { shape: k, scale },
            Dist::PearsonV { shape: k, scale },
        ))
    }
}
