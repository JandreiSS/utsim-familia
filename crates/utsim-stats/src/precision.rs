//! Precision of an estimate: how many replications reach a target half-width, and
//! intervals from one long run by batch means.

use serde::Serialize;
use utsim_core::stats::Summary;

use crate::{lag1_autocorrelation, summary, t_quantile};

/// Largest number of replications [`replications_needed`] will suggest.
pub const MAX_REPLICATIONS: usize = 100_000;

/// Smallest `n >= 2` with `t(level, n-1) · s / √n <= target`: the replications needed
/// for a half-width of at most `target`, assuming the standard deviation stays at `s`
/// (the usual iterative rule, Law §9.4.1). `None` when `s` or `target` is not a
/// positive finite number (with `s = 0` two replications already suffice), or when the
/// answer would exceed [`MAX_REPLICATIONS`].
pub fn replications_needed(s: f64, target: f64, level: f64) -> Option<usize> {
    if !(s.is_finite() && s >= 0.0 && target.is_finite() && target > 0.0) {
        return None;
    }
    let hw = |n: usize| t_quantile(level, n - 1) * s / (n as f64).sqrt();
    // Normal-theory first guess, then walk: t shrinks with n, so the half-width is
    // decreasing and the first n that fits is the answer.
    let z = t_quantile(level, 1_000_000);
    let guess = ((z * s / target).powi(2).floor() as usize).clamp(2, MAX_REPLICATIONS);
    let mut n = guess;
    while n > 2 && hw(n - 1) <= target {
        n -= 1;
    }
    while hw(n) > target {
        n += 1;
        if n > MAX_REPLICATIONS {
            return None;
        }
    }
    Some(n)
}

/// What the precision target is relative to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Target {
    /// Half-width in the measure's own units.
    Absolute(f64),
    /// Half-width as a fraction of the mean's magnitude (`0.02` = 2%).
    Relative(f64),
}

/// Precision of one measure over the replications run so far.
#[derive(Clone, Debug, Serialize)]
pub struct Precision {
    pub current: Summary,
    /// Target half-width in the measure's units.
    pub target: f64,
    /// Replications needed in total (`None`: see [`replications_needed`]).
    pub needed: Option<usize>,
    /// How many more replications to run (0 when the target is already met).
    pub more: Option<usize>,
}

/// Replications needed for `samples` (one value per replication) to reach `target`.
pub fn precision(samples: &[f64], target: Target, level: f64) -> Precision {
    let current = summary(samples, level);
    let target = match target {
        Target::Absolute(h) => h,
        Target::Relative(r) => r * current.mean.abs(),
    };
    let needed = if current.n < 2 {
        None
    } else {
        replications_needed(current.std_dev, target, level)
    };
    Precision {
        current,
        target,
        needed,
        more: needed.map(|n| n.saturating_sub(current.n)),
    }
}

/// Smallest number of batches accepted by [`batch_means`].
pub const MIN_BATCHES: usize = 10;
/// Lag-1 autocorrelation of the batch means below which they count as independent.
pub const MAX_LAG1: f64 = 0.2;

/// Interval from one long run by the method of batch means.
#[derive(Clone, Debug, Serialize)]
pub struct BatchMeans {
    /// Observations per batch.
    pub batch_size: usize,
    /// Batch means used (the trailing partial batch is left out).
    pub batches: usize,
    /// Lag-1 autocorrelation of the batch means.
    pub lag1: f64,
    /// Mean and interval over the batch means.
    pub summary: Summary,
    /// Whether the batch means passed the independence check (`lag1 < 0.2` with at
    /// least [`MIN_BATCHES`] batches). When `false` the interval is too narrow.
    pub independent: bool,
    /// Plain-language warning when the run is too short, in Portuguese.
    pub warning: Option<String>,
}

/// Batch means over `xs`: batch sizes double from 1 until the lag-1 autocorrelation of
/// the batch means drops below [`MAX_LAG1`], keeping at least [`MIN_BATCHES`] batches.
/// When that never happens, the result uses the largest batch size that still leaves
/// [`MIN_BATCHES`] batches and carries a warning. `None` with fewer than
/// [`MIN_BATCHES`] observations.
pub fn batch_means(xs: &[f64], level: f64) -> Option<BatchMeans> {
    if xs.len() < MIN_BATCHES {
        return None;
    }
    let mut size = 1;
    loop {
        let means: Vec<f64> = xs
            .chunks_exact(size)
            .map(|c| c.iter().sum::<f64>() / size as f64)
            .collect();
        let lag1 = lag1_autocorrelation(&means);
        let independent = lag1 < MAX_LAG1;
        let last = xs.len() / (size * 2) < MIN_BATCHES;
        if independent || last {
            let warning = (!independent).then(|| {
                format!(
                    "a autocorrelação dos {} lotes ainda é {:.2} (acima de {MAX_LAG1}): a rodada é curta \
                     demais para lotes independentes e o intervalo sai estreito demais; aumente o tempo de parada",
                    means.len(),
                    lag1
                )
            });
            return Some(BatchMeans {
                batch_size: size,
                batches: means.len(),
                lag1,
                summary: summary(&means, level),
                independent,
                warning,
            });
        }
        size *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};
    use rand_pcg::Pcg64;

    #[test]
    fn replications_needed_by_hand() {
        // s = 10, h* = 5 at 95%: n = 18 gives 2.110·10/√18 = 4.973 <= 5, while n = 17
        // gives 2.120·10/√17 = 5.142 > 5.
        assert_eq!(replications_needed(10.0, 5.0, 0.95), Some(18));
        // s = 2, h* = 1: n = 18 again (same ratio).
        assert_eq!(replications_needed(2.0, 1.0, 0.95), Some(18));
        // A wide target is met by two replications: 12.706·1/√2 = 8.98.
        assert_eq!(replications_needed(1.0, 9.0, 0.95), Some(2));
        assert_eq!(replications_needed(1.0, 8.9, 0.95), Some(3));
        // 99%: s = 10, h* = 5 -> n = 30 (2.756·10/√30 = 5.03 > 5), n = 31: 2.750·10/√31 = 4.94.
        assert_eq!(replications_needed(10.0, 5.0, 0.99), Some(31));
        assert_eq!(replications_needed(0.0, 1.0, 0.95), Some(2));
        assert_eq!(replications_needed(1.0, 0.0, 0.95), None);
        assert_eq!(replications_needed(1e9, 1e-9, 0.95), None);
    }

    #[test]
    fn relative_target_uses_the_mean() {
        let xs = [100.0, 104.0, 96.0, 102.0, 98.0];
        let p = precision(&xs, Target::Relative(0.01), 0.95);
        assert_eq!(p.target, 1.0);
        // s = √10 ≈ 3.162; n = 40 gives 2.021·3.162/√40 = 1.0105 > 1 and n = 41 gives
        // 2.021·3.162/√41 = 0.9981: 41 in total, 36 more than the 5 run.
        assert_eq!(p.needed, Some(41));
        assert_eq!(p.more, Some(36));
        let done = precision(&xs, Target::Absolute(100.0), 0.95);
        assert_eq!(done.more, Some(0));
    }

    /// AR(1) with mean `mu`: x_t = mu + phi·(x_{t-1} - mu) + e_t.
    fn ar1(rng: &mut Pcg64, n: usize, mu: f64, phi: f64) -> Vec<f64> {
        let mut x = mu;
        (0..n)
            .map(|_| {
                let e = rng.random::<f64>() - 0.5;
                x = mu + phi * (x - mu) + e;
                x
            })
            .collect()
    }

    #[test]
    fn batch_means_cover_the_true_mean() {
        let mut rng = Pcg64::seed_from_u64(3);
        let runs = 200;
        let mut covered = 0;
        for _ in 0..runs {
            let xs = ar1(&mut rng, 10_000, 5.0, 0.9);
            let b = batch_means(&xs, 0.95).unwrap();
            assert!(b.independent);
            assert!(b.batch_size >= 8, "{}", b.batch_size);
            if (b.summary.mean - 5.0).abs() <= b.summary.half_width {
                covered += 1;
            }
        }
        let coverage = covered as f64 / runs as f64;
        assert!((0.88..=0.99).contains(&coverage), "cobertura {coverage}");
    }

    #[test]
    fn batch_means_warn_on_a_short_correlated_run() {
        let mut rng = Pcg64::seed_from_u64(5);
        let xs = ar1(&mut rng, 60, 0.0, 0.99);
        let b = batch_means(&xs, 0.95).unwrap();
        assert!(!b.independent);
        assert!(b.warning.is_some());
        assert!(b.batches >= MIN_BATCHES);
        assert!(batch_means(&[1.0; 9], 0.95).is_none());
        // Independent data needs no batching.
        let iid: Vec<f64> = (0..1000).map(|_| rng.random::<f64>()).collect();
        assert_eq!(batch_means(&iid, 0.95).unwrap().batch_size, 1);
    }
}
