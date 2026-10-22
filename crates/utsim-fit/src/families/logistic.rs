//! Log-logistic family.

use std::f64::consts::PI;

use utsim_core::Dist;

use super::common::{Law, fitted, require_positive, summarize};
use crate::family::{Family, Fitted};

struct LogLogisticLaw {
    shape: f64,
    scale: f64,
}

impl Law for LogLogisticLaw {
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            1.0 / (1.0 + (x / self.scale).powf(-self.shape))
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let z = (x / self.scale).powf(self.shape);
        self.shape / x * z / ((1.0 + z) * (1.0 + z))
    }

    fn quantile(&self, p: f64) -> f64 {
        self.scale * (p / (1.0 - p)).powf(1.0 / self.shape)
    }
}

/// `ln(1 + eᵛ)` without overflow.
fn softplus(v: f64) -> f64 {
    v.max(0.0) + (-v.abs()).exp().ln_1p()
}

/// Logistic maximum likelihood for `z = ln x` (location `mu`, scale `s`); the
/// log-logistic shape is `1/s` and its scale is `exp(mu)`.
///
/// Newton's method on the exact 2x2 Hessian (Fisher scoring when it is not negative
/// definite), with step halving on the log-likelihood, started at the moment estimates.
fn logistic_mle(z: &[f64]) -> Result<(f64, f64), String> {
    let n = z.len() as f64;
    let mut mu = z.iter().sum::<f64>() / n;
    let sd = (z.iter().map(|v| (v - mu).powi(2)).sum::<f64>() / n).sqrt();
    let mut s = sd * 3.0_f64.sqrt() / PI;
    if s < 1e-12 {
        return Err("dados praticamente constantes".into());
    }
    let ll = |mu: f64, s: f64| -> f64 {
        z.iter()
            .map(|&v| {
                let u = (v - mu) / s;
                -u - s.ln() - 2.0 * softplus(-u)
            })
            .sum()
    };
    let mut cur = ll(mu, s);
    for _ in 0..200 {
        let (mut t_sum, mut q_sum, mut ut, mut uq, mut u2q) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &v in z {
            let u = (v - mu) / s;
            let t = (u / 2.0).tanh();
            let q = 1.0 - t * t;
            t_sum += t;
            q_sum += q;
            ut += u * t;
            uq += u * q;
            u2q += u * u * q;
        }
        let (g_mu, g_s) = (t_sum / s, (ut - n) / s);
        let h_mm = -q_sum / (2.0 * s * s);
        let h_ms = -(t_sum + uq / 2.0) / (s * s);
        let h_ss = (n - 2.0 * ut - u2q / 2.0) / (s * s);
        let det = h_mm * h_ss - h_ms * h_ms;
        let (d_mu, d_s) = if h_mm < 0.0 && det > 0.0 {
            // Newton: solve H d = -g
            (
                (-h_ss * g_mu + h_ms * g_s) / det,
                (h_ms * g_mu - h_mm * g_s) / det,
            )
        } else {
            // Fisher scoring with the expected information of the logistic law.
            (
                g_mu * 3.0 * s * s / n,
                g_s * 9.0 * s * s / (n * (PI * PI + 3.0)),
            )
        };
        let mut t = 1.0;
        let mut moved = false;
        for _ in 0..60 {
            let (nm, ns) = (mu + t * d_mu, s + t * d_s);
            if ns > 0.0 {
                let v = ll(nm, ns);
                if v >= cur {
                    let small = (nm - mu).abs() < 1e-11 * s && (ns - s).abs() < 1e-11 * s;
                    (mu, s, cur, moved) = (nm, ns, v, true);
                    if small {
                        return Ok((mu, s));
                    }
                    break;
                }
            }
            t /= 2.0;
        }
        if !moved {
            return Ok((mu, s));
        }
    }
    Err("não convergiu".into())
}

/// Log-logistic `(shape, scale)`. `ln X` is logistic, so we maximise the logistic
/// likelihood of `ln x` (the Jacobian `1/x` does not depend on the parameters).
pub struct LogLogistic;

impl Family for LogLogistic {
    fn name(&self) -> &'static str {
        "Log-logística"
    }

    fn n_params(&self) -> usize {
        2
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        require_positive(&s)?;
        let z: Vec<f64> = data.iter().map(|x| x.ln()).collect();
        let (mu, sc) = logistic_mle(&z)?;
        let (shape, scale) = (1.0 / sc, mu.exp());
        Ok(fitted(
            self.name(),
            vec![("forma", shape), ("escala", scale)],
            LogLogisticLaw { shape, scale },
            Dist::LogLogistic { shape, scale },
        ))
    }
}
