//! Inline SVG plots (no dependencies): histogram with fitted densities, Q-Q, P-P and
//! ECDF versus fitted CDF.
//!
//! Every function returns a self-contained `<svg>` string and never panics, even for
//! empty or constant samples. Axes and text use `currentColor`, so the plots follow the
//! surrounding theme when embedded in HTML.

use std::fmt::Write;

use crate::rank::CandidateResult;

const W: f64 = 560.0;
const H: f64 = 340.0;
const ML: f64 = 58.0;
const MR: f64 = 16.0;
const MT: f64 = 30.0;
const MB: f64 = 46.0;
/// Series colours, readable on light and dark backgrounds.
const COLORS: [&str; 4] = ["#2f7fd1", "#e08a1e", "#2aa876", "#c25ad1"];
const MAX_POINTS: usize = 400;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Compact number for axes (decimal comma, about 4 significant digits).
fn tick(v: f64) -> String {
    if !v.is_finite() {
        return "-".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let mag = v.abs().log10().floor() as i32;
    if !(-3..=6).contains(&mag) {
        return format!("{v:.2e}").replace('.', ",");
    }
    let decimals = (3 - mag).clamp(0, 8) as usize;
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    s.replace('.', ",")
}

fn nice_step(range: f64, target: f64) -> f64 {
    let raw = range / target;
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let nice = if f < 1.5 {
        1.0
    } else if f < 3.5 {
        2.0
    } else if f < 7.5 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn ticks(lo: f64, hi: f64) -> Vec<f64> {
    if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater)
        || !lo.is_finite()
        || !hi.is_finite()
    {
        return vec![lo];
    }
    let step = nice_step(hi - lo, 5.0);
    let mut v = (lo / step).ceil() * step;
    let mut out = Vec::new();
    while v <= hi + step * 1e-9 && out.len() < 20 {
        out.push(v);
        v += step;
    }
    out
}

/// Widens a degenerate range so scaling never divides by zero.
fn widen(lo: f64, hi: f64) -> (f64, f64) {
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    if hi > lo {
        (lo, hi)
    } else {
        let pad = if lo == 0.0 { 1.0 } else { lo.abs() * 0.5 };
        (lo - pad, hi + pad)
    }
}

struct Frame {
    svg: String,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

impl Frame {
    fn new(title: &str, xlabel: &str, ylabel: &str, xr: (f64, f64), yr: (f64, f64)) -> Frame {
        let (x0, x1) = widen(xr.0, xr.1);
        let (y0, y1) = widen(yr.0, yr.1);
        let mut f = Frame {
            svg: String::new(),
            x0,
            x1,
            y0,
            y1,
        };
        let _ = write!(
            f.svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {W} {H}\" width=\"100%\" \
             role=\"img\" aria-label=\"{t}\" font-family=\"system-ui, sans-serif\" font-size=\"11\" \
             style=\"max-width:{W}px\">",
            t = esc(title)
        );
        let _ = write!(
            f.svg,
            "<text x=\"{}\" y=\"18\" text-anchor=\"middle\" font-size=\"13\" font-weight=\"600\" \
             fill=\"currentColor\">{}</text>",
            W / 2.0,
            esc(title)
        );
        for t in ticks(x0, x1) {
            let px = f.px(t);
            let _ = write!(
                f.svg,
                "<line x1=\"{px:.1}\" y1=\"{MT}\" x2=\"{px:.1}\" y2=\"{}\" stroke=\"currentColor\" opacity=\"0.12\"/>\
                 <text x=\"{px:.1}\" y=\"{}\" text-anchor=\"middle\" fill=\"currentColor\">{}</text>",
                H - MB,
                H - MB + 14.0,
                tick(t)
            );
        }
        for t in ticks(y0, y1) {
            let py = f.py(t);
            let _ = write!(
                f.svg,
                "<line x1=\"{ML}\" y1=\"{py:.1}\" x2=\"{}\" y2=\"{py:.1}\" stroke=\"currentColor\" opacity=\"0.12\"/>\
                 <text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"currentColor\">{}</text>",
                W - MR,
                ML - 5.0,
                py + 3.5,
                tick(t)
            );
        }
        let _ = write!(
            f.svg,
            "<rect x=\"{ML}\" y=\"{MT}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"currentColor\" opacity=\"0.5\"/>\
             <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" fill=\"currentColor\">{}</text>\
             <text transform=\"translate(14 {}) rotate(-90)\" text-anchor=\"middle\" fill=\"currentColor\">{}</text>",
            W - MR - ML,
            H - MB - MT,
            (ML + W - MR) / 2.0,
            H - 6.0,
            esc(xlabel),
            (MT + H - MB) / 2.0,
            esc(ylabel)
        );
        f
    }

    fn px(&self, x: f64) -> f64 {
        ML + (x - self.x0) / (self.x1 - self.x0) * (W - MR - ML)
    }

    fn py(&self, y: f64) -> f64 {
        H - MB - (y - self.y0) / (self.y1 - self.y0) * (H - MB - MT)
    }

    fn clamp_pt(&self, x: f64, y: f64) -> (f64, f64) {
        (self.px(x).clamp(ML, W - MR), self.py(y).clamp(MT, H - MB))
    }

    fn polyline(&mut self, pts: &[(f64, f64)], color: &str, width: f64) {
        let mut d = String::new();
        for &(x, y) in pts.iter().filter(|(x, y)| x.is_finite() && y.is_finite()) {
            let (px, py) = self.clamp_pt(x, y);
            let _ = write!(d, "{px:.1},{py:.1} ");
        }
        if !d.is_empty() {
            let _ = write!(
                self.svg,
                "<polyline fill=\"none\" stroke=\"{color}\" stroke-width=\"{width}\" points=\"{}\"/>",
                d.trim_end()
            );
        }
    }

    fn points(&mut self, pts: &[(f64, f64)], color: &str) {
        for &(x, y) in pts.iter().filter(|(x, y)| x.is_finite() && y.is_finite()) {
            let (px, py) = self.clamp_pt(x, y);
            let _ = write!(
                self.svg,
                "<circle cx=\"{px:.1}\" cy=\"{py:.1}\" r=\"2.2\" fill=\"{color}\" fill-opacity=\"0.75\"/>"
            );
        }
    }

    fn legend(&mut self, entries: &[(String, &str)]) {
        for (i, (label, color)) in entries.iter().enumerate() {
            let y = MT + 14.0 + 15.0 * i as f64;
            let x = W - MR - 150.0;
            let _ = write!(
                self.svg,
                "<line x1=\"{x}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{color}\" stroke-width=\"2.5\"/>\
                 <text x=\"{}\" y=\"{}\" fill=\"currentColor\">{}</text>",
                y - 3.5,
                x + 18.0,
                y - 3.5,
                x + 23.0,
                y,
                esc(label)
            );
        }
    }

    fn note(&mut self, text: &str) {
        let _ = write!(
            self.svg,
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" fill=\"currentColor\" opacity=\"0.7\">{}</text>",
            (ML + W - MR) / 2.0,
            (MT + H - MB) / 2.0,
            esc(text)
        );
    }

    fn finish(mut self) -> String {
        self.svg.push_str("</svg>");
        self.svg
    }
}

fn clean_sorted(data: &[f64]) -> Vec<f64> {
    let mut v: Vec<f64> = data.iter().copied().filter(|x| x.is_finite()).collect();
    v.sort_by(f64::total_cmp);
    v
}

fn bounds(v: &[f64]) -> (f64, f64) {
    match (v.first(), v.last()) {
        (Some(&a), Some(&b)) => (a, b),
        _ => (0.0, 1.0),
    }
}

/// Indices of at most `MAX_POINTS` evenly spread elements of `0..n`.
fn spread(n: usize) -> Vec<usize> {
    if n <= MAX_POINTS {
        (0..n).collect()
    } else {
        (0..MAX_POINTS)
            .map(|k| k * (n - 1) / (MAX_POINTS - 1))
            .collect()
    }
}

fn empty_plot(title: &str) -> String {
    let mut f = Frame::new(title, "", "", (0.0, 1.0), (0.0, 1.0));
    f.note("sem dados para desenhar");
    f.finish()
}

/// Histogram (or pmf bars when `discrete`) with the fitted densities of `candidates`
/// overlaid. Candidates without a fitted model are ignored.
pub fn histogram(data: &[f64], candidates: &[&CandidateResult], discrete: bool) -> String {
    let title = "Histograma e densidades ajustadas";
    let v = clean_sorted(data);
    if v.is_empty() {
        return empty_plot(title);
    }
    let n = v.len() as f64;
    let (lo, hi) = widen(bounds(&v).0, bounds(&v).1);
    let models: Vec<_> = candidates
        .iter()
        .filter_map(|c| c.fitted.as_ref().map(|f| (c.family.clone(), &f.model)))
        .take(COLORS.len())
        .collect();

    // Bars: (left, right, height).
    let bars: Vec<(f64, f64, f64)> = if discrete {
        let mut out: Vec<(f64, f64, f64)> = Vec::new();
        for &x in &v {
            match out.last_mut() {
                Some(last) if (last.0 + last.1) / 2.0 == x => last.2 += 1.0 / n,
                _ => out.push((x - 0.35, x + 0.35, 1.0 / n)),
            }
        }
        out
    } else {
        let k = ((n.sqrt().ceil()) as usize).clamp(5, 40);
        let w = (hi - lo) / k as f64;
        let mut counts = vec![0usize; k];
        for &x in &v {
            let i = (((x - lo) / w) as usize).min(k - 1);
            counts[i] += 1;
        }
        counts
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                (
                    lo + i as f64 * w,
                    lo + (i + 1) as f64 * w,
                    c as f64 / (n * w),
                )
            })
            .collect()
    };
    let top = bars.iter().map(|b| b.2).fold(0.0, f64::max);
    let ymax = top * 1.15;
    let xpad = if discrete { 0.8 } else { 0.0 };
    let mut f = Frame::new(
        title,
        "valor",
        if discrete {
            "probabilidade"
        } else {
            "densidade"
        },
        (lo - xpad, hi + xpad),
        (0.0, ymax),
    );
    for (l, r, h) in &bars {
        let (x1, y1) = f.clamp_pt(*l, *h);
        let (x2, y2) = f.clamp_pt(*r, 0.0);
        let _ = write!(
            f.svg,
            "<rect x=\"{x1:.1}\" y=\"{y1:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"currentColor\" \
             fill-opacity=\"0.18\" stroke=\"currentColor\" stroke-opacity=\"0.45\"/>",
            (x2 - x1).max(1.0),
            (y2 - y1).max(0.0)
        );
    }
    let mut legend = Vec::new();
    for (i, (name, model)) in models.iter().enumerate() {
        let color = COLORS[i];
        if discrete {
            let pts: Vec<(f64, f64)> = bars
                .iter()
                .map(|b| {
                    let x = (b.0 + b.1) / 2.0;
                    (x, model.pdf(x))
                })
                .collect();
            f.polyline(&pts, color, 1.5);
            f.points(&pts, color);
        } else {
            let pts: Vec<(f64, f64)> = (0..=200)
                .map(|j| {
                    let x = lo + (hi - lo) * j as f64 / 200.0;
                    (x, model.pdf(x))
                })
                .collect();
            f.polyline(&pts, color, 2.0);
        }
        legend.push((name.clone(), color));
    }
    f.legend(&legend);
    f.finish()
}

/// Q-Q plot: sample quantiles against the fitted model's quantiles.
pub fn qq(data: &[f64], candidate: &CandidateResult) -> String {
    let title = format!("Q-Q: {}", candidate.family);
    let v = clean_sorted(data);
    let Some(fitted) = candidate.fitted.as_ref().filter(|_| !v.is_empty()) else {
        return empty_plot(&title);
    };
    let n = v.len() as f64;
    let pts: Vec<(f64, f64)> = spread(v.len())
        .into_iter()
        .map(|i| (fitted.model.quantile((i as f64 + 0.5) / n), v[i]))
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .collect();
    if pts.is_empty() {
        return empty_plot(&title);
    }
    let (lo, hi) = pts.iter().fold((f64::MAX, f64::MIN), |(l, h), &(a, b)| {
        (l.min(a).min(b), h.max(a).max(b))
    });
    let (lo, hi) = widen(lo, hi);
    let mut f = Frame::new(
        &title,
        "quantil teórico",
        "quantil da amostra",
        (lo, hi),
        (lo, hi),
    );
    f.polyline(&[(lo, lo), (hi, hi)], "currentColor", 1.0);
    f.points(&pts, COLORS[0]);
    f.finish()
}

/// P-P plot: empirical probability against the fitted CDF.
pub fn pp(data: &[f64], candidate: &CandidateResult) -> String {
    let title = format!("P-P: {}", candidate.family);
    let v = clean_sorted(data);
    let Some(fitted) = candidate.fitted.as_ref().filter(|_| !v.is_empty()) else {
        return empty_plot(&title);
    };
    let n = v.len() as f64;
    let pts: Vec<(f64, f64)> = spread(v.len())
        .into_iter()
        .map(|i| (fitted.model.cdf(v[i]), (i as f64 + 0.5) / n))
        .collect();
    let mut f = Frame::new(
        &title,
        "probabilidade teórica",
        "probabilidade empírica",
        (0.0, 1.0),
        (0.0, 1.0),
    );
    f.polyline(&[(0.0, 0.0), (1.0, 1.0)], "currentColor", 1.0);
    f.points(&pts, COLORS[1]);
    f.finish()
}

/// Empirical CDF (step line) against the fitted CDF.
pub fn ecdf(data: &[f64], candidate: &CandidateResult) -> String {
    let title = format!("ECDF × CDF: {}", candidate.family);
    let v = clean_sorted(data);
    let Some(fitted) = candidate.fitted.as_ref().filter(|_| !v.is_empty()) else {
        return empty_plot(&title);
    };
    let n = v.len() as f64;
    let (lo, hi) = widen(bounds(&v).0, bounds(&v).1);
    let mut step = vec![(lo, 0.0)];
    for i in spread(v.len()) {
        step.push((v[i], i as f64 / n));
        step.push((v[i], (i + 1) as f64 / n));
    }
    step.push((hi, 1.0));
    let curve: Vec<(f64, f64)> = (0..=200)
        .map(|j| {
            let x = lo + (hi - lo) * j as f64 / 200.0;
            (x, fitted.model.cdf(x))
        })
        .collect();
    let mut f = Frame::new(
        &title,
        "valor",
        "probabilidade acumulada",
        (lo, hi),
        (0.0, 1.0),
    );
    f.polyline(&step, COLORS[0], 1.5);
    f.polyline(&curve, COLORS[1], 2.0);
    f.legend(&[
        ("empírica".to_string(), COLORS[0]),
        (candidate.family.clone(), COLORS[1]),
    ]);
    f.finish()
}

/// Scatter of x[i] against x[i+1]: a cloud with structure hints at dependence.
pub fn lag_scatter(data: &[f64]) -> String {
    let title = "Dispersão x[i] × x[i+1]";
    let v: Vec<f64> = data.iter().copied().filter(|x| x.is_finite()).collect();
    if v.len() < 2 {
        return empty_plot(title);
    }
    let pts: Vec<(f64, f64)> = v.windows(2).map(|w| (w[0], w[1])).collect();
    let (lo, hi) = v
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), &x| (l.min(x), h.max(x)));
    let mut f = Frame::new(title, "x[i]", "x[i+1]", (lo, hi), (lo, hi));
    f.points(&pts, COLORS[2]);
    f.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::Exponential;
    use crate::family::Family;
    use crate::gof::GofConfig;

    fn sample() -> Vec<f64> {
        (1..=60)
            .map(|i| -(1.0 - i as f64 / 61.0).ln() * 2.0)
            .collect()
    }

    fn cand(data: &[f64]) -> CandidateResult {
        CandidateResult::evaluate(&Exponential, data, &GofConfig::default())
    }

    fn assert_svg(s: &str) {
        assert!(s.starts_with("<svg"), "{s}");
        assert!(s.ends_with("</svg>"));
        assert!(!s.contains("NaN") && !s.contains("inf"), "{s}");
    }

    #[test]
    fn all_plots_are_valid_svg() {
        let d = sample();
        let c = cand(&d);
        let h = histogram(&d, &[&c], false);
        assert_svg(&h);
        assert!(h.contains("<rect") && h.contains("<polyline") && h.contains("Exponencial"));
        let q = qq(&d, &c);
        assert_svg(&q);
        assert!(q.contains("<circle"));
        assert!(pp(&d, &c).contains("<circle"));
        let e = ecdf(&d, &c);
        assert_svg(&e);
        assert_eq!(e.matches("<polyline").count(), 2);
    }

    #[test]
    fn discrete_histogram_has_bars_and_points() {
        let d: Vec<f64> = (0..50).map(|i| (i % 5) as f64).collect();
        let c = cand(&d);
        let h = histogram(&d, &[&c], true);
        assert_svg(&h);
        assert!(h.contains("<circle"));
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        let c = cand(&[1.0, 2.0, 3.0]);
        let failed = CandidateResult::failed("Beta", 4, "não se aplica".into());
        for d in [vec![], vec![5.0], vec![3.0; 20], vec![f64::NAN, 1.0]] {
            assert_svg(&histogram(&d, &[&c, &failed], false));
            assert_svg(&histogram(&d, &[], true));
            for cand in [&c, &failed] {
                assert_svg(&qq(&d, cand));
                assert_svg(&pp(&d, cand));
                assert_svg(&ecdf(&d, cand));
            }
        }
        assert!(histogram(&[], &[], false).contains("sem dados"));
        assert_svg(&lag_scatter(&[]));
        assert_svg(&lag_scatter(&[2.0, 2.0, 2.0]));
        assert!(lag_scatter(&[1.0, 2.0, 4.0]).contains("<circle"));
    }

    #[test]
    fn labels_are_escaped() {
        let mut c = cand(&sample());
        c.family = "A<B&C".into();
        assert!(qq(&sample(), &c).contains("A&lt;B&amp;C"));
    }

    #[test]
    fn family_trait_is_used_for_fitting() {
        // Guards the import above: the plots rely on `Family::fit` through `evaluate`.
        assert!(Exponential.fit(&sample()).is_ok());
    }
}
