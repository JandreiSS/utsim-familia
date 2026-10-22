//! Probability distributions used for times and label values.

use rand::Rng;
use rand_distr::{Beta, Binomial, Distribution, Gamma, Poisson, StandardNormal};
use serde::{Deserialize, Serialize};

use crate::rng::SimRng;

/// A distribution as written in a model file, e.g. `Exp(mean: 2.0)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Dist {
    Const(f64),
    Uniform {
        min: f64,
        max: f64,
    },
    Exp {
        mean: f64,
    },
    Triangular {
        min: f64,
        mode: f64,
        max: f64,
    },
    Normal {
        mean: f64,
        sd: f64,
    },
    /// Parameterised by the mean and standard deviation of the variable itself.
    LogNormal {
        mean: f64,
        sd: f64,
    },
    Gamma {
        shape: f64,
        scale: f64,
    },
    Erlang {
        k: u32,
        mean: f64,
    },
    Weibull {
        shape: f64,
        scale: f64,
    },
    /// `(value, weight)` pairs; weights need not sum to 1.
    Discrete(Vec<(f64, f64)>),
    /// Beta distribution rescaled to `[min, max]`.
    Beta {
        alpha: f64,
        beta: f64,
        min: f64,
        max: f64,
    },
    /// Log-logistic: `P(X <= x) = 1 / (1 + (x / scale)^-shape)`.
    LogLogistic {
        shape: f64,
        scale: f64,
    },
    /// Pearson type V (inverse gamma): `X = scale / G`, `G ~ Gamma(shape, 1)`.
    PearsonV {
        shape: f64,
        scale: f64,
    },
    /// Pearson type VI (beta prime): `X = scale * G1 / G2`, `Gi ~ Gamma(shapei, 1)`.
    PearsonVI {
        shape1: f64,
        shape2: f64,
        scale: f64,
    },
    /// Adds a constant location to another distribution (`offset + dist`).
    Shifted {
        offset: f64,
        dist: Box<Dist>,
    },
    /// Integer-valued, support 0, 1, 2...
    Poisson {
        mean: f64,
    },
    /// Number of failures before the first success, support 0, 1, 2...
    Geometric {
        p: f64,
    },
    Binomial {
        n: u32,
        p: f64,
    },
    /// Number of failures before the `r`-th success (`r` need not be an integer).
    NegBinomial {
        r: f64,
        p: f64,
    },
    /// Continuous empirical distribution: the observations, in ascending order, are
    /// joined by a piecewise-linear ECDF (each gap between neighbours is equally likely).
    Empirical(Vec<f64>),
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
            Dist::Beta {
                alpha,
                beta,
                min,
                max,
            } => {
                let y: f64 = Beta::new(*alpha, *beta)
                    .expect("beta parameters are validated before the run")
                    .sample(rng);
                min + (max - min) * y
            }
            Dist::LogLogistic { shape, scale } => {
                let u = rng.random::<f64>();
                scale * (u / (1.0 - u)).powf(1.0 / shape)
            }
            Dist::PearsonV { shape, scale } => {
                let g: f64 = Gamma::new(*shape, 1.0)
                    .expect("pearson V parameters are validated before the run")
                    .sample(rng);
                scale / g
            }
            Dist::PearsonVI {
                shape1,
                shape2,
                scale,
            } => {
                let g1: f64 = Gamma::new(*shape1, 1.0)
                    .expect("pearson VI parameters are validated before the run")
                    .sample(rng);
                let g2: f64 = Gamma::new(*shape2, 1.0)
                    .expect("pearson VI parameters are validated before the run")
                    .sample(rng);
                scale * g1 / g2
            }
            Dist::Shifted { offset, dist } => offset + dist.sample(rng),
            Dist::Poisson { mean } => sample_poisson(*mean, rng),
            Dist::Geometric { p } => {
                if *p >= 1.0 {
                    0.0
                } else {
                    let u = 1.0 - rng.random::<f64>(); // (0, 1]
                    (u.ln() / (-p).ln_1p()).floor()
                }
            }
            Dist::Binomial { n, p } => Binomial::new(u64::from(*n), *p)
                .expect("binomial parameters are validated before the run")
                .sample(rng) as f64,
            Dist::NegBinomial { r, p } => {
                if *p >= 1.0 {
                    0.0
                } else {
                    // Gamma-Poisson mixture.
                    let lambda: f64 = Gamma::new(*r, (1.0 - p) / p)
                        .expect("negative binomial parameters are validated before the run")
                        .sample(rng);
                    sample_poisson(lambda, rng)
                }
            }
            Dist::Empirical(xs) => {
                let n = xs.len();
                if n == 1 {
                    return xs[0];
                }
                let pos = rng.random::<f64>() * (n - 1) as f64;
                let i = (pos.floor() as usize).min(n - 2);
                xs[i] + (pos - i as f64) * (xs[i + 1] - xs[i])
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
            Dist::Beta {
                alpha,
                beta,
                min,
                max,
            } => min + (max - min) * alpha / (alpha + beta),
            Dist::LogLogistic { shape, scale } => {
                if *shape > 1.0 {
                    let x = std::f64::consts::PI / shape;
                    scale * x / x.sin()
                } else {
                    f64::INFINITY
                }
            }
            Dist::PearsonV { shape, scale } => {
                if *shape > 1.0 {
                    scale / (shape - 1.0)
                } else {
                    f64::INFINITY
                }
            }
            Dist::PearsonVI {
                shape1,
                shape2,
                scale,
            } => {
                if *shape2 > 1.0 {
                    scale * shape1 / (shape2 - 1.0)
                } else {
                    f64::INFINITY
                }
            }
            Dist::Shifted { offset, dist } => offset + dist.mean(),
            Dist::Poisson { mean } => *mean,
            Dist::Geometric { p } => (1.0 - p) / p,
            Dist::Binomial { n, p } => f64::from(*n) * p,
            Dist::NegBinomial { r, p } => r * (1.0 - p) / p,
            Dist::Empirical(xs) => {
                if xs.len() == 1 {
                    xs[0]
                } else {
                    // Each of the n-1 gaps is uniform and equally likely.
                    xs.windows(2).map(|w| (w[0] + w[1]) / 2.0).sum::<f64>() / (xs.len() - 1) as f64
                }
            }
        }
    }

    /// Checks parameters; the message is shown to the model author.
    pub fn validate(&self) -> Result<(), String> {
        let ok = |cond: bool, msg: &str| if cond { Ok(()) } else { Err(msg.to_string()) };
        let finite = |xs: &[f64]| xs.iter().all(|x| x.is_finite());
        match self {
            Dist::Const(v) => ok(v.is_finite(), "Const precisa de um valor finito"),
            Dist::Uniform { min, max } => ok(
                finite(&[*min, *max]) && min <= max,
                "Uniform exige min <= max",
            ),
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
                    && table
                        .iter()
                        .all(|(v, w)| v.is_finite() && w.is_finite() && *w >= 0.0)
                    && table.iter().any(|(_, w)| *w > 0.0),
                "Discrete exige ao menos um par (valor, peso) com peso > 0",
            ),
            Dist::Beta {
                alpha,
                beta,
                min,
                max,
            } => ok(
                finite(&[*alpha, *beta, *min, *max]) && *alpha > 0.0 && *beta > 0.0 && min <= max,
                "Beta exige alpha > 0, beta > 0 e min <= max",
            ),
            Dist::LogLogistic { shape, scale } => ok(
                finite(&[*shape, *scale]) && *shape > 0.0 && *scale > 0.0,
                "LogLogistic exige shape > 0 e scale > 0",
            ),
            Dist::PearsonV { shape, scale } => ok(
                finite(&[*shape, *scale]) && *shape > 0.0 && *scale > 0.0,
                "PearsonV exige shape > 0 e scale > 0",
            ),
            Dist::PearsonVI {
                shape1,
                shape2,
                scale,
            } => ok(
                finite(&[*shape1, *shape2, *scale])
                    && *shape1 > 0.0
                    && *shape2 > 0.0
                    && *scale > 0.0,
                "PearsonVI exige shape1 > 0, shape2 > 0 e scale > 0",
            ),
            Dist::Shifted { offset, dist } => {
                ok(offset.is_finite(), "Shifted exige um offset finito")?;
                dist.validate()
            }
            Dist::Poisson { mean } => ok(mean.is_finite() && *mean > 0.0, "Poisson exige mean > 0"),
            Dist::Geometric { p } => ok(
                p.is_finite() && *p > 0.0 && *p <= 1.0,
                "Geometric exige 0 < p <= 1",
            ),
            Dist::Binomial { n, p } => ok(
                *n > 0 && p.is_finite() && (0.0..=1.0).contains(p),
                "Binomial exige n > 0 e 0 <= p <= 1",
            ),
            Dist::NegBinomial { r, p } => ok(
                finite(&[*r, *p]) && *r > 0.0 && *p > 0.0 && *p <= 1.0,
                "NegBinomial exige r > 0 e 0 < p <= 1",
            ),
            Dist::Empirical(xs) => ok(
                !xs.is_empty() && finite(xs) && xs.windows(2).all(|w| w[0] <= w[1]),
                "Empirical exige ao menos uma observação finita, em ordem crescente",
            ),
        }
    }
}

/// Poisson sample as `f64`; a zero (or invalid) mean always yields 0.
fn sample_poisson(mean: f64, rng: &mut SimRng) -> f64 {
    match Poisson::new(mean) {
        Ok(d) => d.sample(rng),
        Err(_) => 0.0,
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
            Dist::Triangular {
                min: 1.0,
                mode: 2.0,
                max: 6.0,
            },
            Dist::Normal {
                mean: 10.0,
                sd: 2.0,
            },
            Dist::LogNormal { mean: 4.0, sd: 1.5 },
            Dist::Gamma {
                shape: 2.0,
                scale: 1.5,
            },
            Dist::Erlang { k: 3, mean: 6.0 },
            Dist::Weibull {
                shape: 1.5,
                scale: 2.0,
            },
            Dist::Discrete(vec![(1.0, 0.25), (2.0, 0.75)]),
            Dist::Beta {
                alpha: 2.0,
                beta: 5.0,
                min: 1.0,
                max: 4.0,
            },
            Dist::LogLogistic {
                shape: 6.0,
                scale: 2.0,
            },
            Dist::PearsonV {
                shape: 8.0,
                scale: 14.0,
            },
            Dist::PearsonVI {
                shape1: 3.0,
                shape2: 10.0,
                scale: 2.0,
            },
            Dist::Shifted {
                offset: 5.0,
                dist: Box::new(Dist::Weibull {
                    shape: 1.5,
                    scale: 2.0,
                }),
            },
            Dist::Poisson { mean: 4.0 },
            Dist::Geometric { p: 0.3 },
            Dist::Binomial { n: 10, p: 0.3 },
            Dist::NegBinomial { r: 2.5, p: 0.4 },
            Dist::Empirical(vec![1.0, 2.0, 4.0, 8.0]),
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
        assert!(Dist::Geometric { p: 0.0 }.validate().is_err());
        assert!(Dist::Binomial { n: 5, p: 1.5 }.validate().is_err());
        assert!(Dist::Empirical(vec![2.0, 1.0]).validate().is_err());
        assert!(
            Dist::Shifted {
                offset: 1.0,
                dist: Box::new(Dist::Exp { mean: -1.0 })
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn new_variants_stay_in_support() {
        let d = Dist::Shifted {
            offset: 2.0,
            dist: Box::new(Dist::PearsonVI {
                shape1: 2.0,
                shape2: 3.0,
                scale: 1.0,
            }),
        };
        let mut rng = stream(3, 0, 0);
        for _ in 0..1000 {
            assert!(d.sample(&mut rng) >= 2.0);
            let g = Dist::Geometric { p: 0.4 }.sample(&mut rng);
            assert!(g >= 0.0 && g.fract() == 0.0);
            let b = Dist::Binomial { n: 6, p: 0.5 }.sample(&mut rng);
            assert!((0.0..=6.0).contains(&b));
            let e = Dist::Empirical(vec![1.0, 2.0, 4.0]).sample(&mut rng);
            assert!((1.0..=4.0).contains(&e));
        }
    }
}
