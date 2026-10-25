//! Statistics accumulators: observation tallies, time-weighted values and state times.

use std::collections::BTreeMap;

use crate::Time;
use crate::block::State;

/// Running mean/variance of discrete observations (Welford).
#[derive(Clone, Debug, Default)]
pub struct Tally {
    n: u64,
    mean: f64,
    m2: f64,
    min: f64,
    max: f64,
    /// Raw observations, kept only when histograms are requested.
    raw: Option<Vec<f64>>,
}

impl Tally {
    /// A tally that also keeps every observation (for histograms).
    pub fn with_raw() -> Self {
        Tally {
            raw: Some(Vec::new()),
            ..Tally::default()
        }
    }

    /// An empty tally with the same raw-keeping setting as `self`.
    pub fn cleared(&self) -> Self {
        if self.raw.is_some() {
            Tally::with_raw()
        } else {
            Tally::default()
        }
    }

    pub fn raw(&self) -> Option<&[f64]> {
        self.raw.as_deref()
    }

    pub fn record(&mut self, x: f64) {
        if let Some(raw) = &mut self.raw {
            raw.push(x);
        }
        self.n += 1;
        if self.n == 1 {
            self.min = x;
            self.max = x;
        } else {
            self.min = self.min.min(x);
            self.max = self.max.max(x);
        }
        let delta = x - self.mean;
        self.mean += delta / self.n as f64;
        self.m2 += delta * (x - self.mean);
    }

    pub fn count(&self) -> u64 {
        self.n
    }

    pub fn mean(&self) -> f64 {
        if self.n == 0 { 0.0 } else { self.mean }
    }

    pub fn std_dev(&self) -> f64 {
        if self.n < 2 {
            0.0
        } else {
            (self.m2 / (self.n - 1) as f64).sqrt()
        }
    }

    pub fn min(&self) -> f64 {
        self.min
    }

    pub fn max(&self) -> f64 {
        self.max
    }
}

/// Time-weighted average of a piecewise-constant value (e.g. content).
#[derive(Clone, Debug, Default)]
pub struct TimeWeighted {
    value: f64,
    start: Time,
    last: Time,
    area: f64,
    max: f64,
}

impl TimeWeighted {
    pub fn set(&mut self, t: Time, value: f64) {
        self.area += self.value * (t - self.last);
        self.last = t;
        self.value = value;
        self.max = self.max.max(value);
    }

    pub fn reset(&mut self, t: Time) {
        self.start = t;
        self.last = t;
        self.area = 0.0;
        self.max = self.value;
    }

    pub fn mean(&self, t: Time) -> f64 {
        let span = t - self.start;
        if span <= 0.0 {
            self.value
        } else {
            (self.area + self.value * (t - self.last)) / span
        }
    }

    pub fn max(&self) -> f64 {
        self.max
    }

    pub fn current(&self) -> f64 {
        self.value
    }
}

/// Accumulated time spent in each [`State`].
#[derive(Clone, Debug, Default)]
pub struct StateTracker {
    current: Option<State>,
    since: Time,
    start: Time,
    /// Time per visited state, kept sorted by state: a block visits a handful of states,
    /// so a short vector beats a map on the hot path (one update per state change).
    totals: Vec<(State, Time)>,
}

impl StateTracker {
    fn add(&mut self, state: State, dt: Time) {
        match self.totals.binary_search_by_key(&state, |e| e.0) {
            Ok(k) => self.totals[k].1 += dt,
            Err(k) => self.totals.insert(k, (state, dt)),
        }
    }

    pub fn set(&mut self, t: Time, state: State) {
        if let Some(c) = self.current {
            self.add(c, t - self.since);
        }
        self.current = Some(state);
        self.since = t;
    }

    pub fn reset(&mut self, t: Time) {
        self.totals.clear();
        self.start = t;
        self.since = t;
    }

    pub fn current(&self) -> Option<State> {
        self.current
    }

    pub fn fractions(&self, t: Time) -> BTreeMap<State, f64> {
        let mut totals: BTreeMap<State, Time> = self.totals.iter().copied().collect();
        if let Some(c) = self.current {
            *totals.entry(c).or_default() += t - self.since;
        }
        let span = t - self.start;
        if span > 0.0 {
            totals.values_mut().for_each(|v| *v /= span);
        }
        totals
    }
}

/// Equal-width histogram over `[min, max]`; the last bin includes `max`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Histogram {
    pub min: f64,
    pub max: f64,
    /// Zero when all observations are equal (everything lands in the first bin).
    pub bin_width: f64,
    pub counts: Vec<u64>,
}

impl Histogram {
    /// Builds a histogram with `bins` bins; `None` if there is no data or `bins == 0`.
    pub fn build(xs: &[f64], bins: usize) -> Option<Histogram> {
        if xs.is_empty() || bins == 0 {
            return None;
        }
        let min = xs.iter().copied().fold(f64::INFINITY, f64::min);
        let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let bin_width = (max - min) / bins as f64;
        let mut counts = vec![0u64; bins];
        for &x in xs {
            let k = if bin_width > 0.0 {
                (((x - min) / bin_width) as usize).min(bins - 1)
            } else {
                0
            };
            counts[k] += 1;
        }
        Some(Histogram {
            min,
            max,
            bin_width,
            counts,
        })
    }

    /// `[start, end)` edges of bin `k`.
    pub fn edges(&self, k: usize) -> (f64, f64) {
        let start = self.min + self.bin_width * k as f64;
        (start, start + self.bin_width)
    }
}

/// Across-replication summary with a 95% confidence half-width.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Summary {
    pub n: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub half_width: f64,
}

impl Summary {
    pub fn from_samples(xs: &[f64]) -> Self {
        let n = xs.len();
        let mut tally = Tally::default();
        xs.iter().for_each(|&x| tally.record(x));
        let std_dev = tally.std_dev();
        let half_width = if n < 2 {
            f64::NAN
        } else {
            t_975(n - 1) * std_dev / (n as f64).sqrt()
        };
        Summary {
            n,
            mean: tally.mean(),
            std_dev,
            half_width,
        }
    }
}

/// Two-sided 95% Student-t quantile.
fn t_975(df: usize) -> f64 {
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    match df {
        0 => f64::NAN,
        1..=30 => TABLE[df - 1],
        31..=40 => 2.021,
        41..=60 => 2.000,
        61..=120 => 1.980,
        _ => 1.960,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tally_mean_and_std_dev() {
        let mut t = Tally::default();
        [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]
            .iter()
            .for_each(|&x| t.record(x));
        assert_eq!(t.mean(), 5.0);
        assert!((t.std_dev() - 2.138_089_935).abs() < 1e-9);
        assert_eq!((t.min(), t.max()), (2.0, 9.0));
    }

    #[test]
    fn time_weighted_mean_and_reset() {
        let mut tw = TimeWeighted::default();
        tw.set(0.0, 1.0);
        tw.set(2.0, 3.0);
        assert_eq!(tw.mean(4.0), 2.0);
        tw.reset(4.0);
        assert_eq!(tw.mean(6.0), 3.0);
    }

    #[test]
    fn histogram_bins_and_edges() {
        let h = Histogram::build(&[0.0, 1.0, 2.0, 3.0, 4.0, 10.0], 5).unwrap();
        assert_eq!((h.min, h.max, h.bin_width), (0.0, 10.0, 2.0));
        assert_eq!(h.counts, vec![2, 2, 1, 0, 1]);
        assert_eq!(h.edges(1), (2.0, 4.0));
        let flat = Histogram::build(&[3.0, 3.0], 4).unwrap();
        assert_eq!(flat.counts, vec![2, 0, 0, 0]);
        assert!(Histogram::build(&[], 4).is_none());
    }

    #[test]
    fn tally_keeps_raw_only_on_request() {
        let mut t = Tally::with_raw();
        t.record(1.0);
        t.record(2.0);
        assert_eq!(t.raw(), Some(&[1.0, 2.0][..]));
        assert_eq!(t.cleared().raw(), Some(&[][..]));
        let mut plain = Tally::default();
        plain.record(1.0);
        assert!(plain.raw().is_none());
    }

    #[test]
    fn state_fractions() {
        let mut s = StateTracker::default();
        s.set(0.0, State::Idle);
        s.set(1.0, State::Processing);
        let f = s.fractions(4.0);
        assert_eq!(f[&State::Idle], 0.25);
        assert_eq!(f[&State::Processing], 0.75);
    }
}
