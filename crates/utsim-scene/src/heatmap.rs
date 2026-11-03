//! Heat map of movements on the floor: where the mobile resources and the items in
//! transit spend their time, and how often they pass, from the paths of the trace.
//!
//! Every `Travel` (a mobile resource moving along a path) and `Transit` (an item on a
//! timed direct connection, after its `delay`) is rasterised onto a grid of the floor
//! plan (`x`, `y`; the height is ignored), with cells of [`DEFAULT_CELL`] metres by
//! default. Movement along a leg is taken at constant speed, as the 3D scene draws it
//! ([`crate::timeline`]), so each cell gets the time spent crossing it, `length in the
//! cell / leg length · leg duration`, and one *passage* each time a leg enters it. A
//! leg cut short by the next `Travel` of the same resource ends there. The time can be
//! limited to a window (for example the statistics window, after the warm-up).
//!
//! Layers separate the movement **by resource** (one layer per transporter, plus one
//! for the transfers) or **by class** (person, forklift, AGV, vehicle and transfers,
//! with the role the 3D scene gives each resource). [`Heatmap::to_svg`] draws a layer
//! over the plan (objects and connections); the dashboard draws the same data itself.

use std::borrow::Cow;

use serde::Serialize;
use utsim_core::TraceEvent;
use utsim_model::Model;
use utsim_model::replay::Replay;

use crate::MobileRole;
use crate::compose::role_of;

/// Side of a cell when the user picks none, in metres.
pub const DEFAULT_CELL: f64 = 0.5;
/// Largest number of cells; a bigger plant gets coarser cells (see [`Heatmap::cell`]).
pub const MAX_CELLS: usize = 1_000_000;
/// Name of the layer of the items on timed direct connections.
pub const TRANSFERS: &str = "transferências";

/// How the movement is separated into layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum HeatGroup {
    /// One layer per mobile resource (and one for the transfers).
    #[default]
    Resource,
    /// One layer per class of resource: person, forklift, AGV, vehicle, transfers.
    Class,
}

impl HeatGroup {
    /// Parses `recurso` / `classe` (and the English names).
    pub fn parse(s: &str) -> Option<HeatGroup> {
        match s.trim().to_lowercase().as_str() {
            "recurso" | "recursos" | "resource" => Some(HeatGroup::Resource),
            "classe" | "classes" | "class" => Some(HeatGroup::Class),
            _ => None,
        }
    }
}

/// Which number of the cells is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum HeatMeasure {
    /// Time spent in the cell.
    #[default]
    Time,
    /// Number of passages through the cell.
    Passes,
}

/// Portuguese name of a resource class.
pub fn class_name(role: MobileRole) -> &'static str {
    match role {
        MobileRole::Person => "pessoas",
        MobileRole::Forklift => "empilhadeiras",
        MobileRole::Agv => "AGVs",
        MobileRole::Vehicle => "veículos",
    }
}

/// One layer of the map: per cell (row-major, `row * cols + col`), the time spent and
/// the passages.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HeatLayer {
    pub name: String,
    pub time: Vec<f64>,
    pub passes: Vec<u32>,
    /// Distance covered inside the window, in metres.
    pub distance: f64,
    /// Time moving inside the window.
    pub moving: f64,
}

impl HeatLayer {
    fn new(name: &str, cells: usize) -> HeatLayer {
        HeatLayer {
            name: name.to_string(),
            time: vec![0.0; cells],
            passes: vec![0; cells],
            distance: 0.0,
            moving: 0.0,
        }
    }

    fn add(&mut self, other: &HeatLayer) {
        for (a, b) in self.time.iter_mut().zip(&other.time) {
            *a += b;
        }
        for (a, b) in self.passes.iter_mut().zip(&other.passes) {
            *a += b;
        }
        self.distance += other.distance;
        self.moving += other.moving;
    }
}

/// One cell of a layer, for listings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct HeatCell {
    pub col: usize,
    pub row: usize,
    /// Centre of the cell in plan coordinates.
    pub x: f64,
    pub y: f64,
    pub time: f64,
    pub passes: u32,
}

/// The heat map of one replication.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Heatmap {
    /// Side of a cell in metres (the requested one, or coarser for a very large plant).
    pub cell: f64,
    /// Lower-left corner of the grid.
    pub x0: f64,
    pub y0: f64,
    pub cols: usize,
    pub rows: usize,
    /// Time window counted.
    pub from: f64,
    pub to: f64,
    pub group: HeatGroup,
    pub layers: Vec<HeatLayer>,
    /// Every layer summed.
    pub total: HeatLayer,
}

/// A movement: a path covered at constant speed from `t0` to `t1`.
struct Leg<'a> {
    layer: usize,
    t0: f64,
    t1: f64,
    path: Cow<'a, [[f64; 3]]>,
}

/// Length of a segment (the 3D length, as the scene moves along it).
fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
}

impl Heatmap {
    /// Builds the map of `replay` over `[from, to]` with cells of `cell` metres.
    /// `model` gives each resource its class (without it every resource is a vehicle).
    pub fn build(
        replay: &Replay,
        model: Option<&Model>,
        cell: f64,
        group: HeatGroup,
        from: f64,
        to: f64,
    ) -> Heatmap {
        let cell = if cell.is_finite() && cell > 0.0 {
            cell
        } else {
            DEFAULT_CELL
        };
        // Layers and the legs of each.
        let mut names: Vec<String> = Vec::new();
        let mut layer_of = |name: &str| -> usize {
            match names.iter().position(|n| n == name) {
                Some(i) => i,
                None => {
                    names.push(name.to_string());
                    names.len() - 1
                }
            }
        };
        let mut legs: Vec<Leg> = Vec::new();
        // Index of the last leg of each carrier, to cut it at the next one.
        let mut last_of: Vec<Option<usize>> = vec![None; replay.blocks.len()];
        for r in &replay.records {
            match &r.event {
                TraceEvent::Travel {
                    carrier,
                    path,
                    duration,
                } => {
                    if let Some(Some(prev)) = last_of.get(*carrier) {
                        let leg: &mut Leg = &mut legs[*prev];
                        if r.t < leg.t1 {
                            // Cut short: keep the part covered until now.
                            let f = (r.t - leg.t0) / (leg.t1 - leg.t0);
                            leg.path = Cow::Owned(cut(&leg.path, f));
                            leg.t1 = r.t;
                        }
                    }
                    let name = replay.blocks.get(*carrier).map_or("?", |b| b.name.as_str());
                    let layer = match group {
                        HeatGroup::Resource => layer_of(name),
                        HeatGroup::Class => layer_of(class_name(role_of(name, model))),
                    };
                    if let Some(slot) = last_of.get_mut(*carrier) {
                        *slot = Some(legs.len());
                    }
                    legs.push(Leg {
                        layer,
                        t0: r.t,
                        t1: r.t + duration.max(0.0),
                        path: Cow::Borrowed(path),
                    });
                }
                TraceEvent::Transit {
                    delay,
                    duration,
                    path,
                    ..
                } => {
                    let layer = layer_of(TRANSFERS);
                    let t0 = r.t + delay.max(0.0);
                    legs.push(Leg {
                        layer,
                        t0,
                        t1: t0 + duration.max(0.0),
                        path: Cow::Borrowed(path),
                    });
                }
                _ => {}
            }
        }
        // The grid covers the objects and every path, plus one cell around.
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        let points = replay
            .blocks
            .iter()
            .map(|b| b.position)
            .chain(legs.iter().flat_map(|l| l.path.iter().copied()));
        for p in points {
            for a in 0..2 {
                if p[a].is_finite() {
                    lo[a] = lo[a].min(p[a]);
                    hi[a] = hi[a].max(p[a]);
                }
            }
        }
        if !lo[0].is_finite() {
            lo = [0.0, 0.0];
            hi = [0.0, 0.0];
        }
        let mut cell = cell;
        let (mut cols, mut rows, mut x0, mut y0);
        loop {
            x0 = (lo[0] / cell).floor() * cell - cell;
            y0 = (lo[1] / cell).floor() * cell - cell;
            cols = ((hi[0] - x0) / cell).floor() as usize + 2;
            rows = ((hi[1] - y0) / cell).floor() as usize + 2;
            if cols * rows <= MAX_CELLS {
                break;
            }
            cell *= 2.0;
        }
        let n = cols * rows;
        let mut layers: Vec<HeatLayer> = names.iter().map(|s| HeatLayer::new(s, n)).collect();
        let index = |x: f64, y: f64| -> Option<usize> {
            let c = ((x - x0) / cell).floor();
            let r = ((y - y0) / cell).floor();
            (c >= 0.0 && r >= 0.0 && (c as usize) < cols && (r as usize) < rows)
                .then(|| r as usize * cols + c as usize)
        };
        for leg in &legs {
            rasterise(leg, from, to, cell, x0, y0, &index, &mut layers[leg.layer]);
        }
        let mut total = HeatLayer::new("todos", n);
        for l in &layers {
            total.add(l);
        }
        Heatmap {
            cell,
            x0,
            y0,
            cols,
            rows,
            from,
            to,
            group,
            layers,
            total,
        }
    }

    /// The layer called `name` (`None` or `"todos"`: the sum of all).
    pub fn layer(&self, name: Option<&str>) -> Option<&HeatLayer> {
        match name {
            None | Some("todos") => Some(&self.total),
            Some(n) => self.layers.iter().find(|l| l.name == n),
        }
    }

    fn cell_at(&self, layer: &HeatLayer, k: usize) -> HeatCell {
        let (row, col) = (k / self.cols, k % self.cols);
        HeatCell {
            col,
            row,
            x: self.x0 + (col as f64 + 0.5) * self.cell,
            y: self.y0 + (row as f64 + 0.5) * self.cell,
            time: layer.time[k],
            passes: layer.passes[k],
        }
    }

    /// The cells of `layer` with any time or passage.
    pub fn cells(&self, layer: &HeatLayer) -> Vec<HeatCell> {
        (0..layer.time.len())
            .filter(|&k| layer.time[k] > 0.0 || layer.passes[k] > 0)
            .map(|k| self.cell_at(layer, k))
            .collect()
    }

    /// The `n` cells of `layer` with the most time (or passages), largest first.
    pub fn hottest(&self, layer: &HeatLayer, measure: HeatMeasure, n: usize) -> Vec<HeatCell> {
        let mut cells = self.cells(layer);
        match measure {
            HeatMeasure::Time => cells.sort_by(|a, b| b.time.total_cmp(&a.time)),
            HeatMeasure::Passes => cells.sort_by_key(|c| std::cmp::Reverse(c.passes)),
        }
        cells.truncate(n);
        cells
    }

    /// A self-contained SVG of `layer` over the plan: the cells coloured from pale
    /// yellow to dark red by `measure` (relative to the largest cell), the objects as
    /// small squares with their names and the connections as thin lines, a scale bar
    /// and the legend.
    pub fn to_svg(&self, replay: &Replay, layer: Option<&str>, measure: HeatMeasure) -> String {
        let Some(l) = self.layer(layer) else {
            return String::new();
        };
        let (w_m, h_m) = (self.cols as f64 * self.cell, self.rows as f64 * self.cell);
        // Margin around the plan, for the names of the objects on its edges.
        const M: f64 = 36.0;
        let px = (900.0 - 2.0 * M) / w_m.max(1e-9);
        let (w, h) = (w_m * px, h_m * px);
        let sx = |x: f64| (x - self.x0) * px;
        // The plan's y grows away from the viewer: up on the page.
        let sy = |y: f64| h - (y - self.y0) * px;
        let value = |c: &HeatCell| match measure {
            HeatMeasure::Time => c.time,
            HeatMeasure::Passes => c.passes as f64,
        };
        let cells = self.cells(l);
        let max = cells.iter().map(value).fold(0.0, f64::max);
        let foot = 70.0;
        let mut s = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {:.1} {:.1}\" \
             width=\"{:.0}\" height=\"{:.0}\" font-family=\"system-ui, sans-serif\" font-size=\"11\">\n",
            w + 2.0 * M,
            h + 2.0 * M + foot,
            w + 2.0 * M,
            h + 2.0 * M + foot
        );
        let what = match measure {
            HeatMeasure::Time => "tempo de ocupação",
            HeatMeasure::Passes => "passagens",
        };
        s += &format!(
            "<title>Mapa de calor de deslocamentos ({}, {what})</title>\n",
            escape(&l.name)
        );
        s += &format!(
            "<rect width=\"{:.1}\" height=\"{:.1}\" fill=\"#ffffff\"/>\n<g transform=\"translate({M} {M})\">\n",
            w + 2.0 * M,
            h + 2.0 * M + foot
        );
        // Light grid every metre (or coarser) for reading distances.
        let step = nice_step(w_m.max(h_m) / 20.0);
        let mut g = (self.x0 / step).ceil() * step;
        while g <= self.x0 + w_m {
            s += &format!(
                "<line x1=\"{0:.1}\" x2=\"{0:.1}\" y1=\"0\" y2=\"{h:.1}\" stroke=\"#eef0f3\"/>\n",
                sx(g)
            );
            g += step;
        }
        let mut g = (self.y0 / step).ceil() * step;
        while g <= self.y0 + h_m {
            s += &format!(
                "<line x1=\"0\" x2=\"{w:.1}\" y1=\"{0:.1}\" y2=\"{0:.1}\" stroke=\"#eef0f3\"/>\n",
                sy(g)
            );
            g += step;
        }
        let side = self.cell * px;
        for c in &cells {
            let v = value(c);
            if v <= 0.0 || max <= 0.0 {
                continue;
            }
            s += &format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{side:.2}\" height=\"{side:.2}\" fill=\"{}\">\
                 <title>({:.2}; {:.2}) m: {} de ocupação, {} passagens</title></rect>\n",
                sx(c.x - self.cell / 2.0),
                sy(c.y + self.cell / 2.0),
                heat_color(v / max),
                c.x,
                c.y,
                num(c.time),
                c.passes
            );
        }
        for &(a, b) in &replay.connections {
            let (Some(pa), Some(pb)) = (replay.blocks.get(a), replay.blocks.get(b)) else {
                continue;
            };
            s += &format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#5d6672\" stroke-width=\"1\" stroke-dasharray=\"3 3\"/>\n",
                sx(pa.position[0]),
                sy(pa.position[1]),
                sx(pb.position[0]),
                sy(pb.position[1])
            );
        }
        for b in &replay.blocks {
            let (x, y) = (sx(b.position[0]), sy(b.position[1]));
            // Names near the right edge end at the object instead of starting there.
            let (tx, anchor) = if x > 0.75 * w {
                (x - 6.0, "end")
            } else {
                (x + 6.0, "start")
            };
            s += &format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"8\" height=\"8\" fill=\"#2563eb\" stroke=\"#1c1f24\"><title>{} ({})</title></rect>\n\
                 <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\" fill=\"#1c1f24\">{}</text>\n",
                x - 4.0,
                y - 4.0,
                escape(&b.name),
                escape(&b.kind),
                tx,
                y - 6.0,
                escape(&b.name)
            );
        }
        // Legend and scale, below the plan.
        s += "</g>\n";
        let ly = h + 2.0 * M + 4.0;
        for i in 0..5 {
            s += &format!(
                "<rect x=\"{}\" y=\"{ly:.1}\" width=\"28\" height=\"12\" fill=\"{}\"/>\n",
                10 + 28 * i,
                heat_color((i as f64 + 0.5) / 5.0)
            );
        }
        let top = match measure {
            HeatMeasure::Time => num(max),
            HeatMeasure::Passes => format!("{max:.0}"),
        };
        s += &format!(
            "<text x=\"160\" y=\"{:.1}\" fill=\"#1c1f24\">{} por célula de {} m: de 0 a {top}. Camada: {}. Janela {} a {}.</text>\n",
            ly + 10.0,
            capitalise(what),
            num(self.cell),
            escape(&l.name),
            num(self.from),
            num(self.to)
        );
        let bar = nice_step(w_m / 5.0);
        s += &format!(
            "<line x1=\"10\" x2=\"{:.1}\" y1=\"{:.1}\" y2=\"{:.1}\" stroke=\"#1c1f24\" stroke-width=\"2\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" fill=\"#1c1f24\">{} m</text>\n",
            10.0 + bar * px,
            ly + 34.0,
            ly + 34.0,
            14.0 + bar * px,
            ly + 38.0,
            num(bar)
        );
        s += "</svg>\n";
        s
    }
}

/// The first `fraction` (0 to 1) of a path, by arc length, ending at the
/// interpolated point.
fn cut(path: &[[f64; 3]], fraction: f64) -> Vec<[f64; 3]> {
    if path.len() < 2 || fraction >= 1.0 {
        return path.to_vec();
    }
    let total: f64 = path.windows(2).map(|w| dist(w[0], w[1])).sum();
    let target = fraction.max(0.0) * total;
    let mut out = vec![path[0]];
    let mut acc = 0.0;
    for w in path.windows(2) {
        let len = dist(w[0], w[1]);
        if acc + len >= target {
            let u = if len > 0.0 { (target - acc) / len } else { 0.0 };
            out.push([0, 1, 2].map(|k| w[0][k] + (w[1][k] - w[0][k]) * u));
            return out;
        }
        acc += len;
        out.push(w[1]);
    }
    out
}

/// Adds one leg to its layer: for every piece of a segment inside one cell, the time
/// spent there (constant speed over the leg) and a passage when the cell changes.
#[allow(clippy::too_many_arguments)]
fn rasterise(
    leg: &Leg,
    from: f64,
    to: f64,
    cell: f64,
    x0: f64,
    y0: f64,
    index: &dyn Fn(f64, f64) -> Option<usize>,
    layer: &mut HeatLayer,
) {
    let path: &[[f64; 3]] = &leg.path;
    if path.len() < 2 || leg.t1 <= leg.t0 {
        return;
    }
    let total: f64 = path.windows(2).map(|w| dist(w[0], w[1])).sum();
    if total <= 0.0 {
        return;
    }
    let speed_t = (leg.t1 - leg.t0) / total; // time per metre
    let mut covered = 0.0;
    let mut last: Option<usize> = None;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = dist(a, b);
        if len <= 0.0 {
            continue;
        }
        // Parameters where the segment crosses a grid line, plus its ends.
        let mut us = vec![0.0, 1.0];
        for axis in 0..2 {
            let (p, q, o) = (a[axis], b[axis], if axis == 0 { x0 } else { y0 });
            if (q - p).abs() < 1e-15 {
                continue;
            }
            let (lo, hi) = (p.min(q), p.max(q));
            let mut g = ((lo - o) / cell).ceil() * cell + o;
            while g < hi {
                us.push((g - p) / (q - p));
                g += cell;
            }
        }
        us.sort_by(f64::total_cmp);
        for pair in us.windows(2) {
            let (u0, u1) = (pair[0], pair[1]);
            if u1 - u0 <= 1e-12 {
                continue;
            }
            let um = 0.5 * (u0 + u1);
            let Some(k) = index(a[0] + (b[0] - a[0]) * um, a[1] + (b[1] - a[1]) * um) else {
                continue;
            };
            // Time interval of this piece, clipped to the window.
            let ta = leg.t0 + (covered + u0 * len) * speed_t;
            let tb = leg.t0 + (covered + u1 * len) * speed_t;
            let (ca, cb) = (ta.max(from), tb.min(to));
            if cb <= ca {
                continue;
            }
            layer.time[k] += cb - ca;
            layer.moving += cb - ca;
            layer.distance += (cb - ca) / speed_t;
            if last != Some(k) {
                layer.passes[k] += 1;
                last = Some(k);
            }
        }
        covered += len;
    }
}

/// Sequential colour from pale yellow (0) to dark red (1).
pub fn heat_color(v: f64) -> String {
    const STOPS: [[f64; 3]; 5] = [
        [255.0, 245.0, 177.0],
        [254.0, 204.0, 92.0],
        [253.0, 141.0, 60.0],
        [227.0, 26.0, 28.0],
        [128.0, 0.0, 38.0],
    ];
    let v = v.clamp(0.0, 1.0) * (STOPS.len() - 1) as f64;
    let i = (v.floor() as usize).min(STOPS.len() - 2);
    let f = v - i as f64;
    let c: Vec<u8> = (0..3)
        .map(|k| (STOPS[i][k] + (STOPS[i + 1][k] - STOPS[i][k]) * f).round() as u8)
        .collect();
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// 1, 2 or 5 times a power of ten, at least `x`.
fn nice_step(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(x.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= x)
        .unwrap_or(10.0 * mag)
}

/// Number with up to two decimals and a decimal comma.
fn num(x: f64) -> String {
    let s = format!("{x:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_core::{RunReport, TraceRecord};
    use utsim_model::replay::ReplayBlock;

    fn replay(records: Vec<TraceRecord>) -> Replay {
        let block = |name: &str, kind: &str, x: f64, y: f64| ReplayBlock {
            name: name.into(),
            kind: kind.into(),
            position: [x, y, 0.0],
        };
        Replay {
            model: "m".into(),
            replication: 0,
            stop_time: 100.0,
            blocks: vec![
                block("a", "Queue", 0.0, 0.0),
                block("b", "Sink", 4.0, 0.0),
                block("agv", "Transporter", 0.0, 0.0),
                block("op", "Transporter", 0.0, 2.0),
            ],
            connections: vec![(0, 1)],
            records,
            report: RunReport {
                replication: 0,
                seed: 0,
                stats_from: 0.0,
                end_time: 100.0,
                events: 0,
                blocks: vec![],
            },
            fluids: None,
        }
    }

    fn travel(t: f64, carrier: usize, path: Vec<[f64; 3]>, duration: f64) -> TraceRecord {
        TraceRecord {
            t,
            event: TraceEvent::Travel {
                carrier,
                path,
                duration,
            },
        }
    }

    #[test]
    fn a_straight_leg_spreads_its_time_over_the_cells() {
        // 2 m along x in 4 time units, inside the row y in [0.0, 0.5): four cells of
        // 0.5 m, one time unit and one passage each.
        let r = replay(vec![travel(
            10.0,
            2,
            vec![[0.0, 0.25, 0.0], [2.0, 0.25, 0.0]],
            4.0,
        )]);
        let m = Heatmap::build(&r, None, 0.5, HeatGroup::Resource, 0.0, 100.0);
        assert_eq!(m.cell, 0.5);
        let l = m.layer(Some("agv")).unwrap();
        let cells = m.cells(l);
        assert_eq!(cells.len(), 4, "{cells:?}");
        for c in &cells {
            assert!((c.time - 1.0).abs() < 1e-12 && c.passes == 1, "{c:?}");
            assert!((c.y - 0.25).abs() < 1e-12);
        }
        assert!((l.distance - 2.0).abs() < 1e-12 && (l.moving - 4.0).abs() < 1e-12);
        // The window keeps only its part of the time.
        let w = Heatmap::build(&r, None, 0.5, HeatGroup::Resource, 11.0, 12.0);
        let l = w.layer(None).unwrap();
        assert!((l.moving - 1.0).abs() < 1e-12);
        assert_eq!(w.cells(l).len(), 1);
        assert!((w.cells(l)[0].x - 0.75).abs() < 1e-12);
    }

    #[test]
    fn diagonal_legs_and_returns_count_passages() {
        // Out and back over the same cells: two passages each, double the time.
        let r = replay(vec![
            travel(0.0, 2, vec![[0.1, 0.1, 0.0], [0.9, 0.9, 0.0]], 2.0),
            travel(5.0, 2, vec![[0.9, 0.9, 0.0], [0.1, 0.1, 0.0]], 2.0),
        ]);
        let m = Heatmap::build(&r, None, 0.5, HeatGroup::Resource, 0.0, 100.0);
        let l = m.layer(None).unwrap();
        let cells = m.cells(l);
        // The diagonal crosses the corner (0.5, 0.5): two cells, half the time each.
        assert_eq!(cells.len(), 2, "{cells:?}");
        for c in &cells {
            assert_eq!(c.passes, 2);
            assert!((c.time - 2.0).abs() < 1e-9, "{c:?}");
        }
        let total: f64 = l.time.iter().sum();
        assert!((total - 4.0).abs() < 1e-9);
        assert!((l.distance - 2.0 * 0.8 * 2f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn layers_by_resource_and_by_class() {
        let r = replay(vec![
            travel(0.0, 2, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], 1.0),
            travel(0.0, 3, vec![[0.0, 2.0, 0.0], [1.0, 2.0, 0.0]], 2.0),
            TraceRecord {
                t: 1.0,
                event: TraceEvent::Transit {
                    item: 7,
                    from: 0,
                    to: 1,
                    delay: 1.0,
                    duration: 4.0,
                    path: vec![[0.0, 0.0, 0.0], [4.0, 0.0, 0.0]],
                },
            },
        ]);
        let m = Heatmap::build(&r, None, 0.5, HeatGroup::Resource, 0.0, 100.0);
        let names: Vec<&str> = m.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["agv", "op", TRANSFERS]);
        let t = m.layer(Some(TRANSFERS)).unwrap();
        // The delay is spent standing still at the origin: only the 4 moving count.
        assert!((t.moving - 4.0).abs() < 1e-12 && (t.distance - 4.0).abs() < 1e-12);
        assert!((m.total.moving - 7.0).abs() < 1e-12);
        // Without the model every resource is a vehicle.
        let c = Heatmap::build(&r, None, 0.5, HeatGroup::Class, 0.0, 100.0);
        let names: Vec<&str> = c.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["veículos", TRANSFERS]);
        assert!((c.layers[0].moving - 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_leg_cut_by_the_next_one_ends_there() {
        let r = replay(vec![
            travel(0.0, 2, vec![[0.0, 0.25, 0.0], [10.0, 0.25, 0.0]], 10.0),
            travel(2.0, 2, vec![[2.0, 0.25, 0.0], [2.0, 3.0, 0.0]], 1.0),
        ]);
        let m = Heatmap::build(&r, None, 0.5, HeatGroup::Resource, 0.0, 100.0);
        let l = m.layer(None).unwrap();
        assert!((l.moving - 3.0).abs() < 1e-9, "{}", l.moving);
        // Nothing beyond x = 2.5 on the first leg's row.
        assert!(m.cells(l).iter().all(|c| c.x < 2.5), "{:?}", m.cells(l));
    }

    #[test]
    fn svg_draws_the_plan_and_the_hottest_cells() {
        let r = replay(vec![travel(
            0.0,
            2,
            vec![[0.0, 0.25, 0.0], [2.0, 0.25, 0.0]],
            4.0,
        )]);
        let m = Heatmap::build(&r, None, 0.5, HeatGroup::Resource, 0.0, 100.0);
        let svg = m.to_svg(&r, None, HeatMeasure::Time);
        assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains(">agv</text>") && svg.contains("stroke-dasharray"));
        assert_eq!(svg.matches("passagens</title></rect>").count(), 4);
        assert!(m.to_svg(&r, Some("nada"), HeatMeasure::Time).is_empty());
        let top = m.hottest(&m.total, HeatMeasure::Passes, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(heat_color(0.0), "#fff5b1");
        assert_eq!(heat_color(1.0), "#800026");
        assert_eq!(HeatGroup::parse("Classe"), Some(HeatGroup::Class));
    }

    #[test]
    fn a_real_model_with_transporters() {
        let model = Model::parse(include_str!("../../../examples/agvs.ron")).unwrap();
        let replay = model.replay(0).unwrap();
        let m = Heatmap::build(
            &replay,
            Some(&model),
            DEFAULT_CELL,
            HeatGroup::Class,
            0.0,
            replay.report.end_time,
        );
        assert!(m.total.moving > 0.0, "the AGVs move");
        assert!(
            m.layers.iter().any(|l| l.name == "AGVs"),
            "{:?}",
            m.layers.iter().map(|l| &l.name).collect::<Vec<_>>()
        );
        let sum: f64 = m.total.time.iter().sum();
        assert!((sum - m.total.moving).abs() < 1e-6 * sum.max(1.0));
    }
}
