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
}

impl Tally {
    pub fn record(&mut self, x: f64) {
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
        if self.n < 2 { 0.0 } else { (self.m2 / (self.n - 1) as f64).sqrt() }
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
    totals: BTreeMap<State, Time>,
}

impl StateTracker {
    pub fn set(&mut self, t: Time, state: State) {
        if let Some(c) = self.current {
            *self.totals.entry(c).or_default() += t - self.since;
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
        let mut totals = self.totals.clone();
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
        let half_width = if n < 2 { f64::NAN } else { t_975(n - 1) * std_dev / (n as f64).sqrt() };
        Summary { n, mean: tally.mean(), std_dev, half_width }
    }
}

/// Two-sided 95% Student-t quantile.
fn t_975(df: usize) -> f64 {
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179,
        2.160, 2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064,
        2.060, 2.056, 2.052, 2.048, 2.045, 2.042,
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
        [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0].iter().for_each(|&x| t.record(x));
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
    fn state_fractions() {
        let mut s = StateTracker::default();
        s.set(0.0, State::Idle);
        s.set(1.0, State::Processing);
        let f = s.fractions(4.0);
        assert_eq!(f[&State::Idle], 0.25);
        assert_eq!(f[&State::Processing], 0.75);
    }
}
