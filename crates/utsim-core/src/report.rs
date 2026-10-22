//! Run results, flattened metrics and across-replication summaries.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Time;
use crate::stats::{Histogram, Summary, Tally};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TallySummary {
    pub count: u64,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub histogram: Option<Histogram>,
}

impl From<&Tally> for TallySummary {
    fn from(t: &Tally) -> Self {
        TallySummary {
            count: t.count(),
            mean: t.mean(),
            std_dev: t.std_dev(),
            min: t.min(),
            max: t.max(),
            histogram: None,
        }
    }
}

impl TallySummary {
    /// Summary plus a histogram with `bins` bins when the tally kept raw observations.
    pub fn with_histogram(t: &Tally, bins: Option<usize>) -> Self {
        let mut s = TallySummary::from(t);
        s.histogram = bins.and_then(|b| Histogram::build(t.raw()?, b));
        s
    }
}

/// Block content and state at one sampling instant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeriesPoint {
    pub t: Time,
    pub content: usize,
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockReport {
    pub name: String,
    pub kind: String,
    pub input: u64,
    pub output: u64,
    pub content_now: usize,
    pub content_mean: f64,
    pub content_max: f64,
    pub staytime: TallySummary,
    /// Fraction of the statistics window spent in each state.
    pub states: BTreeMap<String, f64>,
    /// Tallies recorded by the block itself (e.g. `time_in_system` at a sink).
    pub extra: BTreeMap<String, TallySummary>,
    /// Time series sampled every `sample_interval` (empty when sampling is off).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<SeriesPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub replication: u32,
    pub seed: u64,
    /// Start of the statistics window (the warm-up time).
    pub stats_from: Time,
    pub end_time: Time,
    pub events: u64,
    pub blocks: Vec<BlockReport>,
}

impl RunReport {
    pub fn block(&self, name: &str) -> Option<&BlockReport> {
        self.blocks.iter().find(|b| b.name == name)
    }

    /// Flat `block.metric` view used for replication summaries and tests.
    pub fn metrics(&self) -> BTreeMap<String, f64> {
        let mut m = BTreeMap::new();
        for b in &self.blocks {
            let n = &b.name;
            m.insert(format!("{n}.input"), b.input as f64);
            m.insert(format!("{n}.output"), b.output as f64);
            m.insert(format!("{n}.content.mean"), b.content_mean);
            m.insert(format!("{n}.content.max"), b.content_max);
            m.insert(format!("{n}.staytime.mean"), b.staytime.mean);
            for (s, f) in &b.states {
                m.insert(format!("{n}.state.{s}"), *f);
            }
            for (k, t) in &b.extra {
                m.insert(format!("{n}.{k}.mean"), t.mean);
            }
        }
        m
    }
}

/// Mean and 95% confidence half-width of every metric across replications.
/// A metric missing from a replication (e.g. a state never entered) counts as 0.
pub fn summarize(reports: &[RunReport]) -> BTreeMap<String, Summary> {
    let per_rep: Vec<_> = reports.iter().map(RunReport::metrics).collect();
    let mut keys: Vec<&String> = per_rep.iter().flat_map(|m| m.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .map(|k| {
            let xs: Vec<f64> = per_rep
                .iter()
                .map(|m| m.get(k).copied().unwrap_or(0.0))
                .collect();
            (k.clone(), Summary::from_samples(&xs))
        })
        .collect()
}
