//! Analyses to understand the dynamics and to compare (see
//! `docs/plano_analises_e_dinamica.md`, section 2, P2): the state Gantt of each object
//! from the trace, work in process and throughput over time with a confidence band
//! across replications, the comparison of several scenarios (ANOVA, Tukey, Bonferroni),
//! the correlation between metrics and the export of every observation in long format.
//! The statistics live in `utsim-stats`; the heat map of movements is in `utsim-scene`
//! (it reads the paths of the trace).

use std::io::{self, Write};

use serde::Serialize;
use utsim_core::{RunReport, State, TraceEvent};
use utsim_stats::compare::{self, Anova, Bonferroni, Tukey};
use utsim_stats::correlation::CorrelationMatrix;

use super::{default_precision_metrics, metric_values};
use crate::experiment::ExperimentResult;
use crate::replay::Replay;
use crate::{Model, ModelError, ObjectKind};

// ---------------------------------------------------------------------------------------
// State Gantt
// ---------------------------------------------------------------------------------------

/// The states of one object over time, `(start, end, state)`, consecutive and without
/// gaps from its first state event on.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GanttRow {
    pub name: String,
    /// Object type as written in the model (`Processor`, `Transporter`...).
    pub kind: String,
    pub segments: Vec<(f64, f64, State)>,
}

/// Exact state Gantt of every object of a traced replication, from its `State` events,
/// clipped to `[from, to]` (segments of zero length dropped, equal neighbours merged).
/// Objects that never report a state (sources without one, sinks) get no row.
pub fn state_gantt(replay: &Replay, from: f64, to: f64) -> Vec<GanttRow> {
    let n = replay.blocks.len();
    let mut rows: Vec<Vec<(f64, f64, State)>> = vec![Vec::new(); n];
    let mut open: Vec<Option<(f64, State)>> = vec![None; n];
    let push = |rows: &mut Vec<Vec<(f64, f64, State)>>, b: usize, s: f64, e: f64, st: State| {
        let (s, e) = (s.max(from), e.min(to));
        if e <= s {
            return;
        }
        match rows[b].last_mut() {
            Some(last) if last.2 == st && last.1 >= s => last.1 = e,
            _ => rows[b].push((s, e, st)),
        }
    };
    for r in &replay.records {
        if r.t > to {
            break;
        }
        let TraceEvent::State { block, state } = r.event else {
            continue;
        };
        if block >= n {
            continue;
        }
        if let Some((s, st)) = open[block] {
            push(&mut rows, block, s, r.t, st);
        }
        open[block] = Some((r.t, state));
    }
    for (b, o) in open.iter().enumerate() {
        if let Some((s, st)) = o {
            push(&mut rows, b, *s, to, *st);
        }
    }
    replay
        .blocks
        .iter()
        .zip(rows)
        .filter(|(_, segs)| !segs.is_empty())
        .map(|(b, segments)| GanttRow {
            name: b.name.clone(),
            kind: b.kind.clone(),
            segments,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// Series with a band across replications
// ---------------------------------------------------------------------------------------

/// Quantile `q` of `xs` (linear interpolation between order statistics, "type 7").
/// NaN when no value is finite.
pub fn quantile(xs: &[f64], q: f64) -> f64 {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    let pos = q.clamp(0.0, 1.0) * (v.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    v[lo] + (v[hi] - v[lo]) * (pos - lo as f64)
}

/// Mean across replications of curves sampled at the same instants, with the
/// confidence interval of the mean at `level` and the P5-P95 range of the replications:
/// `[t, mean, low, high, p5, p95]` per instant, over the shortest curve. Without an
/// interval (one replication) `low = high = mean`.
pub fn band(curves: &[Vec<[f64; 2]>], level: f64) -> Vec<[f64; 6]> {
    let len = curves.iter().map(Vec::len).min().unwrap_or(0);
    (0..len)
        .map(|k| {
            let xs: Vec<f64> = curves.iter().map(|c| c[k][1]).collect();
            let s = utsim_stats::summary(&xs, level);
            let (lo, hi) = if s.half_width.is_finite() {
                (s.mean - s.half_width, s.mean + s.half_width)
            } else {
                (s.mean, s.mean)
            };
            [
                curves[0][k][0],
                s.mean,
                lo,
                hi,
                quantile(&xs, 0.05),
                quantile(&xs, 0.95),
            ]
        })
        .collect()
}

/// Kinds whose content is work in process (sources, sinks and resources are not).
pub const WIP_KINDS: [&str; 6] = [
    "queue",
    "processor",
    "conveyor",
    "rack",
    "combiner",
    "separator",
];

/// Work in process over time, one curve per replication: the summed sampled content
/// of the objects in `group`, or of every object of a [`WIP_KINDS`] kind when `group`
/// is `None` (the whole system). Empty when the model samples nothing.
pub fn wip_curves(reports: &[RunReport], group: Option<&[String]>) -> Vec<Vec<[f64; 2]>> {
    reports
        .iter()
        .map(|r| {
            let blocks: Vec<_> = r
                .blocks
                .iter()
                .filter(|b| {
                    !b.series.is_empty()
                        && match group {
                            Some(g) => g.contains(&b.name),
                            None => WIP_KINDS.contains(&b.kind.as_str()),
                        }
                })
                .collect();
            let len = blocks.iter().map(|b| b.series.len()).min().unwrap_or(0);
            (0..len)
                .map(|k| {
                    let t = blocks[0].series[k].t;
                    [t, blocks.iter().map(|b| b.series[k].content as f64).sum()]
                })
                .collect()
        })
        .filter(|c: &Vec<[f64; 2]>| !c.is_empty())
        .collect()
}

/// [`wip_curves`] summarised by [`band`].
pub fn wip_band(reports: &[RunReport], group: Option<&[String]>, level: f64) -> Vec<[f64; 6]> {
    band(&wip_curves(reports, group), level)
}

/// Names of the sinks of a model, in model order.
pub fn sinks(model: &Model) -> Vec<String> {
    model
        .objects
        .iter()
        .filter(|o| matches!(o.kind, ObjectKind::Sink))
        .map(|o| o.name.clone())
        .collect()
}

/// Runs the replications keeping the instant of every arrival at the sinks (the
/// `time_in_system` observations), which [`throughput_curves`] needs. The statistics
/// are the same as [`Model::run_replications`].
pub fn run_for_throughput(model: &Model) -> Result<Vec<RunReport>, ModelError> {
    let measures: Vec<(String, String)> = sinks(model)
        .into_iter()
        .map(|s| (s, "time_in_system".to_string()))
        .collect();
    model.run_replications_timed(&measures)
}

/// Window of the throughput series when the user picks none: a twentieth of the
/// statistics window.
pub fn default_window(report: &RunReport) -> f64 {
    ((report.end_time - report.stats_from) / 20.0).max(f64::MIN_POSITIVE)
}

/// Throughput per window, one curve per replication: arrivals at `sinks` (all of them
/// when empty) counted in consecutive windows of length `window` from the start of the
/// statistics window, divided by `window`. Each point is `[end of the window, rate]`; a
/// last partial window is left out. Needs the timed observations of
/// [`run_for_throughput`] (replications without them give no curve).
pub fn throughput_curves(
    reports: &[RunReport],
    sinks: &[String],
    window: f64,
) -> Vec<Vec<[f64; 2]>> {
    if window.is_nan() || window <= 0.0 {
        return Vec::new();
    }
    reports
        .iter()
        .filter_map(|r| {
            let times: Vec<f64> = r
                .blocks
                .iter()
                .filter(|b| b.kind == "sink" && (sinks.is_empty() || sinks.contains(&b.name)))
                .filter_map(|b| b.extra.get("time_in_system")?.observations.as_ref())
                .flat_map(|obs| obs.iter().map(|o| o[0]))
                .collect();
            let any_timed = r.blocks.iter().any(|b| {
                b.extra
                    .get("time_in_system")
                    .is_some_and(|t| t.observations.is_some())
            });
            if !any_timed {
                return None;
            }
            let span = r.end_time - r.stats_from;
            let windows = (span / window + 1e-9).floor() as usize;
            let mut counts = vec![0u64; windows];
            for t in times {
                let k = ((t - r.stats_from) / window).floor();
                if k >= 0.0 && (k as usize) < windows {
                    counts[k as usize] += 1;
                }
            }
            Some(
                counts
                    .iter()
                    .enumerate()
                    .map(|(k, c)| [r.stats_from + window * (k + 1) as f64, *c as f64 / window])
                    .collect(),
            )
        })
        .filter(|c: &Vec<[f64; 2]>| !c.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------------------
// Comparing scenarios
// ---------------------------------------------------------------------------------------

/// Comparison of every scenario of an experiment on one metric.
#[derive(Clone, Debug, Serialize)]
pub struct ScenarioComparison {
    pub metric: String,
    pub scenarios: Vec<String>,
    pub level: f64,
    /// Mean and half-width of each scenario at `level`.
    pub summaries: Vec<utsim_core::stats::Summary>,
    /// One-way ANOVA over the scenarios (`None` with fewer than two, or one replication).
    pub anova: Option<Anova>,
    /// Tukey's HSD (independent groups).
    pub tukey: Option<Tukey>,
    /// Paired comparisons with Bonferroni (common random numbers: the scenarios share
    /// the seeds, so replication `r` is paired across them).
    pub bonferroni: Option<Bonferroni>,
}

/// ANOVA, Tukey and Bonferroni of `metric` over the scenarios of `result`. A scenario
/// without the metric counts as zeros (as in `summarize`).
pub fn compare_scenarios(
    result: &ExperimentResult,
    metric: &str,
    level: f64,
) -> ScenarioComparison {
    let values: Vec<Vec<f64>> = result
        .scenarios
        .iter()
        .map(|s| {
            let n = s.per_replication.values().map(Vec::len).max().unwrap_or(0);
            s.per_replication
                .get(metric)
                .cloned()
                .unwrap_or_else(|| vec![0.0; n])
        })
        .collect();
    let groups: Vec<&[f64]> = values.iter().map(Vec::as_slice).collect();
    let anova = compare::anova(&groups);
    let tukey = anova
        .as_ref()
        .filter(|a| a.ms_within > 0.0)
        .map(|a| compare::tukey(a, level));
    ScenarioComparison {
        metric: metric.to_string(),
        scenarios: result.scenarios.iter().map(|s| s.name.clone()).collect(),
        level,
        summaries: groups
            .iter()
            .map(|g| utsim_stats::summary(g, level))
            .collect(),
        anova,
        tukey,
        bonferroni: compare::bonferroni_paired(&groups, level),
    }
}

/// The metrics compared when the user names none: the experiment's `measures`, else
/// the default metrics of the precision analysis over the first scenario's values.
pub fn default_comparison_metrics(result: &ExperimentResult) -> Vec<String> {
    if !result.measures.is_empty() {
        return result.measures.clone();
    }
    let Some(first) = result.scenarios.first() else {
        return Vec::new();
    };
    let mut keep: Vec<String> = first
        .per_replication
        .iter()
        .filter(|(k, v)| {
            (k.ends_with(".utilization")
                || k.ends_with(".content.mean")
                || k.ends_with(".staytime.mean")
                || k.ends_with(".time_in_system.mean"))
                && v.iter().any(|x| *x != 0.0)
        })
        .map(|(k, _)| k.clone())
        .collect();
    // Metrics that differ between scenarios first: those are what the user changed.
    keep.sort_by_key(|k| {
        result
            .scenarios
            .iter()
            .all(|s| s.per_replication.get(k) == first.per_replication.get(k))
    });
    keep
}

// ---------------------------------------------------------------------------------------
// Correlation between metrics
// ---------------------------------------------------------------------------------------

/// Metrics correlated when the user names none: the throughput of every sink (its
/// arrivals) plus the default metrics of the precision analysis.
pub fn default_correlation_metrics(reports: &[RunReport]) -> Vec<String> {
    let Some(first) = reports.first() else {
        return Vec::new();
    };
    let mut out: Vec<String> = first
        .blocks
        .iter()
        .filter(|b| b.kind == "sink")
        .map(|b| format!("{}.input", b.name))
        .collect();
    out.extend(default_precision_metrics(reports));
    out.retain(|k| {
        let v = metric_values(reports, k);
        v.iter().any(|x| (x - v[0]).abs() > 0.0)
    });
    out.dedup();
    out
}

/// Pearson and Spearman correlations of `metrics` across the replications.
pub fn correlation(reports: &[RunReport], metrics: &[String]) -> CorrelationMatrix {
    let series: Vec<(String, Vec<f64>)> = metrics
        .iter()
        .map(|m| (m.clone(), metric_values(reports, m)))
        .collect();
    CorrelationMatrix::build(&series)
}

// ---------------------------------------------------------------------------------------
// Observations in long format
// ---------------------------------------------------------------------------------------

/// Runs the replications keeping the instant of every observation of every tally of
/// every object (`staytime` and the object's own tallies, such as `time_in_system`).
/// The tallies are discovered in a first run of replication 0 alone. The statistics
/// are the same as [`Model::run_replications`].
pub fn run_for_observations(model: &Model) -> Result<Vec<RunReport>, ModelError> {
    let mut probe = model.clone();
    probe.run.replications = 1;
    let first = probe.run_replications()?;
    let mut measures = Vec::new();
    if let Some(r) = first.first() {
        for b in &r.blocks {
            measures.push((b.name.clone(), "staytime".to_string()));
            for k in b.extra.keys() {
                measures.push((b.name.clone(), k.clone()));
            }
        }
    }
    model.run_replications_timed(&measures)
}

/// Header of the long-format CSV.
pub const OBSERVATIONS_HEADER: &str = "replicacao,instante,objeto,medida,valor";

/// Writes every observation of `reports` as CSV in long format, one row per value:
/// `replicacao,instante,objeto,medida,valor`. Measures: `conteudo` (each sample of the
/// content, when the model samples), `staytime` and every own tally of the objects
/// (with the instants kept by [`run_for_observations`]). Returns the rows written.
pub fn write_observations(reports: &[RunReport], out: &mut impl Write) -> io::Result<u64> {
    use crate::csv::{field, num};
    writeln!(out, "{OBSERVATIONS_HEADER}")?;
    let mut rows = 0u64;
    for r in reports {
        let rep = r.replication;
        for b in &r.blocks {
            let name = field(&b.name);
            for p in &b.series {
                writeln!(out, "{rep},{},{name},conteudo,{}", num(p.t), p.content)?;
                rows += 1;
            }
            let tallies = std::iter::once(("staytime", &b.staytime))
                .chain(b.extra.iter().map(|(k, t)| (k.as_str(), t)));
            for (measure, t) in tallies {
                let Some(obs) = &t.observations else { continue };
                let m = field(measure);
                for o in obs {
                    writeln!(out, "{rep},{},{name},{m},{}", num(o[0]), num(o[1]))?;
                    rows += 1;
                }
            }
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::Scenario;

    const LINE: &str = r#"Model(
        name: "linha",
        run: (stop_time: 100.0, seed: 3, replications: 3, sample_interval: Some(10.0)),
        objects: [
            (name: "chegada", kind: Source(interarrival: Const(10.0))),
            (name: "fila", kind: Queue()),
            (name: "maquina", kind: Processor(process: Const(4.0))),
            (name: "saida", kind: Sink),
        ],
        connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
    )"#;

    #[test]
    fn gantt_follows_the_state_events_exactly() {
        let model = Model::parse(LINE).unwrap();
        let replay = model.replay(0).unwrap();
        let rows = state_gantt(&replay, 0.0, 100.0);
        let m = rows.iter().find(|r| r.name == "maquina").unwrap();
        // Deterministic line: one item every 10, 4 of processing each.
        let busy: f64 = m
            .segments
            .iter()
            .filter(|s| s.2 == State::Processing)
            .map(|s| s.1 - s.0)
            .sum();
        let starts: Vec<f64> = m
            .segments
            .iter()
            .filter(|s| s.2 == State::Processing)
            .map(|s| s.0)
            .collect();
        assert!(starts.len() >= 9, "{starts:?}");
        for w in starts.windows(2) {
            assert!((w[1] - w[0] - 10.0).abs() < 1e-9, "{starts:?}");
        }
        assert!(busy > 35.0 && busy <= 40.0 + 1e-9, "{busy}");
        // Contiguous, ordered and inside the window.
        for w in m.segments.windows(2) {
            assert!((w[0].1 - w[1].0).abs() < 1e-12 && w[0].2 != w[1].2);
        }
        assert!(m.segments.first().unwrap().0 >= 0.0);
        assert!((m.segments.last().unwrap().1 - 100.0).abs() < 1e-12);
        // Clipping keeps only the window.
        let part = state_gantt(&replay, 20.0, 30.0);
        let m = part.iter().find(|r| r.name == "maquina").unwrap();
        assert!((m.segments[0].0 - 20.0).abs() < 1e-12);
        assert!((m.segments.last().unwrap().1 - 30.0).abs() < 1e-12);
    }

    #[test]
    fn band_and_quantile_by_hand() {
        assert_eq!(quantile(&[1.0, 2.0, 3.0, 4.0], 0.5), 2.5);
        assert!(quantile(&[f64::NAN], 0.5).is_nan());
        let curves = vec![
            vec![[0.0, 1.0], [1.0, 2.0]],
            vec![[0.0, 3.0], [1.0, 4.0], [2.0, 9.0]],
            vec![[0.0, 5.0], [1.0, 6.0]],
        ];
        let b = band(&curves, 0.95);
        assert_eq!(b.len(), 2, "shortest curve");
        assert_eq!(b[0][1], 3.0);
        // s = 2, n = 3: half-width 4.303 · 2 / √3.
        let hw = 4.303 * 2.0 / 3f64.sqrt();
        assert!((b[0][3] - b[0][1] - hw).abs() < 1e-12);
        assert!((b[0][4] - 1.2).abs() < 1e-12 && (b[0][5] - 4.8).abs() < 1e-12);
        let one = band(&curves[..1], 0.95);
        assert_eq!((one[0][2], one[0][3]), (1.0, 1.0));
    }

    #[test]
    fn wip_and_throughput_of_a_deterministic_line() {
        let model = Model::parse(LINE).unwrap();
        let reports = run_for_throughput(&model).unwrap();
        // WIP: the machine holds an item 4 of every 10; sampled at 0, 10, 20... it is
        // empty or busy depending on the phase, the same in every replication.
        let w = wip_band(&reports, None, 0.95);
        assert!(!w.is_empty());
        assert!(
            w.iter().all(|p| p[2] == p[1] && p[3] == p[1]),
            "no spread: {w:?}"
        );
        let g = wip_band(&reports, Some(&["fila".to_string()]), 0.95);
        assert!(g.iter().all(|p| p[1] == 0.0), "the queue never holds");
        // Throughput: arrivals at 10, 20...; exits 4 later (14, 24...). Windows of 20
        // hold two exits (0.1 per time unit), except the first, which holds only 14.
        let tp = throughput_curves(&reports, &[], 20.0);
        assert_eq!(tp.len(), 3);
        assert_eq!(tp[0].len(), 5);
        assert!((tp[0][0][1] - 0.05).abs() < 1e-12, "{:?}", tp[0]);
        for p in &tp[0][1..] {
            assert!((p[1] - 0.1).abs() < 1e-12, "{:?}", tp[0]);
        }
        assert_eq!(tp[0][0][0], 20.0);
        // Without timed observations there is no curve.
        let plain = model.run_replications().unwrap();
        assert!(throughput_curves(&plain, &[], 20.0).is_empty());
        assert_eq!(default_window(&plain[0]), 5.0);
    }

    #[test]
    fn scenarios_are_compared_with_anova_tukey_and_bonferroni() {
        let mut base = Model::parse(include_str!("../../../../examples/mm1.ron")).unwrap();
        base.run.replications = 6;
        base.run.stop_time = 6000.0;
        let sc = |name: &str, mean: &str| Scenario {
            name: name.to_string(),
            set: [("maquina.process".to_string(), format!("Exp(mean: {mean})"))]
                .into_iter()
                .collect(),
        };
        let scenarios = [sc("lenta", "0.9"), sc("media", "0.8"), sc("rapida", "0.5")];
        let res = crate::experiment::run_scenarios(&base, &scenarios, &[], &|_, _| true).unwrap();
        let result = ExperimentResult {
            name: "x".into(),
            measures: vec![],
            scenarios: res,
        };
        let c = compare_scenarios(&result, "maquina.utilization", 0.95);
        assert_eq!(c.scenarios, ["lenta", "media", "rapida"]);
        let a = c.anova.as_ref().unwrap();
        assert_eq!((a.groups, a.df_within), (3, 15));
        assert!(
            a.p_value < 1e-6,
            "utilisations 0.9, 0.8, 0.5: {}",
            a.p_value
        );
        let t = c.tukey.as_ref().unwrap();
        assert!(t.pairs.iter().all(|p| p.significant));
        let b = c.bonferroni.as_ref().unwrap();
        assert_eq!((b.comparisons, b.n), (3, 6));
        // Common random numbers: the paired interval is narrower than Tukey's.
        assert!(b.pairs[0].half_width < t.pairs[0].half_width);
        let metrics = default_comparison_metrics(&result);
        assert!(metrics.contains(&"maquina.utilization".to_string()));
    }

    #[test]
    fn correlation_of_queue_and_wait() {
        let mut model = Model::parse(include_str!("../../../../examples/mm1.ron")).unwrap();
        model.run.replications = 12;
        model.run.stop_time = 8000.0;
        let reports = model.run_replications().unwrap();
        let metrics = default_correlation_metrics(&reports);
        assert!(metrics.contains(&"saida.input".to_string()), "{metrics:?}");
        let m = correlation(
            &reports,
            &["fila.content.mean".into(), "fila.staytime.mean".into()],
        );
        // Little: a longer queue goes with a longer wait.
        assert!(m.pearson[0][1] > 0.8, "{}", m.pearson[0][1]);
        assert_eq!(m.n, 12);
    }

    #[test]
    fn observations_are_written_in_long_format() {
        let model = Model::parse(LINE).unwrap();
        let reports = run_for_observations(&model).unwrap();
        let plain = model.run_replications().unwrap();
        // Same statistics with the instants kept.
        assert_eq!(
            reports[1].block("saida").unwrap().input,
            plain[1].block("saida").unwrap().input
        );
        let mut buf = Vec::new();
        let rows = write_observations(&reports, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some(OBSERVATIONS_HEADER));
        assert_eq!(lines.clone().count() as u64, rows);
        assert!(text.contains("\n0,10,fila,conteudo,0\n"), "{text}");
        let tis: Vec<&str> = text
            .lines()
            .filter(|l| l.contains(",saida,time_in_system,"))
            .collect();
        // Every item waits 4 (processing) in the system.
        assert!(!tis.is_empty());
        assert!(tis.iter().all(|l| l.ends_with(",4")), "{tis:?}");
        let saida = reports[0].block("saida").unwrap();
        let count = saida.extra["time_in_system"].count as usize;
        assert_eq!(tis.iter().filter(|l| l.starts_with("0,")).count(), count);
    }
}
