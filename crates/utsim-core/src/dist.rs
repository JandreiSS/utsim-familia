//! Probability distributions used for times and label values.

use rand::Rng;
use rand_distr::{Distribution, Gamma, StandardNormal};
use serde::{Deserialize, Serialize};

use crate::rng::SimRng;

/// A distribution as written in a model file, e.g. `Exp(mean: 2.0)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Dist {
    Const(f64),
    Uniform { min: f64, max: f64 },
    Exp { mean: f64 },
    Triangular { min: f64, mode: f64, max: f64 },
    Normal { mean: f64, sd: f64 },
    /// Parameterised by the mean and standard deviation of the variable itself.
    LogNormal { mean: f64, sd: f64 },
    Gamma { shape: f64, scale: f64 },
    Erlang { k: u32, mean: f64 },
    Weibull { shape: f64, scale: f64 },
    /// `(value, weight)` pairs; weights need not sum to 1.
    Discrete(Vec<(f64, f64)>),
}

impl Dist {
    pub fn sample(&self, rng: &mut SimRng) -> f64 {
        match self {
            Dist::Const(v) => *v,
            Dist::Uniform { min, max } => min + (max - min) * rng.random::<f64>(),
            Dist::Exp { mean } => -mean * (1.0 - rng.random::<f64>()).ln(),
            Dist::Triangular { min, mode, max } => {
                if max <= min {
                    return *min;
                }
                let u = rng.random::<f64>();
                let range = max - min;
                if u < (mode - min) / range {
                    min + (range * (mode - min) * u).sqrt()
                } else {
                    max - (range * (max - mode) * (1.0 - u)).sqrt()
                }
            }
            Dist::Normal { mean, sd } => {
                let z: f64 = rng.sample(StandardNormal);
                mean + sd * z
            }
            Dist::LogNormal { mean, sd } => {
                let s2 = (1.0 + (sd / mean).powi(2)).ln();
                let mu = mean.ln() - s2 / 2.0;
                let z: f64 = rng.sample(StandardNormal);
                (mu + s2.sqrt() * z).exp()
            }
            Dist::Gamma { shape, scale } => Gamma::new(*shape, *scale)
                .expect("gamma parameters are validated before the run")
                .sample(rng),
            Dist::Erlang { k, mean } => {
                let k = f64::from(*k);
                Gamma::new(k, mean / k)
                    .expect("erlang parameters are validated before the run")
                    .sample(rng)
            }
            Dist::Weibull { shape, scale } => {
                scale * (-(1.0 - rng.random::<f64>()).ln()).powf(1.0 / shape)
            }
            Dist::Discrete(table) => {
                let total: f64 = table.iter().map(|(_, w)| w).sum();
                let mut x = rng.random::<f64>() * total;
                for (value, weight) in table {
                    if x < *weight {
                        return *value;
                    }
                    x -= weight;
                }
                table.last().map_or(0.0, |(v, _)| *v)
            }
        }
    }

    /// Theoretical mean, used by tests and reports.
    pub fn mean(&self) -> f64 {
        match self {
            Dist::Const(v) => *v,
            Dist::Uniform { min, max } => (min + max) / 2.0,
            Dist::Exp { mean } | Dist::LogNormal { mean, .. } | Dist::Erlang { mean, .. } => *mean,
            Dist::Triangular { min, mode, max } => (min + mode + max) / 3.0,
            Dist::Normal { mean, .. } => *mean,
            Dist::Gamma { shape, scale } => shape * scale,
            Dist::Weibull { shape, scale } => scale * gamma_fn(1.0 + 1.0 / shape),
            Dist::Discrete(table) => {
                let total: f64 = table.iter().map(|(_, w)| w).sum();
                table.iter().map(|(v, w)| v * w).sum::<f64>() / total
            }
        }
    }

    /// Checks parameters; the message is shown to the model author.
    pub fn validate(&self) -> Result<(), String> {
        let ok = |cond: bool, msg: &str| if cond { Ok(()) } else { Err(msg.to_string()) };
        let finite = |xs: &[f64]| xs.iter().all(|x| x.is_finite());
        match self {
            Dist::Const(v) => ok(v.is_finite(), "Const precisa de um valor finito"),
            Dist::Uniform { min, max } => {
                ok(finite(&[*min, *max]) && min <= max, "Uniform exige min <= max")
            }
            Dist::Exp { mean } => ok(mean.is_finite() && *mean > 0.0, "Exp exige mean > 0"),
            Dist::Triangular { min, mode, max } => ok(
                finite(&[*min, *mode, *max]) && min <= mode && mode <= max,
                "Triangular exige min <= mode <= max",
            ),
            Dist::Normal { mean, sd } => {
                ok(finite(&[*mean, *sd]) && *sd >= 0.0, "Normal exige sd >= 0")
            }
            Dist::LogNormal { mean, sd } => ok(
                finite(&[*mean, *sd]) && *mean > 0.0 && *sd >= 0.0,
                "LogNormal exige mean > 0 e sd >= 0",
            ),
            Dist::Gamma { shape, scale } => ok(
                finite(&[*shape, *scale]) && *shape > 0.0 && *scale > 0.0,
                "Gamma exige shape > 0 e scale > 0",
            ),
            Dist::Erlang { k, mean } => ok(
                *k > 0 && mean.is_finite() && *mean > 0.0,
                "Erlang exige k > 0 e mean > 0",
            ),
            Dist::Weibull { shape, scale } => ok(
                finite(&[*shape, *scale]) && *shape > 0.0 && *scale > 0.0,
                "Weibull exige shape > 0 e scale > 0",
            ),
            Dist::Discrete(table) => ok(
                !table.is_empty()
                    && table.iter().all(|(v, w)| v.is_finite() && w.is_finite() && *w >= 0.0)
                    && table.iter().any(|(_, w)| *w > 0.0),
                "Discrete exige ao menos um par (valor, peso) com peso > 0",
            ),
        }
    }
}

/// Lanczos approximation of the gamma function (only needed for the Weibull mean).
fn gamma_fn(x: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma_fn(1.0 - x))
    } else {
        let x = x - 1.0;
        let mut a = C[0];
        let t = x + G + 0.5;
        for (i, c) in C.iter().enumerate().skip(1) {
            a += c / (x + i as f64);
        }
        (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::stream;

    fn sample_mean(d: &Dist, n: usize) -> f64 {
        let mut rng = stream(7, 0, 0);
        (0..n).map(|_| d.sample(&mut rng)).sum::<f64>() / n as f64
    }

    #[test]
    fn sample_means_match_theory() {
        let cases = [
            Dist::Const(3.0),
            Dist::Uniform { min: 1.0, max: 5.0 },
            Dist::Exp { mean: 2.0 },
            Dist::Triangular { min: 1.0, mode: 2.0, max: 6.0 },
            Dist::Normal { mean: 10.0, sd: 2.0 },
            Dist::LogNormal { mean: 4.0, sd: 1.5 },
            Dist::Gamma { shape: 2.0, scale: 1.5 },
            Dist::Erlang { k: 3, mean: 6.0 },
            Dist::Weibull { shape: 1.5, scale: 2.0 },
            Dist::Discrete(vec![(1.0, 0.25), (2.0, 0.75)]),
        ];
        for d in &cases {
            d.validate().unwrap();
            let m = sample_mean(d, 200_000);
            let rel = (m - d.mean()).abs() / d.mean();
            assert!(rel < 0.01, "{d:?}: sample mean {m} vs {}", d.mean());
        }
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(Dist::Exp { mean: 0.0 }.validate().is_err());
        assert!(Dist::Uniform { min: 2.0, max: 1.0 }.validate().is_err());
        assert!(Dist::Discrete(vec![]).validate().is_err());
    }
}
