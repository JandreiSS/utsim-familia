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
    /// `(t, x)` of every observation of the statistics window, only when an analysis
    /// asked for this measure (`Simulation::keep_timed`); `None` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observations: Option<Vec<[f64; 2]>>,
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
            observations: t.timed().map(<[[f64; 2]]>::to_vec),
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
    /// Metrics computed by the block itself (`Behavior::custom_metrics`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, f64>,
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

/// States in which a mobile resource counts as busy.
pub const BUSY_TRAVEL_STATES: &[&str] = &[
    "travel_empty",
    "travel_loaded",
    "loading",
    "unloading",
    "utilize",
];

/// Utilisation of one resource in one replication, every figure in `[0, 1]` and never
/// above 1 per server. See [`BlockReport::utilization`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Utilization {
    /// Mean busy servers over capacity: the fraction of the window spent working
    /// (processing or setting up), ignoring items that finished but cannot leave.
    pub work: f64,
    /// `work` plus the servers held by finished items that cannot leave yet.
    pub incl_blocked: f64,
    /// `work` over the on-shift part of the window (off-shift time left out).
    pub in_shift: f64,
    /// `work` over the part of the window not spent blocked.
    pub excl_blocked: f64,
}

fn ratio(num: f64, den: f64) -> f64 {
    if den > 1e-12 {
        (num / den).min(1.0)
    } else {
        0.0
    }
}

impl BlockReport {
    fn state(&self, name: &str) -> f64 {
        self.states.get(name).copied().unwrap_or(0.0)
    }

    /// Utilisation by block kind, `None` for blocks that are not resources.
    ///
    /// Processors, combiners and separators count the time in the working states
    /// (processing, setup); a processor with capacity above 1 uses the mean number of
    /// busy servers (`servers.busy`) divided by its capacity, so it never exceeds 100%.
    /// The content of the block is not used: a combiner holds the components waiting for
    /// the rest of the set, which are not work. Mobile resources count the states in
    /// [`BUSY_TRAVEL_STATES`].
    pub fn utilization(&self) -> Option<Utilization> {
        let (work, blocked) = match self.kind.as_str() {
            "processor" => {
                let cap = self.custom.get("servers.capacity").copied().unwrap_or(1.0);
                let busy = self.custom.get("servers.busy").map_or_else(
                    || self.state("processing") + self.state("setup"),
                    |b| b / cap,
                );
                let held = self.custom.get("servers.blocked").map_or_else(
                    || self.state("blocked") + self.state("waiting_transport"),
                    |b| b / cap,
                );
                (busy, held)
            }
            "combiner" | "separator" => (
                self.state("processing") + self.state("setup"),
                self.state("blocked"),
            ),
            "transporter" => (BUSY_TRAVEL_STATES.iter().map(|s| self.state(s)).sum(), 0.0),
            _ => return None,
        };
        let work = work.clamp(0.0, 1.0);
        Some(Utilization {
            work,
            incl_blocked: (work + blocked).min(1.0),
            in_shift: ratio(work, 1.0 - self.state("off_shift")),
            excl_blocked: ratio(work, 1.0 - blocked),
        })
    }
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
                m.insert(format!("{n}.{k}.count"), t.count as f64);
            }
            for (k, v) in &b.custom {
                m.insert(format!("{n}.{k}"), *v);
            }
            if let Some(u) = b.utilization() {
                m.insert(format!("{n}.utilization"), u.work);
                m.insert(format!("{n}.utilization_incl_blocked"), u.incl_blocked);
                m.insert(format!("{n}.utilization_in_shift"), u.in_shift);
                m.insert(format!("{n}.utilization_excl_blocked"), u.excl_blocked);
            }
        }
        // Totals over the sources that can lose items (`on_blocked: Drop`).
        let (lost, entered) = self
            .blocks
            .iter()
            .filter(|b| b.custom.contains_key("loss_fraction"))
            .fold((0.0, 0.0), |(l, e), b| {
                (
                    l + b.custom.get("lost.count").copied().unwrap_or(0.0),
                    e + b.custom.get("entered.count").copied().unwrap_or(0.0),
                )
            });
        if self
            .blocks
            .iter()
            .any(|b| b.custom.contains_key("loss_fraction"))
        {
            m.insert(MODEL_LOST.to_string(), lost);
            m.insert(MODEL_ENTERED.to_string(), entered);
            m.insert(MODEL_LOSS_FRACTION.to_string(), ratio(lost, lost + entered));
        }
        m
    }
}

/// Model-wide totals over every source with `on_blocked: Drop`.
pub const MODEL_LOST: &str = "model.lost.count";
pub const MODEL_ENTERED: &str = "model.entered.count";
pub const MODEL_LOSS_FRACTION: &str = "model.loss_fraction";

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
