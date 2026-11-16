//! Exposure of people on the floor, computed on the trace of one replication (stage QS5 of
//! `docs/plano_qualidade_e_seguranca.md`, section 4.6): time each person spends inside
//! each risk zone (`Model::zones`), entries without the protective equipment the zone
//! asks for, **simulated near-conflicts** (a person and a vehicle closer than a threshold),
//! a heat map of presence, layout warnings and the time of each person **under the
//! suspended load of an overhead crane** (inside the crane's `load_zone` around the hook
//! while an item hangs from it).
//!
//! The simulated near-conflicts compare layouts and traffic rules of the same model. They
//! do not estimate the probability of a real accident.
//!
//! Movement is rebuilt from the `Travel` events: a mobile resource goes along the path at
//! constant speed during the travel (accelerations are not modelled here) and stays where
//! it stopped until the next travel. Coordinates are metres on the floor (`x`, `y`).

use std::collections::BTreeSet;

use crate::{LoadZone, Model, ModelError, ObjectKind, ZoneKind};
use serde::{Deserialize, Serialize};
use utsim_core::TraceEvent;
use utsim_nav::Role;

/// Most sampling instants of the proximity and of the heat map per replication.
pub const MAX_SAMPLES: usize = 2_000_000;

/// Default distance of a simulated near-conflict (m), decision S4 of the plan.
pub const NEAR_MISS_DISTANCE: f64 = 1.5;

/// Analysis settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Options {
    /// A person and a vehicle closer than this (m) are a simulated near-conflict.
    pub near_miss: f64,
    /// Sampling step (time units of the model) of the proximity and of the heat map.
    pub step: f64,
    /// Side of a heat map cell (m).
    pub cell: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            near_miss: NEAR_MISS_DISTANCE,
            step: 0.1,
            cell: 1.0,
        }
    }
}

/// A straight move, or a stop when `p0 == p1`, between two instants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub t0: f64,
    pub t1: f64,
    pub p0: [f64; 2],
    pub p1: [f64; 2],
}

impl Move {
    fn at(&self, t: f64) -> [f64; 2] {
        let d = self.t1 - self.t0;
        let f = if d > 0.0 {
            ((t - self.t0) / d).clamp(0.0, 1.0)
        } else {
            1.0
        };
        lerp(self.p0, self.p1, f)
    }
}

/// Where one mobile resource was, from 0 to the end of the run.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// Object index in the model.
    pub object: usize,
    pub name: String,
    pub role: Role,
    /// Protective equipment the resource wears: its own `ppe` and that of the objects it
    /// serves.
    pub ppe: BTreeSet<String>,
    /// Moves in time order, without gaps.
    pub moves: Vec<Move>,
}

impl Track {
    pub fn is_person(&self) -> bool {
        self.role == Role::Person
    }

    /// Position at instant `t`.
    pub fn at(&self, t: f64) -> [f64; 2] {
        position_at(&self.moves, t)
    }

    /// Intervals of time spent inside `polygon`, within `[from, to]`, merged.
    pub fn inside(&self, polygon: &[[f64; 2]], from: f64, to: f64) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = Vec::new();
        for m in &self.moves {
            let (a, b) = (m.t0.max(from), m.t1.min(to));
            if b <= a {
                continue;
            }
            let d = m.t1 - m.t0;
            let (fa, fb) = if d > 0.0 {
                ((a - m.t0) / d, (b - m.t0) / d)
            } else {
                (0.0, 1.0)
            };
            let (q0, q1) = (lerp(m.p0, m.p1, fa), lerp(m.p0, m.p1, fb));
            for (s0, s1) in segment_inside(q0, q1, polygon) {
                let (u0, u1) = (a + (b - a) * s0, a + (b - a) * s1);
                match out.last_mut() {
                    Some(last) if (u0 - last.1).abs() < 1e-9 => last.1 = u1,
                    _ => out.push((u0, u1)),
                }
            }
        }
        out
    }
}

fn position_at(moves: &[Move], t: f64) -> [f64; 2] {
    let k = moves.partition_point(|m| m.t1 < t);
    match moves.get(k).or(moves.last()) {
        Some(m) => m.at(t),
        None => [0.0, 0.0],
    }
}

fn lerp(a: [f64; 2], b: [f64; 2], f: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Whether point `p` is inside `polygon` (even-odd rule; the boundary counts as inside
/// only up to rounding).
pub fn contains(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0];
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Parts `(s0, s1)` of the segment `p0 → p1`, as fractions of it in `[0, 1]`, that lie
/// inside `polygon`. Exact: the segment is cut where it crosses an edge.
pub fn segment_inside(p0: [f64; 2], p1: [f64; 2], polygon: &[[f64; 2]]) -> Vec<(f64, f64)> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    if p0 == p1 {
        return if contains(polygon, p0) {
            vec![(0.0, 1.0)]
        } else {
            Vec::new()
        };
    }
    let mut cuts = vec![0.0, 1.0];
    let r = [p1[0] - p0[0], p1[1] - p0[1]];
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let s = [b[0] - a[0], b[1] - a[1]];
        let den = r[0] * s[1] - r[1] * s[0];
        if den.abs() < 1e-15 {
            continue;
        }
        let q = [a[0] - p0[0], a[1] - p0[1]];
        let t = (q[0] * s[1] - q[1] * s[0]) / den;
        let u = (q[0] * r[1] - q[1] * r[0]) / den;
        if (0.0..=1.0).contains(&t) && (-1e-12..=1.0 + 1e-12).contains(&u) {
            cuts.push(t);
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for w in cuts.windows(2) {
        let mid = lerp(p0, p1, (w[0] + w[1]) / 2.0);
        if contains(polygon, mid) {
            match out.last_mut() {
                Some(last) if (last.1 - w[0]).abs() < 1e-12 => last.1 = w[1],
                _ => out.push((w[0], w[1])),
            }
        }
    }
    out
}

/// Time of one person inside one zone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZoneExposure {
    pub zone: String,
    pub kind: ZoneKind,
    pub person: String,
    /// Time inside the zone in the statistics window.
    pub time: f64,
    /// Times the person went in.
    pub entries: u32,
    /// Entries while missing some equipment the zone asks for.
    pub entries_without_ppe: u32,
    /// Equipment the zone asks for that the person does not wear.
    pub missing_ppe: Vec<String>,
}

/// Time of one person under the suspended load of one overhead crane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnderLoad {
    pub crane: String,
    pub person: String,
    /// Time under the load in the statistics window.
    pub time: f64,
    /// Times the person came under it.
    pub entries: u32,
}

/// The hook of one overhead crane: where it was and when an item hung from it.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadTrack {
    /// Object index in the model.
    pub object: usize,
    pub name: String,
    pub zone: LoadZone,
    /// Floor position of the hook, in time order, without gaps.
    pub moves: Vec<Move>,
    /// Intervals with an item hanging: from the hooking to the touch down at the
    /// destination (the end of the last move of the hook before the item is delivered).
    pub hanging: Vec<(f64, f64)>,
}

impl LoadTrack {
    /// Floor position of the hook at instant `t`.
    pub fn at(&self, t: f64) -> [f64; 2] {
        position_at(&self.moves, t)
    }
}

/// A simulated near-conflict: a person and a vehicle closer than the threshold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NearMiss {
    /// Instant the distance went below the threshold.
    pub t: f64,
    pub person: String,
    pub vehicle: String,
    /// Smallest distance during the episode.
    pub min_distance: f64,
    /// Middle point between the two at the smallest distance.
    pub at: [f64; 2],
    /// How long the episode lasted.
    pub duration: f64,
}

/// Presence of people and near-conflicts per floor cell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeatMap {
    /// Lower-left corner of the grid.
    pub origin: [f64; 2],
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
    /// Person-time per cell, row by row (`iy * nx + ix`).
    pub presence: Vec<f64>,
    /// Near-conflicts per cell.
    pub near_misses: Vec<u32>,
}

impl HeatMap {
    fn index(&self, p: [f64; 2]) -> Option<usize> {
        let ix = ((p[0] - self.origin[0]) / self.cell).floor();
        let iy = ((p[1] - self.origin[1]) / self.cell).floor();
        (ix >= 0.0 && iy >= 0.0 && (ix as usize) < self.nx && (iy as usize) < self.ny)
            .then(|| iy as usize * self.nx + ix as usize)
    }
}

/// Exposure of one replication.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Exposure {
    pub replication: u32,
    /// Statistics window `[from, to]`.
    pub from: f64,
    pub to: f64,
    pub options: Options,
    pub zones: Vec<ZoneExposure>,
    pub near_misses: Vec<NearMiss>,
    pub heat: HeatMap,
    pub warnings: Vec<String>,
    /// Time of each person under each crane's suspended load (models with cranes; empty
    /// when measured by [`measure`] alone, filled by [`analyze`]).
    #[serde(default)]
    pub under_load: Vec<UnderLoad>,
}

impl Exposure {
    /// Total time of all people under suspended loads.
    pub fn under_load_time(&self) -> f64 {
        self.under_load.iter().map(|u| u.time).sum()
    }

    /// Times people came under a suspended load.
    pub fn under_load_entries(&self) -> u32 {
        self.under_load.iter().map(|u| u.entries).sum()
    }

    pub fn near_miss_count(&self) -> usize {
        self.near_misses.len()
    }

    /// Total time of all people inside zones of the given kind.
    pub fn time_in(&self, kind: ZoneKind) -> f64 {
        self.zones
            .iter()
            .filter(|z| z.kind == kind)
            .map(|z| z.time)
            .sum()
    }

    pub fn entries_without_ppe(&self) -> u32 {
        self.zones.iter().map(|z| z.entries_without_ppe).sum()
    }
}

/// Mean over the replications with the half-width of the 95 % interval.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    pub mean: f64,
    pub half_width: f64,
    pub n: usize,
}

pub fn interval(values: &[f64]) -> Interval {
    let n = values.len();
    let mean = values.iter().sum::<f64>() / n.max(1) as f64;
    let half_width = if n > 1 {
        let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
        utsim_stats::t_quantile(0.95, n - 1) * (var / n as f64).sqrt()
    } else {
        f64::NAN
    };
    Interval {
        mean,
        half_width,
        n,
    }
}

/// Tracks of every mobile resource of the model, from the trace of a replay.
pub fn tracks(model: &Model, records: &[utsim_core::TraceRecord], stop: f64) -> Vec<Track> {
    let mut out = Vec::new();
    for (i, obj) in model.objects.iter().enumerate() {
        let ObjectKind::Transporter { home_at, .. } = &obj.kind else {
            continue;
        };
        let start = home_at.unwrap_or(obj.position);
        let moves = moves_of(i, start, records, stop);
        let ppe = model.worn_ppe(obj);
        out.push(Track {
            object: i,
            name: obj.name.clone(),
            role: model.role_of(obj),
            ppe,
            moves,
        });
    }
    out
}

/// Moves of mobile resource `i` (block index) from its `start`, rebuilt from its
/// `Travel` events.
fn moves_of(
    i: usize,
    start: [f64; 3],
    records: &[utsim_core::TraceRecord],
    stop: f64,
) -> Vec<Move> {
    let mut pos = [start[0], start[1]];
    let mut t = 0.0;
    let mut moves = Vec::new();
    for r in records {
        let TraceEvent::Travel {
            carrier,
            path,
            duration,
        } = &r.event
        else {
            continue;
        };
        if *carrier != i || path.is_empty() {
            continue;
        }
        if r.t > t {
            moves.push(Move {
                t0: t,
                t1: r.t,
                p0: pos,
                p1: pos,
            });
        }
        let pts: Vec<[f64; 2]> = path.iter().map(|p| [p[0], p[1]]).collect();
        let total: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        let mut tt = r.t;
        if pts.len() < 2 || total <= 0.0 {
            moves.push(Move {
                t0: tt,
                t1: tt + duration,
                p0: pts[0],
                p1: pts[pts.len() - 1],
            });
        } else {
            for w in pts.windows(2) {
                let d = duration * dist(w[0], w[1]) / total;
                moves.push(Move {
                    t0: tt,
                    t1: tt + d,
                    p0: w[0],
                    p1: w[1],
                });
                tt += d;
            }
        }
        t = r.t + duration;
        pos = pts[pts.len() - 1];
    }
    if t < stop {
        moves.push(Move {
            t0: t,
            t1: stop,
            p0: pos,
            p1: pos,
        });
    }
    moves
}

/// Hooks of every overhead crane of the model, from the trace of a replay.
pub fn load_tracks(
    model: &Model,
    records: &[utsim_core::TraceRecord],
    stop: f64,
) -> Vec<LoadTrack> {
    let mut out = Vec::new();
    for (i, obj) in model.objects.iter().enumerate() {
        let Some(zone) = model.crane_load_zone(obj) else {
            continue;
        };
        let mut hanging = Vec::new();
        // Moves of the hook (start, end): the travels and the hoisting.
        let mut motions: Vec<(f64, f64)> = Vec::new();
        let mut held = 0u32;
        let mut since = 0.0;
        for r in records {
            match &r.event {
                TraceEvent::Travel {
                    carrier, duration, ..
                }
                | TraceEvent::Hoist {
                    carrier, duration, ..
                } if *carrier == i => motions.push((r.t, r.t + duration)),
                TraceEvent::Moved { to, .. } if *to == i => {
                    if held == 0 {
                        since = r.t;
                    }
                    held += 1;
                }
                TraceEvent::Moved { from, .. } if *from == i && held > 0 => {
                    held -= 1;
                    if held == 0 {
                        // The load touched down at the end of the last move started
                        // before the delivery (the raise afterwards does not count).
                        let end = motions
                            .iter()
                            .filter(|(s, _)| *s >= since && *s < r.t)
                            .map(|m| m.1)
                            .fold(since, f64::max)
                            .min(r.t);
                        if end > since {
                            hanging.push((since, end));
                        }
                    }
                }
                _ => {}
            }
        }
        if held > 0 && stop > since {
            hanging.push((since, stop));
        }
        out.push(LoadTrack {
            object: i,
            name: obj.name.clone(),
            zone,
            moves: moves_of(i, obj.position, records, stop),
            hanging,
        });
    }
    out
}

/// Time of each person under each crane's suspended load in `[from, to]`, sampled every
/// `step` (at most [`MAX_SAMPLES`] instants per pair).
pub fn under_loads(
    people: &[&Track],
    loads: &[LoadTrack],
    from: f64,
    to: f64,
    step: f64,
) -> Vec<UnderLoad> {
    let mut out = Vec::new();
    for load in loads {
        let spans: Vec<(f64, f64)> = load
            .hanging
            .iter()
            .map(|&(a, b)| (a.max(from), b.min(to)))
            .filter(|(a, b)| b > a)
            .collect();
        let total: f64 = spans.iter().map(|(a, b)| b - a).sum();
        let mut step = if step.is_finite() {
            step.max(1e-9)
        } else {
            1.0
        };
        if total.is_finite() && total / step > MAX_SAMPLES as f64 {
            step = total / MAX_SAMPLES as f64;
        }
        for p in people {
            let mut time = 0.0;
            let mut entries = 0;
            for &(a, b) in &spans {
                let n = ((b - a) / step).ceil().clamp(1.0, MAX_SAMPLES as f64) as usize;
                let dt = (b - a) / n as f64;
                let mut was_in = false;
                for k in 0..n {
                    let t = a + (k as f64 + 0.5) * dt;
                    let inside = load.zone.contains(load.at(t), p.at(t));
                    if inside {
                        time += dt;
                        if !was_in {
                            entries += 1;
                        }
                    }
                    was_in = inside;
                }
            }
            out.push(UnderLoad {
                crane: load.name.clone(),
                person: p.name.clone(),
                time,
                entries,
            });
        }
    }
    out
}

impl Model {
    /// Whether the model has an overhead crane and people moving on the floor: the run
    /// then measures the time of the people under the suspended loads.
    pub fn has_people_under_cranes(&self) -> bool {
        self.objects
            .iter()
            .any(|o| matches!(o.kind, ObjectKind::Crane { .. }))
            && self.objects.iter().any(|o| {
                matches!(o.kind, ObjectKind::Transporter { .. }) && self.role_of(o) == Role::Person
            })
    }

    /// Protective equipment a mobile resource wears: its own `ppe` and that of the objects
    /// it serves (the exposure analysis and the people of the 3D view read the same set).
    pub fn worn_ppe(&self, obj: &crate::ObjectDef) -> BTreeSet<String> {
        let mut ppe: BTreeSet<String> = obj.ppe.iter().cloned().collect();
        for served in self.served_by(&obj.name) {
            ppe.extend(served.ppe.iter().cloned());
        }
        ppe
    }
}

/// Runs replication `replication` with its trace and measures the exposure.
pub fn analyze(model: &Model, replication: u32, options: Options) -> Result<Exposure, ModelError> {
    let replay = model.replay(replication)?;
    let stop = replay.report.end_time;
    let from = replay.report.stats_from;
    let tracks = tracks(model, &replay.records, stop);
    let mut e = measure(model, &tracks, replication, from, stop, options);
    let people: Vec<&Track> = tracks.iter().filter(|t| t.is_person()).collect();
    let loads = load_tracks(model, &replay.records, stop);
    e.under_load = under_loads(&people, &loads, from, stop, options.step);
    Ok(e)
}

/// Exposure of every replication of the model (`run.replications`).
pub fn analyze_all(model: &Model, options: Options) -> Result<Vec<Exposure>, ModelError> {
    (0..model.run.replications)
        .map(|r| analyze(model, r, options))
        .collect()
}

/// The measures, on tracks already built.
pub fn measure(
    model: &Model,
    tracks: &[Track],
    replication: u32,
    from: f64,
    to: f64,
    options: Options,
) -> Exposure {
    let people: Vec<&Track> = tracks.iter().filter(|t| t.is_person()).collect();
    let vehicles: Vec<&Track> = tracks.iter().filter(|t| !t.is_person()).collect();
    let mut zones = Vec::new();
    for z in &model.zones {
        for p in &people {
            let spans = p.inside(&z.polygon, from, to);
            let missing: Vec<String> = z
                .ppe
                .iter()
                .filter(|e| !p.ppe.contains(*e))
                .cloned()
                .collect();
            let entries = spans.len() as u32;
            zones.push(ZoneExposure {
                zone: z.name.clone(),
                kind: z.kind,
                person: p.name.clone(),
                time: spans.iter().map(|(a, b)| b - a).sum(),
                entries,
                entries_without_ppe: if missing.is_empty() { 0 } else { entries },
                missing_ppe: missing,
            });
        }
    }
    let mut heat = empty_heat(model, tracks, options.cell);
    // At most MAX_SAMPLES instants per pair: a longer window takes a longer step.
    let janela = (to - from).max(0.0);
    let mut step = if options.step.is_finite() {
        options.step.max(1e-9)
    } else {
        1.0
    };
    if janela.is_finite() && janela / step > MAX_SAMPLES as f64 {
        step = janela / MAX_SAMPLES as f64;
    }
    let samples = if janela.is_finite() {
        (janela / step).floor().min(MAX_SAMPLES as f64) as usize
    } else {
        0
    };
    let mut near_misses = Vec::new();
    for p in &people {
        for k in 0..samples {
            let t = from + (k as f64 + 0.5) * step;
            if let Some(i) = heat.index(p.at(t)) {
                heat.presence[i] += step;
            }
        }
        for v in &vehicles {
            let mut open: Option<NearMiss> = None;
            for k in 0..=samples {
                let t = from + k as f64 * step;
                let (a, b) = (p.at(t), v.at(t));
                let d = dist(a, b);
                if d < options.near_miss {
                    let o = open.get_or_insert(NearMiss {
                        t,
                        person: p.name.clone(),
                        vehicle: v.name.clone(),
                        min_distance: d,
                        at: lerp(a, b, 0.5),
                        duration: 0.0,
                    });
                    o.duration = t - o.t;
                    if d < o.min_distance {
                        o.min_distance = d;
                        o.at = lerp(a, b, 0.5);
                    }
                } else if let Some(o) = open.take() {
                    near_misses.push(o);
                }
            }
            near_misses.extend(open);
        }
    }
    near_misses.sort_by(|a, b| a.t.total_cmp(&b.t));
    for n in &near_misses {
        if let Some(i) = heat.index(n.at) {
            heat.near_misses[i] += 1;
        }
    }
    Exposure {
        replication,
        from,
        to,
        options,
        zones,
        near_misses,
        heat,
        warnings: layout_warnings(model),
        under_load: Vec::new(),
    }
}

fn empty_heat(model: &Model, tracks: &[Track], cell: f64) -> HeatMap {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut add = |p: [f64; 2]| {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    };
    for o in &model.objects {
        add([o.position[0], o.position[1]]);
    }
    for z in &model.zones {
        z.polygon.iter().for_each(|p| add(*p));
    }
    for t in tracks {
        for m in &t.moves {
            add(m.p0);
            add(m.p1);
        }
    }
    if !lo[0].is_finite() {
        lo = [0.0, 0.0];
        hi = [0.0, 0.0];
    }
    let cell = if cell > 0.0 && cell.is_finite() {
        cell
    } else {
        1.0
    };
    let origin = [
        (lo[0] / cell).floor() * cell - cell,
        (lo[1] / cell).floor() * cell - cell,
    ];
    // At most 2000 cells per side, clamped before the conversion: huge or non-finite
    // coordinates must not overflow (NaN becomes 0).
    let lado = |d: f64| ((d / cell).ceil().clamp(0.0, 1999.0) as usize) + 1;
    let nx = lado(hi[0] - origin[0]);
    let ny = lado(hi[1] - origin[1]);
    HeatMap {
        origin,
        cell,
        nx,
        ny,
        presence: vec![0.0; nx * ny],
        near_misses: vec![0; nx * ny],
    }
}

/// Layout checks for the safety inventory: work stations inside vehicle zones and aisles
/// shared by people and vehicles without a stop before crossings.
pub fn layout_warnings(model: &Model) -> Vec<String> {
    let mut out = Vec::new();
    for o in &model.objects {
        let station = !o.ppe.is_empty()
            || matches!(
                o.kind,
                ObjectKind::Processor {
                    operators: Some(_),
                    ..
                }
            );
        if !station {
            continue;
        }
        for z in model.zones.iter().filter(|z| z.kind == ZoneKind::Vehicle) {
            if contains(&z.polygon, [o.position[0], o.position[1]]) {
                out.push(format!(
                    "posto '{}' dentro da zona de circulação de veículos '{}'",
                    o.name, z.name
                ));
            }
        }
    }
    let mixed = model
        .aisles
        .iter()
        .any(|a| a.kind == utsim_nav::AisleKind::Mixed);
    let stop = model.floor_marking.as_ref().map(|f| f.crossing_stop);
    if mixed && stop == Some(0.0) {
        out.push(
            "corredor misto de pessoas e veículos sem parada obrigatória antes dos cruzamentos"
                .to_string(),
        );
    }
    out
}

/// The exposure of one replication as flat keys, for the teaching missions and the
/// experiments: `exposicao.quase_conflitos`, `exposicao.sem_epi`, `exposicao.tempo.<zona>`
/// (all people) and `exposicao.entradas.<zona>`.
pub fn metricas(e: &Exposure) -> std::collections::BTreeMap<String, f64> {
    let mut out = std::collections::BTreeMap::new();
    out.insert(
        "exposicao.quase_conflitos".into(),
        e.near_miss_count() as f64,
    );
    out.insert(
        "exposicao.sem_epi".into(),
        f64::from(e.entries_without_ppe()),
    );
    for z in &e.zones {
        *out.entry(format!("exposicao.tempo.{}", z.zone))
            .or_insert(0.0) += z.time;
        *out.entry(format!("exposicao.entradas.{}", z.zone))
            .or_insert(0.0) += f64::from(z.entries);
    }
    if !e.under_load.is_empty() {
        out.insert("exposicao.sob_carga.tempo".into(), e.under_load_time());
        out.insert(
            "exposicao.sob_carga.entradas".into(),
            f64::from(e.under_load_entries()),
        );
    }
    out
}

/// Puts the time of the people under suspended loads into the report: for each person
/// `<pessoa>.tempo_sob_carga` and `<pessoa>.entradas_sob_carga`, for each crane
/// `<ponte>.tempo_sob_carga` and `<ponte>.entradas_sob_carga` (all its people), with
/// the default [`Options`]. Called by `Model::drive` for models with cranes and people.
pub fn add_under_load_to_report(
    model: &Model,
    report: &mut utsim_core::RunReport,
    records: &[utsim_core::TraceRecord],
) {
    let stop = report.end_time;
    let tracks = tracks(model, records, stop);
    let people: Vec<&Track> = tracks.iter().filter(|t| t.is_person()).collect();
    let loads = load_tracks(model, records, stop);
    let under = under_loads(
        &people,
        &loads,
        report.stats_from,
        stop,
        Options::default().step,
    );
    let mut sums: std::collections::BTreeMap<&str, (f64, u32)> = Default::default();
    for u in &under {
        for name in [u.person.as_str(), u.crane.as_str()] {
            let s = sums.entry(name).or_default();
            s.0 += u.time;
            s.1 += u.entries;
        }
    }
    for (i, obj) in model.objects.iter().enumerate() {
        let (Some(&(time, entries)), Some(b)) =
            (sums.get(obj.name.as_str()), report.blocks.get_mut(i))
        else {
            continue;
        };
        b.custom.insert("tempo_sob_carga".into(), time);
        b.custom
            .insert("entradas_sob_carga".into(), f64::from(entries));
    }
}

/// Puts the exposure of one replication into its report, as custom metrics of each mobile
/// resource (`<recurso>.quase_conflitos`, and for people `<pessoa>.sem_epi`,
/// `<pessoa>.tempo_zona.<zona>` and `<pessoa>.entradas_zona.<zona>`), with the default
/// [`Options`] (decision S4: 1.5 m). Called by `Model::drive` for models with zones.
pub fn add_to_report(
    model: &Model,
    report: &mut utsim_core::RunReport,
    records: &[utsim_core::TraceRecord],
) {
    let tracks = tracks(model, records, report.end_time);
    let e = measure(
        model,
        &tracks,
        report.replication,
        report.stats_from,
        report.end_time,
        Options::default(),
    );
    for t in &tracks {
        let Some(b) = report.blocks.get_mut(t.object) else {
            continue;
        };
        let n = e
            .near_misses
            .iter()
            .filter(|m| m.person == t.name || m.vehicle == t.name)
            .count();
        b.custom.insert("quase_conflitos".into(), n as f64);
        if t.is_person() {
            let mine: Vec<&ZoneExposure> = e.zones.iter().filter(|z| z.person == t.name).collect();
            b.custom.insert(
                "sem_epi".into(),
                f64::from(mine.iter().map(|z| z.entries_without_ppe).sum::<u32>()),
            );
            for z in mine {
                b.custom.insert(format!("tempo_zona.{}", z.zone), z.time);
                b.custom
                    .insert(format!("entradas_zona.{}", z.zone), f64::from(z.entries));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: [[f64; 2]; 4] = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];

    #[test]
    fn segment_through_a_square() {
        // From x = -5 to x = 15 at y = 5: inside from 5 / 20 to 15 / 20.
        let s = segment_inside([-5.0, 5.0], [15.0, 5.0], &SQUARE);
        assert_eq!(s.len(), 1);
        assert!((s[0].0 - 0.25).abs() < 1e-12 && (s[0].1 - 0.75).abs() < 1e-12);
        assert!(segment_inside([-5.0, 20.0], [15.0, 20.0], &SQUARE).is_empty());
        assert_eq!(
            segment_inside([1.0, 1.0], [2.0, 2.0], &SQUARE),
            vec![(0.0, 1.0)]
        );
    }

    #[test]
    fn concave_polygon_gives_two_spans() {
        // A "U": the segment at y = 8 crosses both arms.
        let u = [
            [0.0, 0.0],
            [9.0, 0.0],
            [9.0, 10.0],
            [6.0, 10.0],
            [6.0, 3.0],
            [3.0, 3.0],
            [3.0, 10.0],
            [0.0, 10.0],
        ];
        let s = segment_inside([-1.0, 8.0], [10.0, 8.0], &u);
        assert_eq!(s.len(), 2);
        let len: f64 = s.iter().map(|(a, b)| (b - a) * 11.0).sum();
        assert!((len - 6.0).abs() < 1e-9);
    }

    #[test]
    fn time_inside_on_a_known_track() {
        // Walks x from -5 to 15 at y = 5 between t = 0 and t = 20 (1 m per time unit),
        // waits at x = 15 until t = 30, comes back to x = 5 by t = 40 and stays.
        let track = Track {
            object: 0,
            name: "p".into(),
            role: Role::Person,
            ppe: BTreeSet::new(),
            moves: vec![
                Move {
                    t0: 0.0,
                    t1: 20.0,
                    p0: [-5.0, 5.0],
                    p1: [15.0, 5.0],
                },
                Move {
                    t0: 20.0,
                    t1: 30.0,
                    p0: [15.0, 5.0],
                    p1: [15.0, 5.0],
                },
                Move {
                    t0: 30.0,
                    t1: 40.0,
                    p0: [15.0, 5.0],
                    p1: [5.0, 5.0],
                },
                Move {
                    t0: 40.0,
                    t1: 50.0,
                    p0: [5.0, 5.0],
                    p1: [5.0, 5.0],
                },
            ],
        };
        let spans = track.inside(&SQUARE, 0.0, 50.0);
        // Inside 5..15, then 35..50 (the last two moves merge).
        assert_eq!(spans.len(), 2);
        assert!((spans[0].0 - 5.0).abs() < 1e-9 && (spans[0].1 - 15.0).abs() < 1e-9);
        assert!((spans[1].0 - 35.0).abs() < 1e-9 && (spans[1].1 - 50.0).abs() < 1e-9);
        // A window cuts the time.
        let w: f64 = track
            .inside(&SQUARE, 10.0, 40.0)
            .iter()
            .map(|(a, b)| b - a)
            .sum();
        assert!((w - 10.0).abs() < 1e-9);
        assert_eq!(track.at(25.0), [15.0, 5.0]);
        assert_eq!(track.at(35.0), [10.0, 5.0]);
    }

    #[test]
    fn interval_of_constant_values_has_no_width() {
        let i = interval(&[2.0, 2.0, 2.0]);
        assert_eq!(i.mean, 2.0);
        assert_eq!(i.half_width, 0.0);
    }
}
