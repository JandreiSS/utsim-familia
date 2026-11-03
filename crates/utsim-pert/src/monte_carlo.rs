//! Monte Carlo over the activity network: `N` draws of every duration, one CPM pass per
//! draw. Gives the distribution of the total time, the criticality index of every
//! activity and path, and the sensitivity (rank correlation and cruciality).
//!
//! Determinism: the draws are split in blocks of [`BLOCK`]; block `k` draws activity `i`
//! from `rng::stream(seed, k, i)`. Blocks run in parallel (rayon) and are merged in block
//! order, so the result depends only on the seed and the number of draws, never on the
//! number of threads.

use std::collections::BTreeMap;

#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::Serialize;
use utsim_core::rng;
use utsim_core::stats::{Histogram, Summary};

use crate::cpm::Cpm;
use crate::math::{correlation, quantile_sorted, ranks, wilson};
use crate::network::ActivityNetwork;

/// Draws per block (and per random stream of each activity).
pub const BLOCK: usize = 500;

/// Critical paths counted per draw (ties between constant durations can multiply them).
const PATHS_PER_DRAW: usize = 16;

/// Percentiles reported for the total time.
pub const PERCENTILES: [f64; 4] = [0.5, 0.8, 0.9, 0.95];

#[derive(Clone, Debug)]
pub struct McOptions {
    pub samples: usize,
    pub seed: u64,
    pub deadline: Option<f64>,
    /// Histogram bins of the total time.
    pub bins: usize,
}

impl Default for McOptions {
    fn default() -> Self {
        McOptions {
            samples: 10_000,
            seed: 1,
            deadline: None,
            bins: 20,
        }
    }
}

/// `P(T ≤ deadline)` with its Wilson 95% interval.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct DeadlineProbability {
    pub deadline: f64,
    pub probability: f64,
    pub low: f64,
    pub high: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PathCriticality {
    /// Activities from the start to the end milestone.
    pub path: Vec<usize>,
    /// Fraction of the draws in which this path is critical.
    pub index: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Sensitivity {
    pub activity: usize,
    /// Spearman correlation between the activity's duration and the total time.
    pub rank_correlation: f64,
    /// Criticality × rank correlation.
    pub cruciality: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct McResult {
    pub samples: usize,
    pub seed: u64,
    /// Total time: mean with a 95% interval, deviation, extremes.
    pub total: Summary,
    pub min: f64,
    pub max: f64,
    /// `(p, quantile)` for [`PERCENTILES`].
    pub percentiles: Vec<(f64, f64)>,
    pub histogram: Option<Histogram>,
    pub deadline: Option<DeadlineProbability>,
    /// Fraction of the draws in which each activity is critical.
    pub criticality: Vec<f64>,
    /// 95% half-width of each criticality (normal approximation).
    pub criticality_half_width: Vec<f64>,
    /// Paths that were critical at least once, most frequent first.
    pub paths: Vec<PathCriticality>,
    /// Activities with a random duration, largest |cruciality| first.
    pub sensitivity: Vec<Sensitivity>,
}

struct BlockOut {
    totals: Vec<f64>,
    /// Row-major: draw × activity.
    durations: Vec<f64>,
    critical: Vec<u64>,
    paths: BTreeMap<Vec<usize>, u64>,
}

fn run_block(net: &ActivityNetwork, cpm: &Cpm, seed: u64, block: usize, len: usize) -> BlockOut {
    let n = net.len();
    let mut rngs: Vec<_> = (0..n)
        .map(|i| rng::stream(seed, block as u32, i as u64))
        .collect();
    let mut out = BlockOut {
        totals: Vec::with_capacity(len),
        durations: Vec::with_capacity(len * n),
        critical: vec![0; n],
        paths: BTreeMap::new(),
    };
    let (mut es, mut ef, mut ls, mut lf) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let mut d = vec![0.0; n];
    let mut crit = vec![false; n];
    for _ in 0..len {
        for (i, act) in net.activities.iter().enumerate() {
            d[i] = act.sample(&mut rngs[i]);
        }
        let t = cpm.forward(&d, &mut es, &mut ef);
        cpm.backward(&d, t, &mut ls, &mut lf);
        cpm.critical_flags(&es, &ls, t, &mut crit);
        for (c, &f) in out.critical.iter_mut().zip(&crit) {
            *c += u64::from(f);
        }
        let (paths, _) = cpm.critical_paths(&crit, &es, &ef, t, PATHS_PER_DRAW);
        for p in paths {
            *out.paths.entry(p).or_default() += 1;
        }
        out.totals.push(t);
        out.durations.extend_from_slice(&d);
    }
    out
}

/// Monte Carlo of the network.
pub fn run(net: &ActivityNetwork, options: &McOptions) -> McResult {
    let n = net.len();
    let samples = options.samples.max(1);
    let cpm = Cpm::new(net);
    let blocks = samples.div_ceil(BLOCK);
    let one = |k: usize| {
        let len = BLOCK.min(samples - k * BLOCK);
        run_block(net, &cpm, options.seed, k, len)
    };
    #[cfg(feature = "parallel")]
    let outs: Vec<BlockOut> = (0..blocks).into_par_iter().map(one).collect();
    #[cfg(not(feature = "parallel"))]
    let outs: Vec<BlockOut> = (0..blocks).map(one).collect();

    let mut totals = Vec::with_capacity(samples);
    let mut durations = Vec::with_capacity(samples * n);
    let mut critical = vec![0u64; n];
    let mut paths: BTreeMap<Vec<usize>, u64> = BTreeMap::new();
    for b in outs {
        totals.extend(b.totals);
        durations.extend(b.durations);
        for (c, x) in critical.iter_mut().zip(b.critical) {
            *c += x;
        }
        for (p, x) in b.paths {
            *paths.entry(p).or_default() += x;
        }
    }

    let nf = samples as f64;
    let criticality: Vec<f64> = critical.iter().map(|&c| c as f64 / nf).collect();
    let criticality_half_width = criticality
        .iter()
        .map(|&p| 1.959_964 * (p * (1.0 - p) / nf).sqrt())
        .collect();
    let mut paths: Vec<PathCriticality> = paths
        .into_iter()
        .map(|(path, c)| PathCriticality {
            path,
            index: c as f64 / nf,
        })
        .collect();
    paths.sort_by(|a, b| b.index.total_cmp(&a.index));

    let total_ranks = ranks(&totals);
    let mut sensitivity: Vec<Sensitivity> = (0..n)
        .filter(|&i| !net.activities[i].is_milestone())
        .filter_map(|i| {
            let col: Vec<f64> = (0..samples).map(|s| durations[s * n + i]).collect();
            let rho = correlation(&ranks(&col), &total_ranks);
            rho.is_finite().then_some(Sensitivity {
                activity: i,
                rank_correlation: rho,
                cruciality: criticality[i] * rho,
            })
        })
        .collect();
    sensitivity.sort_by(|a, b| b.cruciality.abs().total_cmp(&a.cruciality.abs()));

    let deadline = options.deadline.map(|t| {
        let k = totals.iter().filter(|&&x| x <= t).count();
        let (low, high) = wilson(k, samples);
        DeadlineProbability {
            deadline: t,
            probability: k as f64 / nf,
            low,
            high,
        }
    });
    let total = Summary::from_samples(&totals);
    let histogram = Histogram::build(&totals, options.bins);
    let mut sorted = totals;
    sorted.sort_by(f64::total_cmp);
    McResult {
        samples,
        seed: options.seed,
        total,
        min: sorted[0],
        max: sorted[samples - 1],
        percentiles: PERCENTILES
            .iter()
            .map(|&p| (p, quantile_sorted(&sorted, p)))
            .collect(),
        histogram,
        deadline,
        criticality,
        criticality_half_width,
        paths,
        sensitivity,
    }
}
