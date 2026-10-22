//! Candidate families, all fitted by maximum likelihood. The exponential is the
//! reference implementation of the [`Family`] contract; the others follow the same
//! pattern, one module per group (see each family's doc comment for its method).

mod bounded;
mod common;
mod discrete;
mod gamma;
mod logistic;
mod normal;
mod pearson6;
mod shifted;
mod weibull;

#[cfg(test)]
mod tests;

use rand::Rng;
use utsim_core::Dist;
use utsim_core::rng::SimRng;

use crate::family::{Family, Fitted, FittedModel};

pub use bounded::{Beta, Triangular, Uniform};
pub use discrete::{Binomial, Geometric, NegBinomial, Poisson};
pub use gamma::{Erlang, Gamma, PearsonV};
pub use logistic::LogLogistic;
pub use normal::{Lognormal, Normal};
pub use pearson6::PearsonVI;
pub use shifted::{ShiftedExponential, ShiftedFamily};
pub use weibull::Weibull;

/// Every candidate tested by `utsim fit`, in report order: continuous first, then discrete.
pub fn all() -> Vec<Box<dyn Family>> {
    vec![
        Box::new(Exponential),
        Box::new(Erlang),
        Box::new(Gamma),
        Box::new(Weibull),
        Box::new(Lognormal),
        Box::new(Normal),
        Box::new(Uniform),
        Box::new(Triangular),
        Box::new(Beta),
        Box::new(LogLogistic),
        Box::new(PearsonV),
        Box::new(PearsonVI),
        Box::new(ShiftedExponential),
        Box::new(ShiftedFamily::weibull()),
        Box::new(ShiftedFamily::gamma()),
        Box::new(ShiftedFamily::lognormal()),
        Box::new(Poisson),
        Box::new(Geometric),
        Box::new(Binomial),
        Box::new(NegBinomial),
    ]
}

pub struct Exponential;

pub(crate) struct ExponentialModel {
    pub(crate) mean: f64,
}

impl FittedModel for ExponentialModel {
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            1.0 - (-x / self.mean).exp()
        }
    }

    fn pdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            0.0
        } else {
            (-x / self.mean).exp() / self.mean
        }
    }

    fn sample(&self, rng: &mut SimRng) -> f64 {
        -self.mean * (1.0 - rng.random::<f64>()).ln()
    }

    fn quantile(&self, p: f64) -> f64 {
        -self.mean * (1.0 - p).ln()
    }
}

impl Family for Exponential {
    fn name(&self) -> &'static str {
        "Exponencial"
    }

    fn n_params(&self) -> usize {
        1
    }

    fn fit(&self, data: &[f64]) -> Result<Fitted, String> {
        if data.is_empty() {
            return Err("sem dados".into());
        }
        if data.iter().any(|x| !x.is_finite()) {
            return Err("há valores não finitos (NaN ou infinito)".into());
        }
        if data.iter().any(|&x| x < 0.0) {
            return Err("há valores negativos".into());
        }
        let mean = data.iter().sum::<f64>() / data.len() as f64;
        if mean <= 0.0 {
            return Err("média igual a zero".into());
        }
        Ok(Fitted {
            family: self.name(),
            params: vec![("média", mean)],
            n_params: 1,
            model: Box::new(ExponentialModel { mean }),
            dist: Some(Dist::Exp { mean }),
        })
    }
}

#[cfg(test)]
mod exponential_tests {
    use super::*;
    use utsim_core::rng::stream;

    #[test]
    fn exponential_mle_is_the_sample_mean() {
        let f = Exponential.fit(&[1.0, 2.0, 3.0]).unwrap();
        assert_eq!(f.params[0].1, 2.0);
        assert_eq!(f.dist, Some(Dist::Exp { mean: 2.0 }));
        assert!((f.model.quantile(f.model.cdf(1.7)) - 1.7).abs() < 1e-12);
        assert!(Exponential.fit(&[-1.0, 2.0]).is_err());
        let mut rng = stream(1, 0, 0);
        let m = (0..50_000).map(|_| f.model.sample(&mut rng)).sum::<f64>() / 50_000.0;
        assert!((m - 2.0).abs() < 0.05);
    }

    #[test]
    fn default_quantile_inverts_the_cdf() {
        struct Generic(ExponentialModel);
        impl FittedModel for Generic {
            fn cdf(&self, x: f64) -> f64 {
                self.0.cdf(x)
            }
            fn pdf(&self, x: f64) -> f64 {
                self.0.pdf(x)
            }
            fn sample(&self, rng: &mut SimRng) -> f64 {
                self.0.sample(rng)
            }
        }
        let g = Generic(ExponentialModel { mean: 3.0 });
        assert!((g.quantile(0.9) - g.0.quantile(0.9)).abs() < 1e-9);
    }
}
