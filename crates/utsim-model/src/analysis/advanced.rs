//! Output analyses that need the reports or the trace of a model: warm-up detection,
//! precision, batch means, Little's law and the shifting bottleneck (see
//! `docs/plano_analises_e_dinamica.md`, section 2, P1). The statistics themselves live
//! in `utsim-stats`; this module turns runs into the series they need.

use serde::Serialize;
use utsim_core::stats::Summary;
use utsim_core::{RunReport, State, TraceEvent, TraceRecord};
use utsim_stats::little::LittleCheck;
use utsim_stats::precision::{BatchMeans, Precision, Target};
use utsim_stats::warmup::Mser;

use super::{is_resource_kind, metric_values};
use crate::{Model, ModelError};

/// Sampling points used for the content series when the model samples nothing.
pub const DEFAULT_SAMPLES: f64 = 500.0;

/// Window of Welch's moving average when the user picks none.
pub const DEFAULT_WELCH_WINDOW: usize = 5;

// ---------------------------------------------------------------------------------------
// Warm-up
// ---------------------------------------------------------------------------------------

/// Which series the warm-up analysis looks at.
#[derive(Clone, Debug, PartialEq)]
pub enum WarmupSeries {
    /// Work in process: the summed content of every object except sinks, sampled at
    /// `run.sample_interval`.
    Wip,
    /// The observations of one tally (`"staytime"` or the object's own, e.g.
    /// `"time_in_system"` of a sink), in the order they happen.
    Measure { object: String, measure: String },
}

impl WarmupSeries {
    /// Parses `objeto.medida` (e.g. `saida.time_in_system`, `fila.staytime`).
    pub fn parse(text: &str) -> Option<WarmupSeries> {
        let (object, measure) = text.rsplit_once('.')?;
        (!object.is_empty() && !measure.is_empty()).then(|| WarmupSeries::Measure {
            object: object.to_string(),
            measure: measure.to_string(),
        })
    }

    /// Short description for printing.
    pub fn label(&self) -> String {
        match self {
            WarmupSeries::Wip => "WIP (conteúdo somado, sem os sumidouros)".to_string(),
            WarmupSeries::Measure { object, measure } => format!("{object}.{measure}"),
        }
    }
}

/// Result of the warm-up analysis over a set of replications run from time 0.
#[derive(Clone, Debug, Serialize)]
pub struct WarmupAnalysis {
    /// Instant of each point of the mean series (for observations: the mean instant
    /// of the `i`-th observation across replications).
    pub times: Vec<f64>,
    /// Across-replication mean of the series, point by point.
    pub mean: Vec<f64>,
    /// Window of Welch's moving average.
    pub window: usize,
    /// Welch's moving average of `mean` (`mean.len() - window` points; the instants
    /// are the first ones of `times`).
    pub welch: Vec<f64>,
    /// MSER-5 on the mean series (`None` with fewer than 20 points).
    pub mser: Option<Mser>,
    /// Suggested `run.warmup`: the instant of the first point MSER-5 keeps.
    pub suggested: Option<f64>,
}

/// The model to run for a warm-up analysis: no warm-up (the transient must be in the
/// data) and, for the WIP series, sampling on (`stop_time / 500` when off).
pub fn warmup_model(model: &Model, series: &WarmupSeries) -> Model {
    let mut m = model.clone();
    m.run.warmup = 0.0;
    if *series == WarmupSeries::Wip && m.run.sample_interval.is_none() {
        m.run.sample_interval = Some(m.run.stop_time / DEFAULT_SAMPLES);
    }
    m
}

/// Runs the replications a warm-up analysis needs (see [`warmup_model`]).
pub fn run_for_warmup(model: &Model, series: &WarmupSeries) -> Result<Vec<RunReport>, ModelError> {
    let m = warmup_model(model, series);
    match series {
        WarmupSeries::Wip => m.run_replications(),
        WarmupSeries::Measure { object, measure } => {
            if m.objects.iter().all(|o| &o.name != object) {
                return Err(ModelError::Invalid(vec![format!(
                    "objeto \"{object}\" não existe no modelo"
                )]));
            }
            m.run_replications_timed(&[(object.clone(), measure.clone())])
        }
    }
}

/// `(t, x)` series of one replication for the warm-up analysis.
fn series_of(report: &RunReport, series: &WarmupSeries) -> Vec<[f64; 2]> {
    match series {
        WarmupSeries::Wip => {
            let blocks: Vec<_> = report.blocks.iter().filter(|b| b.kind != "sink").collect();
            let n = blocks.iter().map(|b| b.series.len()).min().unwrap_or(0);
            (0..n)
                .map(|k| {
                    let t = blocks[0].series[k].t;
                    let wip: usize = blocks.iter().map(|b| b.series[k].content).sum();
                    [t, wip as f64]
                })
                .collect()
        }
        WarmupSeries::Measure { object, measure } => report
            .block(object)
            .and_then(|b| {
                if measure == "staytime" {
                    b.staytime.observations.clone()
                } else {
                    b.extra.get(measure).and_then(|t| t.observations.clone())
                }
            })
            .unwrap_or_default(),
    }
}

/// Welch and MSER-5 over the replications (run them with [`run_for_warmup`]).
pub fn warmup(reports: &[RunReport], series: &WarmupSeries, window: usize) -> WarmupAnalysis {
    let per_rep: Vec<Vec<[f64; 2]>> = reports.iter().map(|r| series_of(r, series)).collect();
    let values: Vec<Vec<f64>> = per_rep
        .iter()
        .map(|s| s.iter().map(|p| p[1]).collect())
        .collect();
    let instants: Vec<Vec<f64>> = per_rep
        .iter()
        .map(|s| s.iter().map(|p| p[0]).collect())
        .collect();
    let mean = utsim_stats::warmup::mean_series(&values);
    let times = utsim_stats::warmup::mean_series(&instants);
    let welch = utsim_stats::warmup::welch(&mean, window);
    let mser = utsim_stats::warmup::mser5(&mean);
    let suggested = mser.as_ref().and_then(|m| times.get(m.cut).copied());
    WarmupAnalysis {
        times,
        mean,
        window,
        welch,
        mser,
        suggested,
    }
}

// ---------------------------------------------------------------------------------------
// Precision
// ---------------------------------------------------------------------------------------

/// Parses a precision target: `2%` (relative to the mean) or `0.5` (absolute, in the
/// metric's units; decimal comma accepted).
pub fn parse_target(text: &str) -> Option<Target> {
    let t = text.trim().replace(',', ".");
    if let Some(p) = t.strip_suffix('%') {
        let v: f64 = p.trim().parse().ok()?;
        (v > 0.0 && v.is_finite()).then_some(Target::Relative(v / 100.0))
    } else {
        let v: f64 = t.parse().ok()?;
        (v > 0.0 && v.is_finite()).then_some(Target::Absolute(v))
    }
}

/// Metrics checked by the precision analysis when the user names none: every
/// resource's utilisation, every object's mean content and stay time, and the time in
/// system of every sink, leaving out the ones that are always zero.
pub fn default_precision_metrics(reports: &[RunReport]) -> Vec<String> {
    let Some(first) = reports.first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for b in &first.blocks {
        let n = &b.name;
        if is_resource_kind(&b.kind) {
            out.push(format!("{n}.utilization"));
        }
        if b.kind != "sink" && b.kind != "source" {
            out.push(format!("{n}.content.mean"));
            out.push(format!("{n}.staytime.mean"));
        }
        if b.extra.contains_key("time_in_system") {
            out.push(format!("{n}.time_in_system.mean"));
        }
    }
    out.retain(|k| metric_values(reports, k).iter().any(|v| *v != 0.0));
    out
}

/// Replications needed for each metric to reach `target` at confidence `level`.
pub fn precision(
    reports: &[RunReport],
    metrics: &[String],
    target: Target,
    level: f64,
) -> Vec<(String, Precision)> {
    metrics
        .iter()
        .map(|k| {
            let xs = metric_values(reports, k);
            (
                k.clone(),
                utsim_stats::precision::precision(&xs, target, level),
            )
        })
        .collect()
}

/// Batch means over the observations of one tally in one replication (a long single
/// run; the observations are those of the statistics window, after the warm-up).
/// `None` when the tally has no timed observations or too few of them.
pub fn batch_means(
    report: &RunReport,
    object: &str,
    measure: &str,
    level: f64,
) -> Option<BatchMeans> {
    let series = WarmupSeries::Measure {
        object: object.to_string(),
        measure: measure.to_string(),
    };
    let xs: Vec<f64> = series_of(report, &series).iter().map(|p| p[1]).collect();
    utsim_stats::precision::batch_means(&xs, level)
}

// ---------------------------------------------------------------------------------------
// Little's law
// ---------------------------------------------------------------------------------------

/// Kinds left out of the per-object check: sinks (items leave at once), assembly and
/// disassembly (items in and out are not the same items), and objects that hold no
/// items (meters, fluid networks).
const LITTLE_SKIPPED: &[&str] = &["sink", "combiner", "separator", "span", "fluid"];

/// Little's law of one object.
#[derive(Clone, Debug, Serialize)]
pub struct LittleRow {
    pub name: String,
    pub kind: String,
    pub check: LittleCheck,
}

/// Little's law per object and for the whole system.
#[derive(Clone, Debug, Serialize)]
pub struct LittleReport {
    pub objects: Vec<LittleRow>,
    /// From the sinks' `time_in_system`: `L` = summed content (plus items in transit),
    /// `λ` = items reaching the sinks per time unit. `None` without sinks that record
    /// it, or when the model assembles or splits items (the count of items changes).
    pub system: Option<LittleCheck>,
    /// Mean number of items in transit on timed direct connections (`transfer`),
    /// included in the system's `L`; `None` when the model has no `transfer`.
    pub transit: Option<Summary>,
}

/// Whether some object of the model has a timed direct connection (`transfer`).
pub fn has_transfers(model: &Model) -> bool {
    model
        .objects
        .iter()
        .any(|o| model.effective_transfer(o).is_some())
}

/// Mean number of items in transit over `[from, to]`, from the `Transit` events of a
/// trace (each item travels for `delay + duration` from its event).
pub fn transit_mean(records: &[TraceRecord], from: f64, to: f64) -> f64 {
    let span = to - from;
    if span <= 0.0 {
        return 0.0;
    }
    let area: f64 = records
        .iter()
        .filter_map(|r| match &r.event {
            TraceEvent::Transit {
                delay, duration, ..
            } => {
                let (s, e) = (r.t.max(from), (r.t + delay + duration).min(to));
                Some((e - s).max(0.0))
            }
            _ => None,
        })
        .sum();
    area / span
}

/// Runs the replications of `model` with their traces (one [`crate::replay::Replay`]
/// per replication, in parallel with the `parallel` feature).
pub fn run_traced(model: &Model) -> Result<Vec<(RunReport, Vec<TraceRecord>)>, ModelError> {
    model.validate()?;
    let one = |r: u32| model.replay(r).map(|rp| (rp.report, rp.records));
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        (0..model.run.replications)
            .into_par_iter()
            .map(one)
            .collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        (0..model.run.replications).map(one).collect()
    }
}

/// Runs what [`little`] needs: the plain replications, or, when the model has
/// `transfer`, traced ones to count the items in transit (one mean per replication).
pub fn run_for_little(model: &Model) -> Result<(Vec<RunReport>, Option<Vec<f64>>), ModelError> {
    if !has_transfers(model) {
        return Ok((model.run_replications()?, None));
    }
    let traced = run_traced(model)?;
    let transit = traced
        .iter()
        .map(|(r, rec)| transit_mean(rec, r.stats_from, r.end_time))
        .collect();
    Ok((traced.into_iter().map(|(r, _)| r).collect(), Some(transit)))
}

/// Little's law per object and for the system. Per object, per replication:
/// `L` = mean content, `λ` = output / window, `W` = mean stay time. `transit` is the
/// mean number in transit per replication (from [`run_for_little`]).
pub fn little(
    reports: &[RunReport],
    transit: Option<&[f64]>,
    level: f64,
    tolerance: f64,
) -> LittleReport {
    let Some(first) = reports.first() else {
        return LittleReport {
            objects: Vec::new(),
            system: None,
            transit: None,
        };
    };
    let window = |r: &RunReport| (r.end_time - r.stats_from).max(f64::MIN_POSITIVE);
    let mut objects = Vec::new();
    for (i, b) in first.blocks.iter().enumerate() {
        if LITTLE_SKIPPED.contains(&b.kind.as_str()) {
            continue;
        }
        let blocks: Vec<_> = reports
            .iter()
            .filter_map(|r| r.blocks.get(i).filter(|x| x.name == b.name).map(|x| (r, x)))
            .collect();
        if blocks.iter().all(|(_, x)| x.input == 0 && x.output == 0) {
            continue;
        }
        let l: Vec<f64> = blocks.iter().map(|(_, x)| x.content_mean).collect();
        let lambda: Vec<f64> = blocks
            .iter()
            .map(|(r, x)| x.output as f64 / window(r))
            .collect();
        let w: Vec<f64> = blocks.iter().map(|(_, x)| x.staytime.mean).collect();
        objects.push(LittleRow {
            name: b.name.clone(),
            kind: b.kind.clone(),
            check: utsim_stats::little::check(&l, &lambda, &w, level, tolerance),
        });
    }
    let changes_count = first
        .blocks
        .iter()
        .any(|b| b.kind == "combiner" || b.kind == "separator");
    let has_sink_times = first
        .blocks
        .iter()
        .any(|b| b.kind == "sink" && b.extra.contains_key("time_in_system"));
    let system = (!changes_count && has_sink_times).then(|| {
        let mut l = Vec::new();
        let mut lambda = Vec::new();
        let mut w = Vec::new();
        for (k, r) in reports.iter().enumerate() {
            let in_system: f64 = r
                .blocks
                .iter()
                .filter(|b| b.kind != "sink")
                .map(|b| b.content_mean)
                .sum();
            let moving = transit.and_then(|t| t.get(k)).copied().unwrap_or(0.0);
            let (count, total) = r
                .blocks
                .iter()
                .filter(|b| b.kind == "sink")
                .filter_map(|b| b.extra.get("time_in_system"))
                .fold((0.0, 0.0), |(c, s), t| {
                    (c + t.count as f64, s + t.count as f64 * t.mean)
                });
            l.push(in_system + moving);
            lambda.push(count / window(r));
            w.push(if count > 0.0 { total / count } else { 0.0 });
        }
        utsim_stats::little::check(&l, &lambda, &w, level, tolerance)
    });
    LittleReport {
        objects,
        system,
        transit: transit.map(|t| utsim_stats::summary(t, level)),
    }
}

// ---------------------------------------------------------------------------------------
// Shifting bottleneck (active period method)
// ---------------------------------------------------------------------------------------

/// States in which a resource counts as active (working) for the shifting bottleneck.
pub const ACTIVE_STATES: &[State] = &[
    State::Setup,
    State::Processing,
    State::TravelEmpty,
    State::TravelLoaded,
    State::Loading,
    State::Unloading,
    State::Utilize,
];

/// Active periods `(start, end)` of every block, from the `State` events of a trace,
/// clipped to `[from, to]`. A period is a continuous stretch in [`ACTIVE_STATES`];
/// periods that touch (a station that finishes one item and starts the next at the
/// same instant) are one period. A period still open at `to` ends there.
pub fn active_periods(
    records: &[TraceRecord],
    blocks: usize,
    from: f64,
    to: f64,
) -> Vec<Vec<(f64, f64)>> {
    let mut periods: Vec<Vec<(f64, f64)>> = vec![Vec::new(); blocks];
    let mut open: Vec<Option<f64>> = vec![None; blocks];
    let close = |periods: &mut Vec<Vec<(f64, f64)>>, b: usize, s: f64, e: f64| {
        let (s, e) = (s.max(from), e.min(to));
        if e <= s {
            return;
        }
        let list = &mut periods[b];
        match list.last_mut() {
            Some(last) if last.1 >= s => last.1 = last.1.max(e),
            _ => list.push((s, e)),
        }
    };
    for r in records {
        if r.t > to {
            break;
        }
        let TraceEvent::State { block, state } = r.event else {
            continue;
        };
        if block >= blocks {
            continue;
        }
        let active = ACTIVE_STATES.contains(&state);
        match (open[block], active) {
            (None, true) => open[block] = Some(r.t),
            (Some(s), false) => {
                close(&mut periods, block, s, r.t);
                open[block] = None;
            }
            _ => {}
        }
    }
    for (b, s) in open.iter().enumerate() {
        if let Some(s) = s {
            close(&mut periods, b, *s, to);
        }
    }
    periods
}

/// Time each resource spends as the sole and as a shifting (shared) bottleneck, from
/// the active periods of every resource over `[from, to]`.
///
/// At each instant the momentary bottleneck is the resource whose current active
/// period is the longest (the first one on ties). Consecutive bottlenecks whose active
/// periods overlap share the overlap (shifting bottleneck); the rest of a bottleneck's
/// active period is sole bottleneck time. Returns `(sole, shifting)` time per resource;
/// instants when no resource is active belong to no one.
pub fn shifting_bottleneck_times(periods: &[Vec<(f64, f64)>]) -> Vec<(f64, f64)> {
    let mut points: Vec<f64> = periods
        .iter()
        .flatten()
        .flat_map(|&(s, e)| [s, e])
        .collect();
    points.sort_by(f64::total_cmp);
    points.dedup();
    // Momentary bottleneck episodes: (resource, period index), consecutive repeats merged.
    let mut episodes: Vec<(usize, usize)> = Vec::new();
    for w in points.windows(2) {
        let mid = 0.5 * (w[0] + w[1]);
        let mut best: Option<(usize, usize, f64)> = None;
        for (r, list) in periods.iter().enumerate() {
            // Periods of one resource are sorted and disjoint.
            let k = list.partition_point(|&(_, e)| e <= mid);
            let Some(&(s, e)) = list.get(k).filter(|&&(s, _)| s <= mid) else {
                continue;
            };
            if best.is_none_or(|(_, _, len)| e - s > len) {
                best = Some((r, k, e - s));
            }
        }
        if let Some((r, k, _)) = best {
            if episodes.last() != Some(&(r, k)) {
                episodes.push((r, k));
            }
        }
    }
    let period = |(r, k): (usize, usize)| periods[r][k];
    let overlap = |a: (usize, usize), b: (usize, usize)| -> Option<(f64, f64)> {
        if a.0 == b.0 {
            return None;
        }
        let ((s1, e1), (s2, e2)) = (period(a), period(b));
        let (s, e) = (s1.max(s2), e1.min(e2));
        (e > s).then_some((s, e))
    };
    let len = |o: Option<(f64, f64)>| o.map_or(0.0, |(s, e)| e - s);
    let mut out = vec![(0.0, 0.0); periods.len()];
    for (i, &ep) in episodes.iter().enumerate() {
        let (s, e) = period(ep);
        let prev = i.checked_sub(1).and_then(|j| overlap(episodes[j], ep));
        let next = episodes.get(i + 1).and_then(|&n| overlap(ep, n));
        // Both overlaps lie inside this period; their union is the shared time.
        let both = match (prev, next) {
            (Some((a, b)), Some((c, d))) => (b.min(d) - a.max(c)).max(0.0),
            _ => 0.0,
        };
        let shared = len(prev) + len(next) - both;
        out[ep.0].0 += (e - s) - shared;
        out[ep.0].1 += shared;
    }
    out
}

/// Shifting bottleneck share of one resource over a set of replications.
#[derive(Clone, Debug, Serialize)]
pub struct BottleneckShare {
    pub name: String,
    pub kind: String,
    /// Fraction of the statistics window as the sole bottleneck.
    pub sole: Summary,
    /// Fraction of the window as a shifting (shared) bottleneck.
    pub shifting: Summary,
}

impl BottleneckShare {
    /// Sole plus shifting fraction (mean), the usual ranking of the method.
    pub fn total(&self) -> f64 {
        self.sole.mean + self.shifting.mean
    }
}

/// Sole and shifting bottleneck fractions of every resource (processes, assemblies and
/// mobile resources), from one trace per replication ([`run_traced`]), most
/// bottlenecked first. Complements [`super::bottleneck`] (highest mean utilisation).
pub fn shifting_bottleneck(
    traced: &[(RunReport, Vec<TraceRecord>)],
    level: f64,
) -> Vec<BottleneckShare> {
    let Some((first, _)) = traced.first() else {
        return Vec::new();
    };
    let resources: Vec<usize> = first
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| is_resource_kind(&b.kind))
        .map(|(i, _)| i)
        .collect();
    let mut sole: Vec<Vec<f64>> = vec![Vec::new(); resources.len()];
    let mut shifting: Vec<Vec<f64>> = vec![Vec::new(); resources.len()];
    for (report, records) in traced {
        let (from, to) = (report.stats_from, report.end_time);
        let span = to - from;
        let all = active_periods(records, report.blocks.len(), from, to);
        let periods: Vec<Vec<(f64, f64)>> = resources.iter().map(|&i| all[i].clone()).collect();
        let times = shifting_bottleneck_times(&periods);
        for (k, (s, h)) in times.into_iter().enumerate() {
            let f = |x: f64| if span > 0.0 { x / span } else { 0.0 };
            sole[k].push(f(s));
            shifting[k].push(f(h));
        }
    }
    let mut out: Vec<BottleneckShare> = resources
        .iter()
        .enumerate()
        .map(|(k, &i)| BottleneckShare {
            name: first.blocks[i].name.clone(),
            kind: first.blocks[i].kind.clone(),
            sole: utsim_stats::summary(&sole[k], level),
            shifting: utsim_stats::summary(&shifting[k], level),
        })
        .collect();
    out.sort_by(|a, b| b.total().total_cmp(&a.total()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn shifting_bottleneck_by_hand() {
        // A [0,10], B [2,14], C [3,20]: A is the bottleneck on [0,2), B on [2,3), C after.
        // Overlaps: A∩B = [2,10], B∩C = [3,14].
        let periods = vec![vec![(0.0, 10.0)], vec![(2.0, 14.0)], vec![(3.0, 20.0)]];
        let t = shifting_bottleneck_times(&periods);
        close(t[0].0, 2.0);
        close(t[0].1, 8.0);
        // B's whole period is shared: [2,10] with A and [3,14] with C.
        close(t[1].0, 0.0);
        close(t[1].1, 12.0);
        close(t[2].0, 6.0);
        close(t[2].1, 11.0);
    }

    #[test]
    fn alternating_stations_are_each_sole_bottleneck_in_turn() {
        // a works [5k, 5k+2], b works [5k+2, 5k+5]: periods only touch, nothing shared.
        let a: Vec<(f64, f64)> = (0..4)
            .map(|k| (5.0 * k as f64, 5.0 * k as f64 + 2.0))
            .collect();
        let b: Vec<(f64, f64)> = (0..4)
            .map(|k| (5.0 * k as f64 + 2.0, 5.0 * k as f64 + 5.0))
            .collect();
        let t = shifting_bottleneck_times(&[a, b]);
        close(t[0].0, 8.0);
        close(t[1].0, 12.0);
        close(t[0].1 + t[1].1, 0.0);
        // A resource inside a longer period of another is never the bottleneck.
        let t = shifting_bottleneck_times(&[vec![(1.0, 3.0), (5.0, 6.0)], vec![(0.0, 10.0)]]);
        close(t[0].0 + t[0].1, 0.0);
        close(t[1].0, 10.0);
        assert!(
            shifting_bottleneck_times(&[vec![], vec![]])
                .iter()
                .all(|&(s, h)| s == 0.0 && h == 0.0)
        );
    }

    #[test]
    fn active_periods_merge_touching_stretches_and_clip() {
        let rec = |t: f64, block: usize, state: State| TraceRecord {
            t,
            event: TraceEvent::State { block, state },
        };
        let records = vec![
            rec(0.0, 0, State::Idle),
            rec(1.0, 0, State::Setup),
            rec(2.0, 0, State::Processing),
            rec(4.0, 0, State::Idle),
            rec(4.0, 0, State::Processing),
            rec(6.0, 0, State::Blocked),
            rec(8.0, 0, State::Processing),
            rec(3.0, 1, State::TravelLoaded),
        ];
        let p = active_periods(&records, 2, 0.5, 9.0);
        assert_eq!(p[0], vec![(1.0, 6.0), (8.0, 9.0)]);
        assert_eq!(p[1], vec![(3.0, 9.0)]);
        // Clipped from 5 on.
        let p = active_periods(&records, 2, 5.0, 9.0);
        assert_eq!(p[0], vec![(5.0, 6.0), (8.0, 9.0)]);
    }

    #[test]
    fn transit_area_over_the_window() {
        let records = vec![
            TraceRecord {
                t: 1.0,
                event: TraceEvent::Transit {
                    item: 1,
                    from: 0,
                    to: 1,
                    delay: 1.0,
                    duration: 2.0,
                    path: vec![],
                },
            },
            TraceRecord {
                t: 9.0,
                event: TraceEvent::Transit {
                    item: 2,
                    from: 0,
                    to: 1,
                    delay: 0.0,
                    duration: 4.0,
                    path: vec![],
                },
            },
        ];
        // [1,4] inside [0,10] (3) plus [9,10] (1): 4 / 10.
        close(transit_mean(&records, 0.0, 10.0), 0.4);
        close(transit_mean(&records, 2.0, 10.0), 3.0 / 8.0);
    }

    #[test]
    fn parse_targets_and_series() {
        assert_eq!(parse_target("2%"), Some(Target::Relative(0.02)));
        assert_eq!(parse_target("0,5"), Some(Target::Absolute(0.5)));
        assert_eq!(parse_target("-1"), None);
        assert_eq!(
            WarmupSeries::parse("saida.time_in_system"),
            Some(WarmupSeries::Measure {
                object: "saida".into(),
                measure: "time_in_system".into()
            })
        );
        assert_eq!(WarmupSeries::parse("saida"), None);
    }
}
