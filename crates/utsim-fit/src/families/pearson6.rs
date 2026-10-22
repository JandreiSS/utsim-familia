//! Pearson type VI (beta prime with scale).

use utsim_core::Dist;

use super::bounded::beta_mle;
use super::common::{Law, fitted, require_positive, summarize};
use crate::family::{Family, Fitted};
use crate::special::{beta_inc, golden_min, ln_beta, ln_gamma};

struct PearsonVILaw {
    a1: f64,
    a2: f64,
    scale: f64,
}

impl Law for PearsonVILaw {
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            beta_inc(self.a1, self.a2, x / (self.scale + x))
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let y = x / self.scale;
        ((self.a1 - 1.0) * y.ln() - (self.a1 + self.a2) * y.ln_1p() - ln_beta(self.a1, self.a2))
            .exp()
            / self.scale
    }
}

/// Profile log-likelihood at scale `beta`, with `(α1, α2)` at their maximum.
///
/// With `z = x / (beta + x)` the data are Beta(α1, α2), and
/// `ln f_X(x) = ln f_Z(z) + ln beta - 2 ln(beta + x)`.
fn profile(data: &[f64], beta: f64) -> Option<(f64, f64, f64)> {
    let n = data.len() as f64;
    let (mut g1, mut g2, mut jac) = (0.0, 0.0, 0.0);
    for &x in data {
        let d = beta + x;
        g1 += (x / d).ln();
        g2 += (beta / d).ln();
        jac += d.ln();
    }
    let (g1, g2) = (g1 / n, g2 / n);
    let (a1, a2) = beta_mle(g1, g2).ok()?;
    let ll_z = ln_gamma(a1 + a2) - ln_gamma(a1) - ln_gamma(a2) + (a1 - 1.0) * g1 + (a2 - 1.0) * g2;
    let ll = n * (ll_z + beta.ln()) - 2.0 * jac;
    ll.is_finite().then_some((ll, a1, a2))
}

/// Pearson VI `(shape1, shape2, scale)`.
///
/// Method: profile likelihood over the scale `β`. For fixed `β`, `z = x / (β + x)` is
/// Beta(α1, α2), so (α1, α2) come from the beta MLE (see `beta_mle`). `ln β` is
/// searched on a grid of 29 points in `[ln(median) - 4, ln(median) + 10]`, then the
/// best cell is refined by golden-section search. When the likelihood keeps growing
/// towards the upper edge the family is degenerating to a gamma, and the edge value is
/// returned.
pub struct PearsonVI;

impl Family for PearsonVI {
    fn name(&self) -> &'static str {
        "Pearson VI"
    }

    fn n_params(&self) -> usize {
        3
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        let s = summarize(data)?;
        require_positive(&s)?;
        let mut sorted = data.to_vec();
        sorted.sort_by(f64::total_cmp);
        let center = sorted[sorted.len() / 2].ln();
        let (lo, hi) = (center - 4.0, center + 10.0);
        let eval = |lb: f64| profile(data, lb.exp()).map_or(f64::NEG_INFINITY, |p| p.0);
        let steps = 28;
        let grid: Vec<f64> = (0..=steps)
            .map(|i| lo + (hi - lo) * i as f64 / steps as f64)
            .collect();
        let (bi, best) = grid
            .iter()
            .map(|&lb| eval(lb))
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("grid is not empty");
        if !best.is_finite() {
            return Err("não convergiu".into());
        }
        let a = grid[bi.saturating_sub(1)];
        let b = grid[(bi + 1).min(steps)];
        let refined = golden_min(|lb| -eval(lb), a, b, 40);
        let lb = if eval(refined) >= best {
            refined
        } else {
            grid[bi]
        };
        let beta = lb.exp();
        let (_, a1, a2) = profile(data, beta).ok_or("não convergiu")?;
        Ok(fitted(
            self.name(),
            vec![("forma1", a1), ("forma2", a2), ("escala", beta)],
            PearsonVILaw {
                a1,
                a2,
                scale: beta,
            },
            Dist::PearsonVI {
                shape1: a1,
                shape2: a2,
                scale: beta,
            },
        ))
    }
}
