//! Classic (analytic) PERT: three-point estimates `(a, m, b)` from the model's
//! distributions, beta moments or the exact moments, and the probability of meeting a
//! deadline on the critical path.
//!
//! Three-point estimates: `Const(c)` gives `a = m = b = c`; `Triangular` gives the three
//! directly; `Uniform` gives `m = (a + b) / 2`; `Beta` on `[min, max]` uses its bounds and
//! mode. Every other distribution uses its 1% and 99% quantiles and its mode (when the
//! mode is not known in closed form, the `m` that makes `(a + 4m + b) / 6` equal the exact
//! mean). Quantiles without a closed form come from 20 001 fixed draws (always the same
//! stream), so the estimates are reproducible.

use serde::Serialize;
use utsim_core::Dist;
use utsim_core::rng;

use crate::cpm::{self, Schedule};
use crate::math::{gamma_fn, norm_cdf, norm_inv, quantile_sorted};
use crate::network::{Activity, ActivityNetwork};

/// Which mean and variance an activity gets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Moments {
    /// The distribution's own mean and variance (default).
    #[default]
    Exact,
    /// Beta approximation: `te = (a + 4m + b) / 6`, `σ² = ((b - a) / 6)²` (for teaching).
    Classic,
}

/// Three-point estimate and moments of one duration.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Estimate {
    /// Optimistic, most likely and pessimistic durations.
    pub a: f64,
    pub m: f64,
    pub b: f64,
    /// Exact mean and variance of the distribution.
    pub mean: f64,
    pub variance: f64,
}

impl Estimate {
    /// `(a + 4m + b) / 6`.
    pub fn te(&self) -> f64 {
        (self.a + 4.0 * self.m + self.b) / 6.0
    }

    /// `((b - a) / 6)²`.
    pub fn classic_variance(&self) -> f64 {
        ((self.b - self.a) / 6.0).powi(2)
    }

    /// Mean and variance under `moments`; heavy tails without a finite exact moment fall
    /// back to the classic formula.
    pub fn moments(&self, moments: Moments) -> (f64, f64) {
        match moments {
            Moments::Exact if self.mean.is_finite() && self.variance.is_finite() => {
                (self.mean, self.variance)
            }
            _ => (self.te(), self.classic_variance()),
        }
    }

    fn constant(c: f64) -> Self {
        Estimate {
            a: c,
            m: c,
            b: c,
            mean: c,
            variance: 0.0,
        }
    }
}

/// Estimate of one distribution.
pub fn estimate(d: &Dist) -> Estimate {
    let mean = d.mean();
    let variance = variance(d);
    let (a, m, b) = match d {
        Dist::Const(c) => (*c, *c, *c),
        Dist::Triangular { min, mode, max } => (*min, *mode, *max),
        Dist::Uniform { min, max } => (*min, (min + max) / 2.0, *max),
        Dist::Beta {
            alpha,
            beta,
            min,
            max,
        } => {
            let m = if *alpha > 1.0 && *beta > 1.0 {
                min + (max - min) * (alpha - 1.0) / (alpha + beta - 2.0)
            } else {
                f64::NAN
            };
            (*min, m, *max)
        }
        Dist::Shifted { offset, dist } => {
            let e = estimate(dist);
            (offset + e.a, offset + e.m, offset + e.b)
        }
        _ => (quantile(d, 0.01), mode(d), quantile(d, 0.99)),
    };
    let m = if m.is_finite() {
        m
    } else {
        (6.0 * mean - a - b) / 4.0
    };
    Estimate {
        a,
        m: m.clamp(a.min(b), b.max(a)),
        b,
        mean,
        variance,
    }
}

/// Estimate of an activity: sums of the parts (independent), scaled by its factor.
pub fn activity_estimate(act: &Activity) -> Estimate {
    let mut e = act
        .parts
        .iter()
        .map(estimate)
        .fold(Estimate::constant(0.0), |s, x| Estimate {
            a: s.a + x.a,
            m: s.m + x.m,
            b: s.b + x.b,
            mean: s.mean + x.mean,
            variance: s.variance + x.variance,
        });
    let f = act.factor;
    e.a *= f;
    e.m *= f;
    e.b *= f;
    e.mean *= f;
    e.variance *= f * f;
    e
}

/// Variance of a distribution (`INFINITY` when it does not exist).
pub fn variance(d: &Dist) -> f64 {
    match d {
        Dist::Const(_) => 0.0,
        Dist::Uniform { min, max } => (max - min).powi(2) / 12.0,
        Dist::Exp { mean } => mean * mean,
        Dist::Triangular { min, mode, max } => {
            let (a, c, b) = (*min, *mode, *max);
            (a * a + b * b + c * c - a * b - a * c - b * c) / 18.0
        }
        Dist::Normal { sd, .. } | Dist::LogNormal { sd, .. } => sd * sd,
        Dist::Gamma { shape, scale } => shape * scale * scale,
        Dist::Erlang { k, mean } => mean * mean / f64::from(*k),
        Dist::Weibull { shape, scale } => {
            let g1 = gamma_fn(1.0 + 1.0 / shape);
            scale * scale * (gamma_fn(1.0 + 2.0 / shape) - g1 * g1)
        }
        Dist::Discrete(table) => {
            let total: f64 = table.iter().map(|(_, w)| w).sum();
            let m = d.mean();
            table.iter().map(|(v, w)| w * (v - m).powi(2)).sum::<f64>() / total
        }
        Dist::Beta {
            alpha,
            beta,
            min,
            max,
        } => {
            let s = alpha + beta;
            (max - min).powi(2) * alpha * beta / (s * s * (s + 1.0))
        }
        Dist::LogLogistic { shape, scale } => {
            if *shape > 2.0 {
                let b = std::f64::consts::PI / shape;
                scale * scale * (2.0 * b / (2.0 * b).sin() - (b / b.sin()).powi(2))
            } else {
                f64::INFINITY
            }
        }
        Dist::PearsonV { shape, scale } => {
            if *shape > 2.0 {
                scale * scale / ((shape - 1.0).powi(2) * (shape - 2.0))
            } else {
                f64::INFINITY
            }
        }
        Dist::PearsonVI {
            shape1,
            shape2,
            scale,
        } => {
            if *shape2 > 2.0 {
                scale * scale * shape1 * (shape1 + shape2 - 1.0)
                    / ((shape2 - 1.0).powi(2) * (shape2 - 2.0))
            } else {
                f64::INFINITY
            }
        }
        Dist::Shifted { dist, .. } => variance(dist),
        Dist::Poisson { mean } => *mean,
        Dist::Geometric { p } | Dist::NegBinomial { p, .. } => {
            let r = match d {
                Dist::NegBinomial { r, .. } => *r,
                _ => 1.0,
            };
            r * (1.0 - p) / (p * p)
        }
        Dist::Binomial { n, p } => f64::from(*n) * p * (1.0 - p),
        Dist::Empirical(xs) => {
            if xs.len() < 2 {
                0.0
            } else {
                // Mixture of equally likely uniform gaps.
                let k = (xs.len() - 1) as f64;
                let second = xs
                    .windows(2)
                    .map(|w| (w[0] * w[0] + w[0] * w[1] + w[1] * w[1]) / 3.0)
                    .sum::<f64>()
                    / k;
                (second - d.mean().powi(2)).max(0.0)
            }
        }
    }
}

/// Quantile `p` of a distribution (closed form where there is one, fixed draws otherwise).
pub fn quantile(d: &Dist, p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    match d {
        Dist::Const(c) => *c,
        Dist::Uniform { min, max } => min + p * (max - min),
        Dist::Exp { mean } => -mean * (1.0 - p).ln(),
        Dist::Triangular { min, mode, max } => {
            let range = max - min;
            if range <= 0.0 {
                *min
            } else if p < (mode - min) / range {
                min + (range * (mode - min) * p).sqrt()
            } else {
                max - (range * (max - mode) * (1.0 - p)).sqrt()
            }
        }
        Dist::Normal { mean, sd } => mean + sd * norm_inv(p),
        Dist::LogNormal { mean, sd } => {
            let s2 = (1.0 + (sd / mean).powi(2)).ln();
            (mean.ln() - s2 / 2.0 + s2.sqrt() * norm_inv(p)).exp()
        }
        Dist::Weibull { shape, scale } => scale * (-(1.0 - p).ln()).powf(1.0 / shape),
        Dist::LogLogistic { shape, scale } => scale * (p / (1.0 - p)).powf(1.0 / shape),
        Dist::Shifted { offset, dist } => offset + quantile(dist, p),
        Dist::Discrete(table) => {
            let mut sorted = table.clone();
            sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
            let total: f64 = sorted.iter().map(|(_, w)| w).sum();
            let mut acc = 0.0;
            for (v, w) in &sorted {
                acc += w / total;
                if acc >= p - 1e-12 {
                    return *v;
                }
            }
            sorted.last().map_or(f64::NAN, |(v, _)| *v)
        }
        Dist::Empirical(xs) => quantile_sorted(xs, p),
        _ => {
            let mut r = rng::stream(0x5045_5254, 0, 0);
            let mut xs: Vec<f64> = (0..20_001).map(|_| d.sample(&mut r)).collect();
            xs.sort_by(f64::total_cmp);
            quantile_sorted(&xs, p)
        }
    }
}

/// Mode in closed form, `NaN` when not known.
pub fn mode(d: &Dist) -> f64 {
    match d {
        Dist::Const(c) => *c,
        Dist::Uniform { min, max } => (min + max) / 2.0,
        Dist::Exp { .. } => 0.0,
        Dist::Triangular { mode, .. } => *mode,
        Dist::Normal { mean, .. } => *mean,
        Dist::LogNormal { mean, sd } => {
            let s2 = (1.0 + (sd / mean).powi(2)).ln();
            (mean.ln() - s2 / 2.0 - s2).exp()
        }
        Dist::Gamma { shape, scale } => (shape - 1.0).max(0.0) * scale,
        Dist::Erlang { k, mean } => f64::from(k - 1) * mean / f64::from(*k),
        Dist::Weibull { shape, scale } => {
            if *shape > 1.0 {
                scale * ((shape - 1.0) / shape).powf(1.0 / shape)
            } else {
                0.0
            }
        }
        Dist::LogLogistic { shape, scale } => {
            if *shape > 1.0 {
                scale * ((shape - 1.0) / (shape + 1.0)).powf(1.0 / shape)
            } else {
                0.0
            }
        }
        Dist::PearsonV { shape, scale } => scale / (shape + 1.0),
        Dist::PearsonVI {
            shape1,
            shape2,
            scale,
        } => {
            if *shape1 >= 1.0 {
                scale * (shape1 - 1.0) / (shape2 + 1.0)
            } else {
                0.0
            }
        }
        Dist::Shifted { offset, dist } => offset + mode(dist),
        Dist::Poisson { mean } => mean.floor(),
        Dist::Geometric { .. } => 0.0,
        Dist::Binomial { n, p } => ((f64::from(*n) + 1.0) * p).floor().min(f64::from(*n)),
        _ => f64::NAN,
    }
}

/// Result of the classic PERT.
#[derive(Clone, Debug, Serialize)]
pub struct PertResult {
    pub moments: Moments,
    pub estimates: Vec<Estimate>,
    /// Mean and variance used for each activity.
    pub means: Vec<f64>,
    pub variances: Vec<f64>,
    /// CPM on the means.
    pub schedule: Schedule,
    /// Expected project duration (sum of the means on the critical path).
    pub mean: f64,
    /// Variance of the critical path (the largest one when several tie).
    pub variance: f64,
    /// Index into `schedule.critical_paths` of the path behind `variance`.
    pub path: Option<usize>,
    pub deadline: Option<f64>,
    /// `Φ((deadline - mean) / σ)`.
    pub probability: Option<f64>,
    pub warnings: Vec<String>,
}

impl PertResult {
    pub fn sd(&self) -> f64 {
        self.variance.sqrt()
    }
}

/// Classic PERT of the network.
pub fn analyze(net: &ActivityNetwork, moments: Moments, deadline: Option<f64>) -> PertResult {
    let estimates: Vec<Estimate> = net.activities.iter().map(activity_estimate).collect();
    let (means, variances): (Vec<f64>, Vec<f64>) =
        estimates.iter().map(|e| e.moments(moments)).unzip();
    let schedule = cpm::schedule(net, &means);
    let path_var = |p: &Vec<usize>| p.iter().map(|&v| variances[v]).sum::<f64>();
    let path = (0..schedule.critical_paths.len()).max_by(|&a, &b| {
        path_var(&schedule.critical_paths[a]).total_cmp(&path_var(&schedule.critical_paths[b]))
    });
    let variance = path.map_or(0.0, |k| path_var(&schedule.critical_paths[k]));
    let mean = schedule.project;
    let sd = variance.sqrt();
    let probability = deadline.map(|t| {
        if sd > 0.0 {
            norm_cdf((t - mean) / sd)
        } else if t >= mean {
            1.0
        } else {
            0.0
        }
    });
    let mut warnings = Vec::new();
    if schedule.critical_paths.len() > 1 {
        warnings.push(format!(
            "{} caminhos críticos empatados: a variância é a do caminho de maior variância",
            schedule.critical_paths.len()
        ));
    }
    let near: Vec<&str> = (0..net.len())
        .filter(|&v| {
            !schedule.critical[v]
                && !net.activities[v].is_milestone()
                && variances[v] > 0.0
                && schedule.total_float[v] < 2.0 * sd
        })
        .map(|v| net.activities[v].name.as_str())
        .collect();
    if !near.is_empty() {
        warnings.push(format!(
            "há caminhos quase críticos em paralelo (folga menor que 2σ: {}); o PERT clássico \
             subestima o tempo total nesse caso (viés da junção), compare com o Monte Carlo",
            near.join(", ")
        ));
    }
    if moments == Moments::Exact
        && estimates
            .iter()
            .any(|e| !(e.mean.is_finite() && e.variance.is_finite()))
    {
        warnings.push(
            "alguma distribuição não tem média ou variância finita: usada a fórmula clássica nela"
                .into(),
        );
    }
    PertResult {
        moments,
        estimates,
        means,
        variances,
        schedule,
        mean,
        variance,
        path,
        deadline,
        probability,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_points_by_hand() {
        let e = estimate(&Dist::Triangular {
            min: 2.0,
            mode: 4.0,
            max: 12.0,
        });
        assert_eq!((e.a, e.m, e.b), (2.0, 4.0, 12.0));
        assert!((e.te() - 5.0).abs() < 1e-12); // (2 + 16 + 12) / 6
        assert!((e.classic_variance() - 25.0 / 9.0).abs() < 1e-12);
        assert!((e.mean - 6.0).abs() < 1e-12);
        // (4 + 144 + 16 - 24 - 8 - 48) / 18 = 84 / 18
        assert!((e.variance - 84.0 / 18.0).abs() < 1e-12);
        let c = estimate(&Dist::Const(3.0));
        assert_eq!(
            (c.a, c.m, c.b, c.te(), c.variance),
            (3.0, 3.0, 3.0, 3.0, 0.0)
        );
    }

    #[test]
    fn variances_match_samples() {
        let cases = [
            Dist::Gamma {
                shape: 2.0,
                scale: 1.5,
            },
            Dist::Weibull {
                shape: 1.5,
                scale: 2.0,
            },
            Dist::Erlang { k: 3, mean: 6.0 },
            Dist::Empirical(vec![1.0, 2.0, 4.0, 8.0]),
            Dist::LogLogistic {
                shape: 6.0,
                scale: 2.0,
            },
            Dist::PearsonVI {
                shape1: 3.0,
                shape2: 10.0,
                scale: 2.0,
            },
            Dist::NegBinomial { r: 2.5, p: 0.4 },
        ];
        for d in &cases {
            let mut r = rng::stream(9, 0, 0);
            let xs: Vec<f64> = (0..200_000).map(|_| d.sample(&mut r)).collect();
            let m = xs.iter().sum::<f64>() / xs.len() as f64;
            let v = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64;
            assert!(
                (v / variance(d) - 1.0).abs() < 0.03,
                "{d:?}: {v} vs {}",
                variance(d)
            );
        }
    }

    #[test]
    fn quantiles_in_closed_form() {
        assert!((quantile(&Dist::Exp { mean: 2.0 }, 0.5) - 2.0 * 2f64.ln()).abs() < 1e-12);
        let n = Dist::Normal {
            mean: 10.0,
            sd: 2.0,
        };
        assert!((quantile(&n, 0.99) - (10.0 + 2.0 * 2.326_348)).abs() < 1e-4);
        let g = Dist::Gamma {
            shape: 9.0,
            scale: 1.0,
        };
        // Median of Gamma(9, 1) is about 8.668.
        assert!((quantile(&g, 0.5) - 8.668).abs() < 0.05);
    }
}
