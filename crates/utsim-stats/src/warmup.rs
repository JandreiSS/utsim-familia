//! Warm-up detection: where does the initial transient end?
//!
//! - **Welch** (Law, *Simulation Modeling and Analysis*, §9.5.1): average the series
//!   across replications point by point, then smooth it with a centred moving average
//!   of window `w`; the warm-up is where the smoothed curve flattens (chosen by eye).
//! - **MSER-5** (White, 1997; White, Cobb & Spratt, 2000): group the series in batches
//!   of 5 and pick the cut `d` (in batches) that minimises
//!   `Σ_{j>d} (Z_j - Z̄_d)² / (k - d)²`, the squared standard error of the mean of what
//!   is left. Cuts beyond half the series are rejected: when the minimum lies there
//!   the run is too short to tell.

use serde::Serialize;

/// Across-replication mean of several series, point by point, up to the shortest one.
pub fn mean_series(series: &[Vec<f64>]) -> Vec<f64> {
    let n = series.iter().map(Vec::len).min().unwrap_or(0);
    (0..n)
        .map(|i| series.iter().map(|s| s[i]).sum::<f64>() / series.len() as f64)
        .collect()
}

/// Welch's moving average of window `w` over a (mean) series of length `m`:
/// point `i` (0-based) averages `2i + 1` points centred on it while `i < w`, then
/// `2w + 1` points. The result has `m - w` points (empty when `m <= w`).
pub fn welch(mean: &[f64], w: usize) -> Vec<f64> {
    let m = mean.len();
    if m <= w {
        return Vec::new();
    }
    (0..m - w)
        .map(|i| {
            let half = i.min(w);
            let window = &mean[i - half..=i + half];
            window.iter().sum::<f64>() / window.len() as f64
        })
        .collect()
}

/// Result of MSER on one series.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Mser {
    /// Batch size (5 for MSER-5).
    pub batch: usize,
    /// Number of whole batches (the trailing partial batch is left out).
    pub batches: usize,
    /// Chosen cut in batches: delete the first `cut_batches` batches.
    pub cut_batches: usize,
    /// Chosen cut in observations (`cut_batches · batch`): the index of the first
    /// observation kept.
    pub cut: usize,
    /// MSER statistic for every candidate cut `d = 0..=batches - 2`.
    pub statistic: Vec<f64>,
    /// The overall minimum lies beyond half the series: the cut kept is the best one in
    /// the first half, but the run is probably too short (or never settles).
    pub too_short: bool,
}

/// MSER with batches of `batch` observations (`batch = 1` is plain MSER). `None` when
/// there are fewer than 4 whole batches.
pub fn mser(series: &[f64], batch: usize) -> Option<Mser> {
    let batch = batch.max(1);
    let k = series.len() / batch;
    if k < 4 {
        return None;
    }
    let z: Vec<f64> = series
        .chunks_exact(batch)
        .map(|c| c.iter().sum::<f64>() / batch as f64)
        .collect();
    // Suffix sums give Σ and Σ² of the batches kept for every cut in one pass.
    let mut sum = vec![0.0; k + 1];
    let mut sq = vec![0.0; k + 1];
    for j in (0..k).rev() {
        sum[j] = sum[j + 1] + z[j];
        sq[j] = sq[j + 1] + z[j] * z[j];
    }
    let statistic: Vec<f64> = (0..=k - 2)
        .map(|d| {
            let left = (k - d) as f64;
            let mean = sum[d] / left;
            // Σ(Z - Z̄)² = ΣZ² - left·Z̄², clamped against rounding below zero.
            let ss = (sq[d] - left * mean * mean).max(0.0);
            ss / (left * left)
        })
        .collect();
    let argmin = |range: std::ops::RangeInclusive<usize>| {
        range
            .min_by(|&a, &b| statistic[a].total_cmp(&statistic[b]))
            .unwrap_or(0)
    };
    let global = argmin(0..=k - 2);
    let half = k / 2;
    let cut_batches = argmin(0..=half.min(k - 2));
    Some(Mser {
        batch,
        batches: k,
        cut_batches,
        cut: cut_batches * batch,
        statistic,
        too_short: global > half,
    })
}

/// MSER-5: [`mser`] with batches of 5.
pub fn mser5(series: &[f64]) -> Option<Mser> {
    mser(series, 5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};
    use rand_pcg::Pcg64;

    fn noise(rng: &mut Pcg64) -> f64 {
        // Sum of 12 uniforms minus 6: mean 0, variance 1 (good enough as Gaussian noise).
        (0..12).map(|_| rng.random::<f64>()).sum::<f64>() - 6.0
    }

    #[test]
    fn welch_moving_average_by_hand() {
        let mean = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        // w = 2: i=0 -> [1]; i=1 -> [1,2,3]; i=2 -> [1..5]; i=3 -> [2..6]
        assert_eq!(welch(&mean, 2), vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(
            welch(&[5.0, 1.0, 5.0, 1.0, 5.0], 1),
            vec![5.0, 11.0 / 3.0, 7.0 / 3.0, 11.0 / 3.0]
        );
        assert!(welch(&mean, 6).is_empty());
        assert_eq!(
            mean_series(&[vec![1.0, 2.0, 3.0], vec![3.0, 4.0]]),
            vec![2.0, 3.0]
        );
    }

    #[test]
    fn mser_statistic_by_hand() {
        // Batches of 1: [10, 0, 0, 0, 0]. d = 0: mean 2, SS = 80, /25 = 3.2;
        // d = 1: all zeros -> 0. The spike is deleted.
        let m = mser(&[10.0, 0.0, 0.0, 0.0, 0.0], 1).unwrap();
        assert_eq!(m.statistic[0], 3.2);
        assert_eq!(m.statistic[1], 0.0);
        assert_eq!((m.cut_batches, m.cut), (1, 1));
        assert!(mser(&[1.0; 19], 5).is_none());
    }

    #[test]
    fn mser5_finds_a_known_exponential_transient() {
        // x_i = 10·e^{-i/50} + noise: the transient is negligible (below 0.1, a tenth of
        // the noise) after i ≈ 230 and still large (above 2) before i ≈ 80.
        let mut rng = Pcg64::seed_from_u64(7);
        for _ in 0..5 {
            let xs: Vec<f64> = (0..5000)
                .map(|i| 10.0 * (-(i as f64) / 50.0).exp() + noise(&mut rng))
                .collect();
            let m = mser5(&xs).unwrap();
            assert!((80..=400).contains(&m.cut), "corte {}", m.cut);
            assert!(!m.too_short);
        }
    }

    #[test]
    fn mser5_cuts_near_zero_on_a_stationary_series() {
        let mut rng = Pcg64::seed_from_u64(11);
        let mut cuts = Vec::new();
        for _ in 0..20 {
            let xs: Vec<f64> = (0..2000).map(|_| 3.0 + noise(&mut rng)).collect();
            cuts.push(mser5(&xs).unwrap().cut);
        }
        cuts.sort_unstable();
        // The median cut stays within the first 5% of the series.
        assert!(cuts[10] <= 100, "{cuts:?}");
    }

    #[test]
    fn mser5_flags_a_series_that_never_settles() {
        // A steady climb: the smallest spread is always at the very end.
        let xs: Vec<f64> = (0..500).map(|i| i as f64).collect();
        let m = mser5(&xs).unwrap();
        assert!(m.too_short);
        assert!(m.cut_batches <= m.batches / 2);
    }
}
