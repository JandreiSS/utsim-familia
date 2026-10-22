//! Normal and lognormal families (closed-form maximum likelihood).

use utsim_core::Dist;

use super::common::{Law, fitted, require_positive, summarize};
use crate::family::{Family, Fitted};
use crate::special::{norm_cdf, norm_ppf};

const LN_SQRT_2PI: f64 = 0.918_938_533_204_672_7;

struct NormalLaw {
    mu: f64,
    sigma: f64,
}

impl Law for NormalLaw {
    fn cdf(&self, x: f64) -> f64 {
        norm_cdf((x - self.mu) / self.sigma)
    }

    fn pdf(&self, x: f64) -> f64 {
        let z = (x - self.mu) / self.sigma;
        (-0.5 * z * z - LN_SQRT_2PI).exp() / self.sigma
    }

    fn quantile(&self, p: f64) -> f64 {
        self.mu + self.sigma * norm_ppf(p)
    }
}

/// Normal: sample mean and standard deviation with divisor `n` (the MLE).
pub struct Normal;

impl Family for Normal {
    fn name(&self) -> &'static str {
        "Normal"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        let sd = s.var.sqrt();
        Ok(fitted(
            self.name(),
            vec![("média", s.mean), ("desvio", sd)],
            NormalLaw {
                mu: s.mean,
                sigma: sd,
            },
            Dist::Normal { mean: s.mean, sd },
        ))
    }
}

struct LognormalLaw {
    mu: f64,
    sigma: f64,
}

impl Law for LognormalLaw {
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            norm_cdf((x.ln() - self.mu) / self.sigma)
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let z = (x.ln() - self.mu) / self.sigma;
        (-0.5 * z * z - LN_SQRT_2PI).exp() / (self.sigma * x)
    }

    fn quantile(&self, p: f64) -> f64 {
        (self.mu + self.sigma * norm_ppf(p)).exp()
    }
}

/// Lognormal: `ln X` is normal, so the MLE is the mean and (divisor `n`) standard
/// deviation of `ln x`. Reported as `mu`/`sigma` of the underlying normal; the
/// returned [`Dist`] uses UTSim's mean/sd parameterisation.
pub struct Lognormal;

impl Family for Lognormal {
    fn name(&self) -> &'static str {
        "Lognormal"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        require_positive(&s)?;
        let n = data.len() as f64;
        let mu = data.iter().map(|x| x.ln()).sum::<f64>() / n;
        let var = data.iter().map(|x| (x.ln() - mu).powi(2)).sum::<f64>() / n;
        if var < 1e-20 {
            return Err("dados praticamente constantes".into());
        }
        let sigma = var.sqrt();
        let mean = (mu + var / 2.0).exp();
        let sd = mean * var.exp_m1().sqrt();
        Ok(fitted(
            self.name(),
            vec![("mu", mu), ("sigma", sigma)],
            LognormalLaw { mu, sigma },
            Dist::LogNormal { mean, sd },
        ))
    }
}
