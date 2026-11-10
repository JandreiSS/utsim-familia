//! The contract every candidate distribution implements.

use utsim_core::Dist;
use utsim_core::rng::SimRng;

/// A fitted distribution, as needed by the tests and plots.
pub trait FittedModel: Send + Sync {
    fn cdf(&self, x: f64) -> f64;
    /// Density for continuous families, probability mass for discrete ones.
    fn pdf(&self, x: f64) -> f64;
    fn sample(&self, rng: &mut SimRng) -> f64;

    /// Inverse CDF, used by Q-Q plots. Default: bisection on [`FittedModel::cdf`]. The
    /// bracket stops doubling when it leaves the finite numbers (at most about 1024 steps on
    /// each side), so a model whose CDF never reaches `p` gives an infinite end, not a loop.
    fn quantile(&self, p: f64) -> f64 {
        let (mut lo, mut hi): (f64, f64) = (-1.0, 1.0);
        while lo.is_finite() && self.cdf(lo) > p {
            lo *= 2.0;
        }
        while hi.is_finite() && self.cdf(hi) < p {
            hi *= 2.0;
        }
        if !lo.is_finite() || !hi.is_finite() {
            return if hi.is_finite() { lo } else { hi };
        }
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if self.cdf(mid) < p {
                lo = mid
            } else {
                hi = mid
            }
        }
        0.5 * (lo + hi)
    }
}

/// A candidate family of distributions (Exponential, Weibull, ...).
pub trait Family: Send + Sync {
    /// Name shown to the user, in Portuguese ("Exponencial", "Weibull"...).
    fn name(&self) -> &'static str;
    /// Number of parameters estimated from the data (used by AIC/BIC and χ² d.o.f.).
    fn n_params(&self) -> usize;
    fn is_discrete(&self) -> bool {
        false
    }
    /// Maximum-likelihood fit. `Err` explains why the family does not apply
    /// (e.g. "dados negativos", "não convergiu"), and is shown in the report.
    fn fit(&self, data: &[f64]) -> Result<Fitted, String>;
}

/// Result of fitting one family to the data.
pub struct Fitted {
    pub family: &'static str,
    /// Parameter names and estimates, in display order.
    pub params: Vec<(&'static str, f64)>,
    pub n_params: usize,
    pub model: Box<dyn FittedModel>,
    /// Equivalent UTSim distribution, ready for a `.ron` model (`None` if not representable).
    pub dist: Option<Dist>,
}

impl Fitted {
    pub fn log_likelihood(&self, data: &[f64]) -> f64 {
        data.iter()
            .map(|&x| self.model.pdf(x).max(f64::MIN_POSITIVE).ln())
            .sum()
    }

    pub fn aic(&self, data: &[f64]) -> f64 {
        2.0 * self.n_params as f64 - 2.0 * self.log_likelihood(data)
    }

    pub fn bic(&self, data: &[f64]) -> f64 {
        (data.len() as f64).ln() * self.n_params as f64 - 2.0 * self.log_likelihood(data)
    }
}
