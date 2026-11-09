//! Tolerance stack-up (`docs/plano_lean_seis_sigma.md`, 1.5, the Design phase of the
//! DMADV): a result `Y = Σ cᵢ·Xᵢ` of dimensions `Xᵢ`, each with its nominal, its lower and
//! upper tolerances and a distribution inside them. [`compute`] gives the worst case, the
//! root sum of squares, and a Monte Carlo of the stack (deterministic for a seed): mean,
//! standard deviation, percentiles, the share outside the limits of `Y`, Cp and Cpk, the
//! contribution of each dimension to the variance and a histogram as numbers.
//!
//! The types are the exchange format of the tool (serde, `Default`); the screens live in
//! the quality app and draw the numbers.

use rand::{Rng, SeedableRng};
use rand_distr::{Distribution as _, StandardNormal};
use rand_pcg::Pcg64;
use serde::{Deserialize, Serialize};

/// How a dimension varies inside its tolerance.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "tipo")]
pub enum Spread {
    /// Normal centred in the tolerance, with the half tolerance equal to `sigmas` standard
    /// deviations (3 is the usual "the tolerance is ±3σ").
    #[serde(rename = "normal")]
    Normal { sigmas: f64 },
    /// Any value inside the tolerance equally likely (a pessimistic choice without data).
    #[default]
    #[serde(rename = "uniforme")]
    Uniform,
    /// Symmetric triangle with the peak in the middle of the tolerance.
    #[serde(rename = "triangular")]
    Triangular,
}

/// One dimension of the stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Dimension {
    #[serde(rename = "nome")]
    pub name: String,
    pub nominal: f64,
    /// How far below the nominal it may go (positive number).
    #[serde(rename = "tol_inferior")]
    pub tol_lower: f64,
    /// How far above the nominal it may go (positive number).
    #[serde(rename = "tol_superior")]
    pub tol_upper: f64,
    /// Sensitivity in the stack: `1` adds, `-1` subtracts, other values for levers and
    /// angles.
    #[serde(rename = "coeficiente")]
    pub coefficient: f64,
    #[serde(rename = "distribuicao")]
    pub spread: Spread,
}

impl Default for Dimension {
    fn default() -> Self {
        Self {
            name: String::new(),
            nominal: 0.0,
            tol_lower: 0.0,
            tol_upper: 0.0,
            coefficient: 1.0,
            spread: Spread::default(),
        }
    }
}

impl Dimension {
    /// Centre and half width of the tolerance band.
    fn band(&self) -> (f64, f64) {
        let lo = self.nominal - self.tol_lower.abs();
        let hi = self.nominal + self.tol_upper.abs();
        ((lo + hi) / 2.0, (hi - lo) / 2.0)
    }

    /// Variance of the dimension (before the coefficient).
    pub fn variance(&self) -> f64 {
        let (_, h) = self.band();
        match self.spread {
            Spread::Normal { sigmas } if sigmas > 0.0 => (h / sigmas).powi(2),
            Spread::Normal { .. } => 0.0,
            Spread::Uniform => h * h / 3.0,
            Spread::Triangular => h * h / 6.0,
        }
    }

    fn draw(&self, rng: &mut Pcg64) -> f64 {
        let (c, h) = self.band();
        match self.spread {
            Spread::Normal { sigmas } if sigmas > 0.0 => {
                let z: f64 = StandardNormal.sample(rng);
                c + z * h / sigmas
            }
            Spread::Normal { .. } => c,
            Spread::Uniform => c + h * (2.0 * rng.random::<f64>() - 1.0),
            Spread::Triangular => c + h * (rng.random::<f64>() + rng.random::<f64>() - 1.0),
        }
    }
}

/// The stack: the dimensions, the limits of the result and the simulation settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stack {
    #[serde(rename = "cotas")]
    pub dimensions: Vec<Dimension>,
    /// Lower and upper specification limits of the result, when there are.
    #[serde(rename = "lie")]
    pub lsl: Option<f64>,
    #[serde(rename = "lse")]
    pub usl: Option<f64>,
    /// Number of simulated assemblies.
    #[serde(rename = "n")]
    pub samples: usize,
    #[serde(rename = "semente")]
    pub seed: u64,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            dimensions: Vec::new(),
            lsl: None,
            usl: None,
            samples: 100_000,
            seed: 1,
        }
    }
}

/// Most and least the samples may be.
pub const SAMPLES: std::ops::RangeInclusive<usize> = 1_000..=2_000_000;

/// Number of classes of the histogram.
pub const BINS: usize = 30;

/// A class of the histogram: `[from, to)` and how many assemblies fell in it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bin {
    pub from: f64,
    pub to: f64,
    pub count: usize,
}

/// The result of [`compute`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StackUp {
    /// `Σ cᵢ·nominalᵢ`.
    pub nominal: f64,
    /// Worst case: every dimension at the end of its tolerance that pushes the result down
    /// (up).
    pub worst_low: f64,
    pub worst_high: f64,
    /// Root sum of squares: `centre ± √Σ (cᵢ·halfᵢ)²`.
    pub rss_low: f64,
    pub rss_high: f64,
    /// Expected standard deviation from the distributions, `√Σ cᵢ²·Var(Xᵢ)`.
    pub sd_expected: f64,
    /// Of the simulation.
    pub mean: f64,
    pub sd: f64,
    pub min: f64,
    pub max: f64,
    /// Percentiles 0.135 %, 50 % and 99.865 % (the ±3σ of a normal).
    pub p00135: f64,
    pub p50: f64,
    pub p99865: f64,
    /// Share of the simulated assemblies below the LSL and above the USL, in %.
    pub below_pct: Option<f64>,
    pub above_pct: Option<f64>,
    /// Outside the limits, in parts per million.
    pub ppm: Option<f64>,
    pub cp: Option<f64>,
    pub cpk: Option<f64>,
    /// Share of each dimension in the variance of the result, in %, in input order.
    pub contribution_pct: Vec<f64>,
    pub histogram: Vec<Bin>,
    pub samples: usize,
    /// What the screen should point out.
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Warning {
    /// No dimension in the stack.
    NoDimensions,
    /// A dimension without tolerance (index).
    NoTolerance(usize),
    /// LSL above USL.
    LimitsSwapped,
    /// The worst case is outside the limits even though the simulation may not show it.
    WorstCaseOutside,
}

impl Warning {
    /// The text in Portuguese (the quality app may use its own catalogue).
    pub fn text(&self, dims: &[Dimension]) -> String {
        match self {
            Warning::NoDimensions => "Acrescente as cotas do empilhamento.".into(),
            Warning::NoTolerance(i) => format!(
                "A cota {} está sem tolerância e entra como fixa.",
                dims.get(*i).map_or("?", |d| d.name.as_str())
            ),
            Warning::LimitsSwapped => "O LIE está acima do LSE.".into(),
            Warning::WorstCaseOutside => {
                "No pior caso o resultado sai dos limites; a simulação diz com que frequência."
                    .into()
            }
        }
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = p * (sorted.len() - 1) as f64;
    let i = pos.floor() as usize;
    let j = (i + 1).min(sorted.len() - 1);
    sorted[i] + (sorted[j] - sorted[i]) * (pos - i as f64)
}

/// The stack-up. Pure and deterministic for `stack.seed`.
pub fn compute(stack: &Stack) -> StackUp {
    let dims = &stack.dimensions;
    let mut warnings = Vec::new();
    if dims.is_empty() {
        warnings.push(Warning::NoDimensions);
    }
    for (i, d) in dims.iter().enumerate() {
        if d.tol_lower.abs() + d.tol_upper.abs() == 0.0 {
            warnings.push(Warning::NoTolerance(i));
        }
    }
    if let (Some(l), Some(u)) = (stack.lsl, stack.usl)
        && l > u
    {
        warnings.push(Warning::LimitsSwapped);
    }
    let nominal: f64 = dims.iter().map(|d| d.coefficient * d.nominal).sum();
    let (mut worst_low, mut worst_high) = (0.0, 0.0);
    let mut centre = 0.0;
    let mut rss2 = 0.0;
    for d in dims {
        let lo = d.coefficient * (d.nominal - d.tol_lower.abs());
        let hi = d.coefficient * (d.nominal + d.tol_upper.abs());
        worst_low += lo.min(hi);
        worst_high += lo.max(hi);
        let (c, h) = d.band();
        centre += d.coefficient * c;
        rss2 += (d.coefficient * h).powi(2);
    }
    let rss = rss2.sqrt();
    let parts: Vec<f64> = dims
        .iter()
        .map(|d| d.coefficient.powi(2) * d.variance())
        .collect();
    let var_expected: f64 = parts.iter().sum();
    let contribution_pct = parts
        .iter()
        .map(|p| {
            if var_expected > 0.0 {
                p / var_expected * 100.0
            } else {
                0.0
            }
        })
        .collect();
    let outside_worst =
        stack.lsl.is_some_and(|l| worst_low < l) || stack.usl.is_some_and(|u| worst_high > u);
    if outside_worst && !dims.is_empty() {
        warnings.push(Warning::WorstCaseOutside);
    }

    let n = stack.samples.clamp(*SAMPLES.start(), *SAMPLES.end());
    let mut rng = Pcg64::seed_from_u64(stack.seed);
    let mut ys: Vec<f64> = (0..n)
        .map(|_| {
            dims.iter()
                .map(|d| d.coefficient * d.draw(&mut rng))
                .sum::<f64>()
        })
        .collect();
    let mean = ys.iter().sum::<f64>() / n as f64;
    let sd = (ys.iter().map(|y| (y - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0)).sqrt();
    let below = stack
        .lsl
        .map(|l| ys.iter().filter(|y| **y < l).count() as f64 / n as f64 * 100.0);
    let above = stack
        .usl
        .map(|u| ys.iter().filter(|y| **y > u).count() as f64 / n as f64 * 100.0);
    let ppm = match (below, above) {
        (None, None) => None,
        (b, a) => Some((b.unwrap_or(0.0) + a.unwrap_or(0.0)) * 10_000.0),
    };
    let cp = match (stack.lsl, stack.usl) {
        (Some(l), Some(u)) if sd > 0.0 => Some((u - l) / (6.0 * sd)),
        _ => None,
    };
    let cpk = if sd > 0.0 {
        let lo = stack.lsl.map(|l| (mean - l) / (3.0 * sd));
        let hi = stack.usl.map(|u| (u - mean) / (3.0 * sd));
        match (lo, hi) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    } else {
        None
    };
    ys.sort_by(f64::total_cmp);
    let (min, max) = (ys[0], ys[n - 1]);
    let width = (max - min) / BINS as f64;
    let histogram = if width > 0.0 {
        let mut counts = vec![0usize; BINS];
        for y in &ys {
            let k = (((y - min) / width) as usize).min(BINS - 1);
            counts[k] += 1;
        }
        counts
            .into_iter()
            .enumerate()
            .map(|(k, count)| Bin {
                from: min + k as f64 * width,
                to: min + (k + 1) as f64 * width,
                count,
            })
            .collect()
    } else {
        vec![Bin {
            from: min,
            to: max,
            count: n,
        }]
    };
    StackUp {
        nominal,
        worst_low,
        worst_high,
        rss_low: centre - rss,
        rss_high: centre + rss,
        sd_expected: var_expected.sqrt(),
        mean,
        sd,
        min,
        max,
        p00135: percentile(&ys, 0.00135),
        p50: percentile(&ys, 0.5),
        p99865: percentile(&ys, 0.99865),
        below_pct: below,
        above_pct: above,
        ppm,
        cp,
        cpk,
        contribution_pct,
        histogram,
        samples: n,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dim(name: &str, nominal: f64, tol: f64, c: f64, spread: Spread) -> Dimension {
        Dimension {
            name: name.into(),
            nominal,
            tol_lower: tol,
            tol_upper: tol,
            coefficient: c,
            spread,
        }
    }

    /// A shaft in a housing: gap = housing − part A − part B.
    fn gap(spread: Spread) -> Stack {
        Stack {
            dimensions: vec![
                dim("Carcaça", 50.0, 0.3, 1.0, spread),
                dim("Peça A", 20.0, 0.2, -1.0, spread),
                dim("Peça B", 29.5, 0.2, -1.0, spread),
            ],
            lsl: Some(0.1),
            usl: Some(0.9),
            samples: 200_000,
            seed: 7,
        }
    }

    #[test]
    fn worst_case_and_rss_by_hand() {
        let r = compute(&gap(Spread::Normal { sigmas: 3.0 }));
        // Nominal 50 − 20 − 29,5 = 0,5; worst case ±0,7; RSS √(0,09 + 0,04 + 0,04) = 0,4123.
        assert!((r.nominal - 0.5).abs() < 1e-12);
        assert!((r.worst_low + 0.2).abs() < 1e-12);
        assert!((r.worst_high - 1.2).abs() < 1e-12);
        let rss = 0.17f64.sqrt();
        assert!((r.rss_high - (0.5 + rss)).abs() < 1e-12);
        assert!(r.warnings.contains(&Warning::WorstCaseOutside));
        // σ expected = 0,4123 / 3 = 0,1374; contributions 0,09 : 0,04 : 0,04 of 0,17.
        assert!((r.sd_expected - rss / 3.0).abs() < 1e-12);
        assert!((r.contribution_pct[0] - 9.0 / 17.0 * 100.0).abs() < 1e-9);
        let total: f64 = r.contribution_pct.iter().sum();
        assert!((total - 100.0).abs() < 1e-9);
    }

    #[test]
    fn monte_carlo_agrees_with_the_normal() {
        let r = compute(&gap(Spread::Normal { sigmas: 3.0 }));
        let s = 0.17f64.sqrt() / 3.0;
        assert!((r.mean - 0.5).abs() < 0.002, "{}", r.mean);
        assert!((r.sd - s).abs() / s < 0.01, "{} against {s}", r.sd);
        // Limits at ±0,4 = ±2,91σ: about 0,36 % outside in total.
        let out = r.below_pct.unwrap() + r.above_pct.unwrap();
        assert!((out - 0.36).abs() < 0.06, "{out}");
        let cp = 0.8 / (6.0 * r.sd);
        assert!((r.cp.unwrap() - cp).abs() < 1e-12);
        assert!(r.cpk.unwrap() <= r.cp.unwrap());
        assert_eq!(
            r.histogram.iter().map(|b| b.count).sum::<usize>(),
            r.samples
        );
        assert!(r.p00135 < r.p50 && r.p50 < r.p99865);
    }

    #[test]
    fn uniform_and_triangular_variances() {
        for (spread, k) in [(Spread::Uniform, 3.0), (Spread::Triangular, 6.0)] {
            let r = compute(&Stack {
                dimensions: vec![dim("X", 10.0, 0.6, 1.0, spread)],
                samples: 400_000,
                ..Stack::default()
            });
            let s = (0.36f64 / k).sqrt();
            assert!((r.sd_expected - s).abs() < 1e-12);
            assert!((r.sd - s).abs() / s < 0.01, "{spread:?}: {}", r.sd);
            assert!(r.min >= 9.4 - 1e-12 && r.max <= 10.6 + 1e-12);
        }
    }

    #[test]
    fn same_seed_same_result_and_asymmetric_tolerance() {
        let s = gap(Spread::Uniform);
        assert_eq!(compute(&s), compute(&s));
        // +0,4 / −0,0: the band is [10; 10,4], centred at 10,2.
        let r = compute(&Stack {
            dimensions: vec![Dimension {
                nominal: 10.0,
                tol_upper: 0.4,
                ..Dimension::default()
            }],
            ..Stack::default()
        });
        assert!((r.mean - 10.2).abs() < 0.005);
        assert!((r.worst_low - 10.0).abs() < 1e-12 && (r.worst_high - 10.4).abs() < 1e-12);
    }

    #[test]
    fn warnings_and_the_exchange_format() {
        let r = compute(&Stack::default());
        assert!(r.warnings.contains(&Warning::NoDimensions));
        let mut s = gap(Spread::Uniform);
        s.dimensions
            .push(dim("Arruela", 1.0, 0.0, -1.0, Spread::Uniform));
        s.lsl = Some(1.0);
        s.usl = Some(0.0);
        let r = compute(&s);
        assert!(r.warnings.contains(&Warning::NoTolerance(3)));
        assert!(r.warnings.contains(&Warning::LimitsSwapped));
        assert!(
            Warning::NoTolerance(3)
                .text(&s.dimensions)
                .contains("Arruela")
        );
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"cotas\"") && json.contains("\"uniforme\""));
        assert_eq!(serde_json::from_str::<Stack>(&json).unwrap(), s);
        let empty: Stack = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.samples, 100_000);
    }
}
