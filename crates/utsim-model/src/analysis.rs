//! Analysis of a set of replications, in the shape a results screen needs: one row per
//! object (input, output, content, stay time, utilisation, states and the object's own
//! metrics, each with its 95% interval), the bottleneck, time series averaged across
//! replications, pooled histograms and paired comparisons between scenarios.
//!
//! Every interval comes from [`Summary::from_samples`] over the per-replication values,
//! the same statistics as [`utsim_core::summarize`] (a metric missing from a replication
//! counts as 0), so the numbers match `utsim run`.

use std::collections::BTreeMap;

use utsim_core::RunReport;
use utsim_core::stats::{Histogram, Summary};

use crate::{Model, ObjectKind};

/// States in which a mobile resource counts as busy.
pub const BUSY_TRAVEL_STATES: &[&str] = &[
    "travel_empty",
    "travel_loaded",
    "loading",
    "unloading",
    "utilize",
];

/// Across-replication statistics of one object (block) of the report.
#[derive(Clone, Debug)]
pub struct ObjectStats {
    pub name: String,
    /// Block kind as the report names it (`"processor"`, `"queue"`...).
    pub kind: String,
    pub input: Summary,
    pub output: Summary,
    pub content: Summary,
    pub staytime: Summary,
    /// Fraction of capacity in use (see [`utilization_of`]); `None` for objects that
    /// are not resources (queues, sources, sinks...).
    pub utilization: Option<Summary>,
    /// Fraction of time in each state, by state name, only the states ever entered.
    pub states: Vec<(String, Summary)>,
    /// Means of the object's own tallies (e.g. `time_in_system`), by tally name.
    pub extra: Vec<(String, Summary)>,
    /// Metrics computed by the object itself (e.g. tank levels), by metric name.
    pub custom: Vec<(String, Summary)>,
}

impl ObjectStats {
    /// Whether the object is a station or a mobile resource (a bottleneck candidate).
    pub fn is_resource(&self) -> bool {
        self.utilization.is_some()
    }
}

/// Whether a block kind (report naming) is a resource whose utilisation is measured.
pub fn is_resource_kind(kind: &str) -> bool {
    matches!(kind, "processor" | "combiner" | "separator" | "transporter")
}

/// Capacity of a station in the model (`1` when it has no capacity field).
fn capacity_of(model: &Model, name: &str) -> f64 {
    model
        .objects
        .iter()
        .find(|o| o.name == name)
        .map_or(1.0, |o| match &o.kind {
            ObjectKind::Processor { capacity, .. } => (*capacity).max(1) as f64,
            _ => 1.0,
        })
}

/// Utilisation of one block in one replication: for stations, mean content divided by
/// the capacity (items finished but blocked still occupy the station); for mobile
/// resources, the fraction of time travelling, loading, unloading or working at a
/// station ([`BUSY_TRAVEL_STATES`]). `None` for blocks that are not resources.
pub fn utilization_of(model: &Model, block: &utsim_core::BlockReport) -> Option<f64> {
    match block.kind.as_str() {
        "processor" | "combiner" | "separator" => {
            Some(block.content_mean / capacity_of(model, &block.name))
        }
        "transporter" => Some(
            BUSY_TRAVEL_STATES
                .iter()
                .filter_map(|s| block.states.get(*s))
                .sum(),
        ),
        _ => None,
    }
}

fn summary_of(values: impl Iterator<Item = f64>) -> Summary {
    let xs: Vec<f64> = values.collect();
    Summary::from_samples(&xs)
}

/// One row per block of the first report, in report order.
pub fn object_stats(model: &Model, reports: &[RunReport]) -> Vec<ObjectStats> {
    let Some(first) = reports.first() else {
        return Vec::new();
    };
    first
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            // Blocks keep the same order in every replication of one model.
            let of = |r: &RunReport| r.blocks.get(i).filter(|x| x.name == b.name).cloned();
            let blocks: Vec<utsim_core::BlockReport> = reports.iter().filter_map(of).collect();
            let field =
                |f: &dyn Fn(&utsim_core::BlockReport) -> f64| summary_of(blocks.iter().map(f));
            let mut state_names: Vec<&String> =
                blocks.iter().flat_map(|x| x.states.keys()).collect();
            state_names.sort();
            state_names.dedup();
            let states = state_names
                .into_iter()
                .map(|s| {
                    let sum = field(&|x| x.states.get(s).copied().unwrap_or(0.0));
                    (s.clone(), sum)
                })
                .filter(|(_, sum)| sum.mean > 0.0)
                .collect();
            let mut extra_names: Vec<&String> =
                blocks.iter().flat_map(|x| x.extra.keys()).collect();
            extra_names.sort();
            extra_names.dedup();
            let extra = extra_names
                .into_iter()
                .map(|k| {
                    let sum = field(&|x| x.extra.get(k).map_or(0.0, |t| t.mean));
                    (k.clone(), sum)
                })
                .collect();
            let mut custom_names: Vec<&String> =
                blocks.iter().flat_map(|x| x.custom.keys()).collect();
            custom_names.sort();
            custom_names.dedup();
            let custom = custom_names
                .into_iter()
                .map(|k| {
                    let sum = field(&|x| x.custom.get(k).copied().unwrap_or(0.0));
                    (k.clone(), sum)
                })
                .collect();
            let utilization = is_resource_kind(&b.kind).then(|| {
                summary_of(
                    blocks
                        .iter()
                        .map(|x| utilization_of(model, x).unwrap_or(0.0)),
                )
            });
            ObjectStats {
                name: b.name.clone(),
                kind: b.kind.clone(),
                input: field(&|x| x.input as f64),
                output: field(&|x| x.output as f64),
                content: field(&|x| x.content_mean),
                staytime: field(&|x| x.staytime.mean),
                utilization,
                states,
                extra,
                custom,
            }
        })
        .collect()
}

/// Index of the bottleneck: the resource with the highest mean utilisation (the first
/// one on ties). `None` when no resource was ever busy.
pub fn bottleneck(stats: &[ObjectStats]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, s) in stats.iter().enumerate() {
        let Some(u) = s.utilization.map(|u| u.mean).filter(|u| *u > 0.0) else {
            continue;
        };
        if best.is_none_or(|(_, b)| u > b) {
            best = Some((i, u));
        }
    }
    best.map(|(i, _)| i)
}

/// Content of one block over time: the mean across replications at each sampling
/// instant and every replication's own curve.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentSeries {
    /// `(t, mean content)` at the instants every replication sampled.
    pub mean: Vec<[f64; 2]>,
    /// `(t, content)` of each replication, in report order.
    pub replications: Vec<Vec<[f64; 2]>>,
}

/// Time series of `block`'s content (empty when sampling was off).
pub fn content_series(reports: &[RunReport], block: &str) -> ContentSeries {
    let replications: Vec<Vec<[f64; 2]>> = reports
        .iter()
        .filter_map(|r| r.block(block))
        .map(|b| b.series.iter().map(|p| [p.t, p.content as f64]).collect())
        .filter(|s: &Vec<[f64; 2]>| !s.is_empty())
        .collect();
    let n = replications.iter().map(Vec::len).min().unwrap_or(0);
    let mean = (0..n)
        .map(|k| {
            let t = replications[0][k][0];
            let m = replications.iter().map(|s| s[k][1]).sum::<f64>() / replications.len() as f64;
            [t, m]
        })
        .collect();
    ContentSeries { mean, replications }
}

/// Histograms of one tally of `block` from every replication, merged into one: the
/// common range spans every replication's range, with as many bins as the widest one,
/// and each original bin's count goes to the common bin holding its midpoint.
/// `tally` is `"staytime"` or the name of one of the block's own tallies.
pub fn pooled_histogram(reports: &[RunReport], block: &str, tally: &str) -> Option<Histogram> {
    let hists: Vec<&Histogram> = reports
        .iter()
        .filter_map(|r| r.block(block))
        .filter_map(|b| {
            if tally == "staytime" {
                b.staytime.histogram.as_ref()
            } else {
                b.extra.get(tally).and_then(|t| t.histogram.as_ref())
            }
        })
        .collect();
    let bins = hists.iter().map(|h| h.counts.len()).max()?;
    let min = hists.iter().map(|h| h.min).fold(f64::INFINITY, f64::min);
    let max = hists
        .iter()
        .map(|h| h.max)
        .fold(f64::NEG_INFINITY, f64::max);
    let bin_width = (max - min) / bins as f64;
    let mut counts = vec![0u64; bins];
    for h in hists {
        for (k, c) in h.counts.iter().enumerate() {
            let (a, b) = h.edges(k);
            let mid = (a + b) / 2.0;
            let j = if bin_width > 0.0 {
                (((mid - min) / bin_width) as usize).min(bins - 1)
            } else {
                0
            };
            counts[j] += c;
        }
    }
    Some(Histogram {
        min,
        max,
        bin_width,
        counts,
    })
}

/// Names of the tallies of `block` that have histograms (`"staytime"` first).
pub fn histogram_tallies(reports: &[RunReport], block: &str) -> Vec<String> {
    let Some(b) = reports.iter().find_map(|r| r.block(block)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if b.staytime.histogram.is_some() {
        out.push("staytime".to_string());
    }
    out.extend(
        b.extra
            .iter()
            .filter(|(_, t)| t.histogram.is_some())
            .map(|(k, _)| k.clone()),
    );
    out
}

/// Value of `metric` in each replication (0 when missing, as in `summarize`).
pub fn metric_values(reports: &[RunReport], metric: &str) -> Vec<f64> {
    reports
        .iter()
        .map(|r| r.metrics().get(metric).copied().unwrap_or(0.0))
        .collect()
}

/// Paired difference `a - b` (same seeds, replication by replication): mean and 95%
/// half-width. Only the replications both sides have are paired.
pub fn paired_difference(a: &[f64], b: &[f64]) -> Summary {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    Summary::from_samples(&d)
}

/// Whether a paired difference is significant at 95%: its interval excludes zero.
/// Without an interval (fewer than two replications) nothing is significant.
pub fn is_significant(d: &Summary) -> bool {
    !d.half_width.is_nan() && d.mean.abs() > d.half_width
}

/// `mean ± half-width` with four decimals (just the mean without an interval).
pub fn mean_hw(s: &Summary) -> String {
    if s.half_width.is_nan() {
        format!("{:.4}", s.mean)
    } else {
        format!("{:.4} ± {:.4}", s.mean, s.half_width)
    }
}

/// Difference cell as the optimiser prints it: `Δ ± hw`, ending in `≈` when the
/// interval contains zero (no significant difference).
pub fn delta_cell(d: &Summary) -> String {
    let text = if d.half_width.is_nan() {
        format!("{:+.4}", d.mean)
    } else {
        format!("{:+.4} ± {:.4}", d.mean, d.half_width)
    };
    if is_significant(d) {
        text
    } else {
        format!("{text} ≈")
    }
}

/// Every metric of a set of replications, as `metric -> values per replication`.
pub fn per_replication(reports: &[RunReport]) -> BTreeMap<String, Vec<f64>> {
    let per_rep: Vec<BTreeMap<String, f64>> = reports.iter().map(RunReport::metrics).collect();
    let mut keys: Vec<&String> = per_rep.iter().flat_map(|m| m.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .map(|k| {
            let xs = per_rep
                .iter()
                .map(|m| m.get(k).copied().unwrap_or(0.0))
                .collect();
            (k.clone(), xs)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"Model(
        name: "linha",
        run: (stop_time: 2000.0, warmup: 100.0, seed: 3, replications: 4,
              sample_interval: 50.0, histogram_bins: 8),
        objects: [
            (name: "chegada", kind: Source(interarrival: Exp(mean: 4.0))),
            (name: "fila_a", kind: Queue()),
            (name: "a", kind: Processor(process: Exp(mean: 2.0))),
            (name: "fila_b", kind: Queue()),
            (name: "b", kind: Processor(capacity: 2, process: Exp(mean: 7.0))),
            (name: "saida", kind: Sink),
        ],
        connections: [("chegada", "fila_a"), ("fila_a", "a"), ("a", "fila_b"),
                      ("fila_b", "b"), ("b", "saida")],
    )"#;

    fn run() -> (Model, Vec<RunReport>) {
        let m = Model::parse(LINE).unwrap();
        let r = m.run_replications().unwrap();
        (m, r)
    }

    #[test]
    fn rows_match_summarize() {
        let (m, reports) = run();
        let stats = object_stats(&m, &reports);
        let flat = utsim_core::summarize(&reports);
        assert_eq!(stats.len(), 6);
        for s in &stats {
            let n = &s.name;
            assert_eq!(s.input.mean, flat[&format!("{n}.input")].mean);
            assert_eq!(s.content.mean, flat[&format!("{n}.content.mean")].mean);
            assert_eq!(
                s.staytime.half_width.to_bits(),
                flat[&format!("{n}.staytime.mean")].half_width.to_bits()
            );
            for (state, sum) in &s.states {
                assert_eq!(sum.mean, flat[&format!("{n}.state.{state}")].mean);
            }
        }
        let saida = stats.iter().find(|s| s.name == "saida").unwrap();
        assert!(saida.extra.iter().any(|(k, _)| k == "time_in_system"));
        assert!(saida.utilization.is_none());
    }

    #[test]
    fn utilisation_divides_by_capacity_and_finds_the_bottleneck() {
        let (m, reports) = run();
        let stats = object_stats(&m, &reports);
        let a = stats.iter().find(|s| s.name == "a").unwrap();
        let b = stats.iter().find(|s| s.name == "b").unwrap();
        // a: 2/4 = 0.5; b: 7/(4*2) = 0.875.
        assert!((a.utilization.unwrap().mean - 0.5).abs() < 0.08, "{a:?}");
        assert!((b.utilization.unwrap().mean - 0.875).abs() < 0.08, "{b:?}");
        assert!((b.utilization.unwrap().mean - b.content.mean / 2.0).abs() < 1e-12);
        assert_eq!(stats[bottleneck(&stats).unwrap()].name, "b");
    }

    #[test]
    fn bottleneck_ignores_idle_and_non_resources() {
        let row = |name: &str, kind: &str, u: Option<f64>| ObjectStats {
            name: name.into(),
            kind: kind.into(),
            input: Summary::from_samples(&[]),
            output: Summary::from_samples(&[]),
            content: Summary::from_samples(&[]),
            staytime: Summary::from_samples(&[]),
            utilization: u.map(|v| Summary::from_samples(&[v, v])),
            states: vec![],
            extra: vec![],
            custom: vec![],
        };
        assert_eq!(bottleneck(&[row("q", "queue", None)]), None);
        assert_eq!(bottleneck(&[row("m", "processor", Some(0.0))]), None);
        let rows = [
            row("m1", "processor", Some(0.6)),
            row("agv", "transporter", Some(0.9)),
            row("m2", "processor", Some(0.9)),
        ];
        assert_eq!(bottleneck(&rows), Some(1));
    }

    #[test]
    fn transporter_utilisation_sums_busy_states() {
        let m = Model::parse(LINE).unwrap();
        let block = utsim_core::BlockReport {
            name: "agv".into(),
            kind: "transporter".into(),
            input: 0,
            output: 0,
            content_now: 0,
            content_mean: 0.0,
            content_max: 0.0,
            staytime: utsim_core::TallySummary {
                count: 0,
                mean: 0.0,
                std_dev: 0.0,
                min: 0.0,
                max: 0.0,
                histogram: None,
            },
            states: BTreeMap::from([
                ("idle".to_string(), 0.4),
                ("travel_loaded".to_string(), 0.35),
                ("travel_empty".to_string(), 0.15),
                ("loading".to_string(), 0.1),
            ]),
            extra: BTreeMap::new(),
            custom: BTreeMap::new(),
            series: vec![],
        };
        assert!((utilization_of(&m, &block).unwrap() - 0.6).abs() < 1e-12);
    }

    #[test]
    fn series_are_averaged_across_replications() {
        let (_, reports) = run();
        let s = content_series(&reports, "fila_b");
        assert_eq!(s.replications.len(), 4);
        assert!(!s.mean.is_empty());
        let k = 3;
        let expect = s.replications.iter().map(|r| r[k][1]).sum::<f64>() / 4.0;
        assert_eq!(s.mean[k], [s.replications[0][k][0], expect]);
        // The first sample is at the end of the warm-up.
        assert_eq!(s.mean[0][0], 100.0);
        assert!(content_series(&reports, "nada").mean.is_empty());
    }

    #[test]
    fn pooled_histogram_keeps_every_observation() {
        let (_, reports) = run();
        let h = pooled_histogram(&reports, "fila_b", "staytime").unwrap();
        let total: u64 = h.counts.iter().sum();
        let expected: u64 = reports
            .iter()
            .map(|r| r.block("fila_b").unwrap().staytime.count)
            .sum();
        assert_eq!(total, expected);
        assert_eq!(h.counts.len(), 8);
        assert!(histogram_tallies(&reports, "saida").contains(&"time_in_system".to_string()));
        assert!(pooled_histogram(&reports, "fila_b", "nada").is_none());
    }

    #[test]
    fn paired_difference_and_cells() {
        let d = paired_difference(&[5.0, 5.2, 4.8, 5.1], &[1.0, 1.1, 0.9, 1.0]);
        assert!(is_significant(&d));
        assert!(!delta_cell(&d).contains('≈'));
        let same = paired_difference(&[1.0, 1.2, 0.8], &[1.1, 1.0, 0.9]);
        assert!(!is_significant(&same));
        assert!(delta_cell(&same).ends_with('≈'));
        assert_eq!(mean_hw(&Summary::from_samples(&[2.0])), "2.0000");
        let (_, reports) = run();
        let per = per_replication(&reports);
        assert_eq!(per["a.input"].len(), 4);
        assert_eq!(metric_values(&reports, "a.input"), per["a.input"]);
    }
}
