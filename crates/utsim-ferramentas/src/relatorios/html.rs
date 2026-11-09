//! Self-contained HTML of a [`Document`]: one page with inline CSS, charts as inline SVG
//! and the app's icon in base64. No network, no script needed, and `@media print` gives the
//! cover its own page and breaks the pages at the [`Block::PageBreak`]s. It shows the same
//! content as the PDF.

use std::fmt::Write as _;

use super::pdf::{PALETTE, REFERENCE, SIGNAL, axis, dark, light, range, tone_colours};
use super::{Block, Chart, ChartKind, Cover, Document, Kpi, Template, Tone};

type Rgb = (f64, f64, f64);

const W: f64 = 511.0;
const PLOT_H: f64 = 150.0;
const GREY: Rgb = (0.35, 0.35, 0.35);
const BLACK: Rgb = (0.0, 0.0, 0.0);

/// Escapes the characters that matter in HTML text and attributes.
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

/// `#rrggbb` of RGB fractions.
fn hex((r, g, b): Rgb) -> String {
    let q = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(r), q(g), q(b))
}

fn n(x: f64) -> String {
    let s = format!("{x:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// The first characters of a text that fit about `max` units at `size`.
fn fit(s: &str, max: f64, size: f64) -> String {
    let cap = ((max / (size * 0.5)).floor() as usize).max(2);
    if s.chars().count() <= cap {
        return s.to_string();
    }
    let mut o: String = s.chars().take(cap - 1).collect();
    o.push('…');
    o
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= c.len() {
                o.push(T[((v >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                o.push('=');
            }
        }
    }
    o
}

/// An SVG under construction (y grows downwards). Inline in HTML it needs no namespace.
struct Svg(String);

impl Svg {
    fn new(h: f64) -> Svg {
        Svg(format!(
            "<svg class=\"fig\" viewBox=\"0 0 {W} {}\" role=\"img\">",
            n(h)
        ))
    }

    fn line(&mut self, a: (f64, f64), b: (f64, f64), w: f64, c: Rgb) {
        let _ = write!(
            self.0,
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
            n(a.0),
            n(a.1),
            n(b.0),
            n(b.1),
            hex(c),
            n(w)
        );
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, c: Rgb) {
        let _ = write!(
            self.0,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            n(x),
            n(y),
            n(w.max(0.0)),
            n(h.max(0.0)),
            hex(c)
        );
    }

    fn frame(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let _ = write!(
            self.0,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#999\" stroke-width=\"0.4\"/>",
            n(x),
            n(y),
            n(w),
            n(h)
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, x: f64, y: f64, size: f64, bold: bool, c: Rgb, anchor: &str, s: &str) {
        let _ = write!(
            self.0,
            "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{}\" text-anchor=\"{anchor}\"{}>{}</text>",
            n(x),
            n(y),
            n(size),
            hex(c),
            if bold { " font-weight=\"bold\"" } else { "" },
            esc(s)
        );
    }

    fn finish(mut self) -> String {
        self.0.push_str("</svg>");
        self.0
    }
}

/// The plot area of a chart (y grows downwards on the page).
struct Area {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
    lo: f64,
    hi: f64,
}

impl Area {
    fn new(right_margin: f64, lo: f64, hi: f64) -> Area {
        Area {
            left: 44.0,
            right: W - right_margin,
            top: 8.0,
            bottom: 8.0 + PLOT_H,
            lo,
            hi,
        }
    }

    fn y(&self, v: f64) -> f64 {
        self.bottom - (v - self.lo) / (self.hi - self.lo) * (self.bottom - self.top)
    }

    fn x(&self, v: f64, x0: f64, x1: f64) -> f64 {
        self.left + (v - x0) / (x1 - x0) * (self.right - self.left)
    }

    /// Frame, grid and the y labels.
    fn draw(&self, s: &mut Svg) {
        s.frame(self.left, self.top, self.right - self.left, PLOT_H);
        for i in 0..=4 {
            let v = self.lo + (self.hi - self.lo) * f64::from(i) / 4.0;
            let y = self.y(v);
            if i > 0 && i < 4 {
                s.line((self.left, y), (self.right, y), 0.2, (0.85, 0.85, 0.85));
            }
            s.text(self.left - 4.0, y + 2.5, 7.0, false, GREY, "end", &axis(v));
        }
    }

    fn x_ticks(&self, s: &mut Svg, x0: f64, x1: f64) {
        for i in 0..=4 {
            let v = x0 + (x1 - x0) * f64::from(i) / 4.0;
            let x = self.left + (self.right - self.left) * f64::from(i) / 4.0;
            s.text(x, self.bottom + 9.0, 6.5, false, GREY, "middle", &axis(v));
        }
    }
}

fn legend(entries: &[(String, Rgb)]) -> String {
    let mut o = String::from("<div class=\"legend\">");
    for (s, c) in entries {
        let _ = write!(
            o,
            "<span><i style=\"background:{}\"></i>{}</span>",
            hex(*c),
            esc(s)
        );
    }
    o.push_str("</div>");
    o
}

fn figure(title: &str, svg: String, leg: String) -> String {
    let t = if title.is_empty() {
        String::new()
    } else {
        format!("<h4>{}</h4>", esc(title))
    };
    format!("<figure class=\"chart\">{t}{svg}{leg}</figure>\n")
}

fn chart(c: &Chart, accent: Rgb) -> String {
    match c.drawn_kind() {
        ChartKind::Pareto => return pareto(c, accent),
        ChartKind::Histogram => return histogram(c, accent),
        ChartKind::Scatter => return scatter(c, accent),
        _ => {}
    }
    let bars = c.drawn_kind() == ChartKind::Bars;
    let cnt = c
        .series
        .iter()
        .map(|(_, v)| v.len())
        .max()
        .unwrap_or(0)
        .max(c.labels.len());
    let mut vals: Vec<f64> = c
        .series
        .iter()
        .flat_map(|(_, v)| v.iter().copied())
        .chain(c.hlines.iter().map(|(_, v)| *v))
        .chain(c.bands.iter().flat_map(|(_, a, b)| [*a, *b]))
        .filter(|v| v.is_finite())
        .collect();
    if bars {
        vals.push(0.0);
    }
    let (lo, hi) = range(&vals, bars);
    let a = Area::new(4.0, lo, hi);
    let mut s = Svg::new(a.bottom + 16.0);
    a.draw(&mut s);
    let w = a.right - a.left;
    for (bi, (_, x, y)) in c.bands.iter().enumerate() {
        if x.is_finite() && y.is_finite() {
            let (y0, y1) = (a.y(x.max(*y)), a.y(x.min(*y)));
            s.rect(
                a.left,
                y0,
                w,
                y1 - y0,
                light(PALETTE[(bi + 2) % PALETTE.len()], 0.16),
            );
        }
    }
    if cnt > 0 {
        let slot = w / cnt as f64;
        let ns = c.series.len().max(1);
        for (si, (_, v)) in c.series.iter().enumerate() {
            let rgb = PALETTE[si % PALETTE.len()];
            if bars {
                let group = slot * 0.75;
                let bw = group / ns as f64;
                for (i, y) in v.iter().enumerate() {
                    if !y.is_finite() {
                        continue;
                    }
                    let x = a.left + i as f64 * slot + (slot - group) / 2.0 + si as f64 * bw;
                    let (p, q) = (a.y(0.0f64.max(lo)), a.y(*y));
                    s.rect(x, p.min(q), bw * 0.92, (q - p).abs(), rgb);
                }
            } else {
                let pts: Vec<Option<(f64, f64)>> = v
                    .iter()
                    .enumerate()
                    .map(|(i, y)| {
                        y.is_finite()
                            .then(|| (a.left + (i as f64 + 0.5) * slot, a.y(*y)))
                    })
                    .collect();
                for pair in pts.windows(2) {
                    if let (Some(p), Some(q)) = (pair[0], pair[1]) {
                        s.line(p, q, 0.9, rgb);
                    }
                }
                if cnt <= 60 {
                    for (x, y) in pts.iter().flatten() {
                        s.rect(x - 1.3, y - 1.3, 2.6, 2.6, rgb);
                    }
                }
            }
        }
        if let Some((_, v)) = c.series.first() {
            for i in &c.marks {
                if let Some(y) = v.get(*i).filter(|y| y.is_finite()) {
                    let x = a.left + (*i as f64 + 0.5) * slot;
                    s.rect(x - 2.4, a.y(*y) - 2.4, 4.8, 4.8, SIGNAL);
                }
            }
        }
        let every = (cnt as f64 * 22.0 / w).ceil().max(1.0) as usize;
        for (i, l) in c.labels.iter().enumerate() {
            if i % every == 0 {
                let x = a.left + (i as f64 + 0.5) * slot;
                s.text(
                    x,
                    a.bottom + 10.0,
                    6.5,
                    false,
                    GREY,
                    "middle",
                    &fit(l, slot * every as f64 - 2.0, 6.5),
                );
            }
        }
    }
    for (_, v) in &c.hlines {
        if v.is_finite() {
            s.line((a.left, a.y(*v)), (a.right, a.y(*v)), 0.7, REFERENCE);
        }
    }
    let named = c.series.iter().filter(|(s, _)| !s.is_empty()).count();
    let total = named + c.hlines.len() + c.bands.len();
    let leg = if total > 1 || !c.hlines.is_empty() || !c.bands.is_empty() {
        let entries: Vec<(String, Rgb)> =
            c.series
                .iter()
                .enumerate()
                .filter(|(_, (s, _))| !s.is_empty())
                .map(|(i, (s, _))| (s.clone(), PALETTE[i % PALETTE.len()]))
                .chain(
                    c.hlines
                        .iter()
                        .map(|(s, v)| (format!("{s} = {}", axis(*v)), REFERENCE)),
                )
                .chain(c.bands.iter().enumerate().map(|(bi, (s, _, _))| {
                    (s.clone(), light(PALETTE[(bi + 2) % PALETTE.len()], 0.35))
                }))
                .collect();
        legend(&entries)
    } else {
        String::new()
    };
    figure(&c.title, s.finish(), leg)
}

fn pareto(c: &Chart, accent: Rgb) -> String {
    let v: Vec<f64> = c
        .series
        .first()
        .map(|s| s.1.iter().map(|x| x.max(0.0)).collect())
        .unwrap_or_default();
    let total: f64 = v.iter().filter(|x| x.is_finite()).sum();
    let total = if total > 0.0 { total } else { 1.0 };
    let a = Area::new(30.0, 0.0, total);
    let mut s = Svg::new(a.bottom + 16.0);
    a.draw(&mut s);
    let slot = (a.right - a.left) / v.len().max(1) as f64;
    let mut cum = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    for (i, x) in v.iter().enumerate() {
        let x0 = a.left + i as f64 * slot;
        s.rect(
            x0 + slot * 0.12,
            a.y(*x),
            slot * 0.76,
            a.bottom - a.y(*x),
            accent,
        );
        cum += x;
        let p = (x0 + slot / 2.0, a.y(cum));
        if let Some(q) = prev {
            s.line(q, p, 1.0, PALETTE[1]);
        }
        s.rect(p.0 - 1.6, p.1 - 1.6, 3.2, 3.2, PALETTE[1]);
        prev = Some(p);
        let l = c.labels.get(i).map_or("", String::as_str);
        s.text(
            x0 + slot / 2.0,
            a.bottom + 10.0,
            6.5,
            false,
            GREY,
            "middle",
            &fit(l, slot - 2.0, 6.5),
        );
    }
    let y80 = a.y(0.8 * total);
    s.line((a.left, y80), (a.right, y80), 0.7, REFERENCE);
    for pct in [0, 50, 80, 100] {
        let y = a.y(total * f64::from(pct) / 100.0);
        s.text(
            a.right + 3.0,
            y + 2.5,
            7.0,
            false,
            GREY,
            "start",
            &format!("{pct} %"),
        );
    }
    let name = c.series.first().map_or(String::new(), |s| s.0.clone());
    let leg = legend(&[
        (
            if name.is_empty() {
                "valor".into()
            } else {
                name
            },
            accent,
        ),
        ("acumulado (%)".into(), PALETTE[1]),
        ("80 %".into(), REFERENCE),
    ]);
    figure(&c.title, s.finish(), leg)
}

fn histogram(c: &Chart, accent: Rgb) -> String {
    let data: Vec<f64> = c
        .series
        .first()
        .map(|s| s.1.iter().copied().filter(|x| x.is_finite()).collect())
        .unwrap_or_default();
    let mut xs = data.clone();
    xs.extend(c.hlines.iter().map(|h| h.1).filter(|x| x.is_finite()));
    let (x0, x1) = range(&xs, false);
    // Sturges' rule, as in the PDF.
    let k = ((data.len().max(1) as f64).log2().ceil() as usize + 1).clamp(3, 30);
    let bw = (x1 - x0) / k as f64;
    let mut counts = vec![0usize; k];
    for x in &data {
        let i = (((x - x0) / bw).floor() as usize).min(k - 1);
        counts[i] += 1;
    }
    let top = counts.iter().copied().max().unwrap_or(1).max(1) as f64;
    let a = Area::new(30.0, 0.0, top * 1.08);
    let mut s = Svg::new(a.bottom + 16.0);
    a.draw(&mut s);
    for (i, cnt) in counts.iter().enumerate() {
        let l = a.x(x0 + i as f64 * bw, x0, x1);
        let r = a.x(x0 + (i as f64 + 1.0) * bw, x0, x1);
        if *cnt > 0 {
            s.rect(
                l + 0.5,
                a.y(*cnt as f64),
                r - l - 1.0,
                a.bottom - a.y(*cnt as f64),
                accent,
            );
        }
    }
    a.x_ticks(&mut s, x0, x1);
    let mut entries = vec![(
        c.series
            .first()
            .map_or(String::new(), |s| format!("{} (n = {})", s.0, data.len())),
        accent,
    )];
    for (name, v) in &c.hlines {
        if v.is_finite() {
            let x = a.x(*v, x0, x1);
            s.line((x, a.bottom), (x, a.top), 1.0, REFERENCE);
            s.text(x + 2.0, a.top + 9.0, 7.0, true, REFERENCE, "start", name);
            entries.push((format!("{name} = {}", axis(*v)), REFERENCE));
        }
    }
    figure(&c.title, s.finish(), legend(&entries))
}

fn scatter(c: &Chart, accent: Rgb) -> String {
    let (Some(xs), Some(ys)) = (c.series.first(), c.series.get(1)) else {
        return String::new();
    };
    let pts: Vec<(f64, f64)> =
        xs.1.iter()
            .zip(&ys.1)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .map(|(x, y)| (*x, *y))
            .collect();
    let (x0, x1) = range(&pts.iter().map(|p| p.0).collect::<Vec<_>>(), false);
    let (lo, hi) = range(&pts.iter().map(|p| p.1).collect::<Vec<_>>(), false);
    let a = Area::new(30.0, lo, hi);
    let mut s = Svg::new(a.bottom + 16.0);
    a.draw(&mut s);
    for (x, y) in &pts {
        s.rect(a.x(*x, x0, x1) - 1.8, a.y(*y) - 1.8, 3.6, 3.6, accent);
    }
    a.x_ticks(&mut s, x0, x1);
    let cnt = pts.len() as f64;
    let mut entries = vec![(format!("{} × {}", ys.0, xs.0), accent)];
    if pts.len() >= 2 {
        let mx = pts.iter().map(|p| p.0).sum::<f64>() / cnt;
        let my = pts.iter().map(|p| p.1).sum::<f64>() / cnt;
        let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
        if sxx > 0.0 {
            let b = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx;
            let b0 = my - b * mx;
            let cl = |y: f64| y.clamp(lo, hi);
            s.line(
                (a.x(x0, x0, x1), a.y(cl(b0 + b * x0))),
                (a.x(x1, x0, x1), a.y(cl(b0 + b * x1))),
                1.0,
                PALETTE[1],
            );
            entries.push((format!("y = {} + {}·x", axis(b0), axis(b)), PALETTE[1]));
        }
    }
    figure(&c.title, s.finish(), legend(&entries))
}

fn ishikawa(effect: &str, cats: &[(String, Vec<String>)], accent: Rgb) -> String {
    let max_causes = cats
        .iter()
        .map(|c| c.1.len())
        .max()
        .unwrap_or(0)
        .clamp(2, 6);
    let half = 30.0 + max_causes as f64 * 11.0;
    let h = 2.0 * half + 50.0;
    let mut s = Svg::new(h);
    let mid = h / 2.0;
    let head_w = (W * 0.22).clamp(90.0, 170.0);
    let head_x = W - head_w;
    let spine_from = 6.0;
    s.line((spine_from, mid), (head_x, mid), 1.6, accent);
    // The head: the effect.
    let per = (((head_w - 12.0) / 4.8) as usize).max(6);
    let lines = wrap_chars(effect, per);
    let hh = (lines.len() as f64 * 11.0 + 12.0).max(34.0);
    s.rect(head_x, mid - hh / 2.0, head_w, hh, light(accent, 0.18));
    s.rect(head_x, mid - hh / 2.0, 3.0, hh, accent);
    for (i, l) in lines.iter().enumerate() {
        s.text(
            head_x + 8.0,
            mid - hh / 2.0 + 15.0 + i as f64 * 11.0,
            9.0,
            true,
            BLACK,
            "start",
            l,
        );
    }
    let n_top = cats.len().div_ceil(2);
    let n_bottom = cats.len() - n_top;
    let place = |k: usize, cnt: usize| -> f64 {
        let span = head_x - spine_from - 40.0;
        spine_from + 40.0 + span * (k as f64 + 1.0) / (cnt as f64 + 1.0)
    };
    let col_w = (head_x - spine_from) / (n_top.max(1) as f64 + 1.0);
    for (i, (cat, causes)) in cats.iter().enumerate() {
        let up = i % 2 == 0;
        let (k, cnt) = if up {
            (i / 2, n_top)
        } else {
            (i / 2, n_bottom)
        };
        let x_spine = place(k, cnt);
        let end_y = if up { mid - half } else { mid + half };
        let x_end = x_spine - half * 0.55;
        s.line((x_end, end_y), (x_spine, mid), 1.0, accent);
        let ty = if up { end_y - 4.0 } else { end_y + 11.0 };
        s.text(
            x_end - 4.0,
            ty,
            9.0,
            true,
            accent,
            "start",
            &fit(cat, col_w + 20.0, 9.0),
        );
        for (j, cause) in causes.iter().take(max_causes).enumerate() {
            let f = (j as f64 + 1.0) / (max_causes as f64 + 1.0);
            let y = end_y + (mid - end_y) * f;
            let x = x_end + (x_spine - x_end) * f;
            s.line((x - 4.0, y), (x, y), 0.5, GREY);
            s.text(
                x - 6.0,
                y + 2.5,
                7.5,
                false,
                BLACK,
                "end",
                &fit(cause, col_w - 4.0, 7.5),
            );
        }
        if causes.len() > max_causes {
            let y = mid + if up { -4.0 } else { 10.0 };
            s.text(
                x_spine + 3.0,
                y,
                7.0,
                false,
                GREY,
                "start",
                &format!("+{}", causes.len() - max_causes),
            );
        }
    }
    format!("<figure class=\"chart\">{}</figure>\n", s.finish())
}

fn wrap_chars(s: &str, per: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in s.split_whitespace() {
        let cand = if cur.is_empty() {
            w.to_string()
        } else {
            format!("{cur} {w}")
        };
        if cand.chars().count() <= per {
            cur = cand;
        } else {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            cur = fit(w, per as f64 * 4.8, 9.0);
        }
    }
    lines.push(cur);
    lines
}

fn table(header: &[String], rows: &[Vec<String>], widths: &[f64], accent: Rgb) -> String {
    let cols = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return String::new();
    }
    let w: Vec<f64> = (0..cols)
        .map(|i| widths.get(i).copied().filter(|w| *w > 0.0).unwrap_or(1.0))
        .collect();
    let total: f64 = w.iter().sum();
    let ink = if dark(accent) { "#fff" } else { "#000" };
    let mut o = String::from("<table class=\"data\"><colgroup>");
    for x in &w {
        let _ = write!(o, "<col style=\"width:{:.2}%\">", x / total * 100.0);
    }
    let _ = write!(
        o,
        "</colgroup><thead><tr style=\"background:{};color:{ink}\">",
        hex(accent)
    );
    for i in 0..cols {
        let _ = write!(
            o,
            "<th>{}</th>",
            esc(header.get(i).map_or("", String::as_str))
        );
    }
    o.push_str("</tr></thead><tbody>");
    for (ri, r) in rows.iter().enumerate() {
        let _ = write!(o, "<tr{}>", if ri % 2 == 1 { " class=\"z\"" } else { "" });
        for i in 0..cols {
            let _ = write!(o, "<td>{}</td>", esc(r.get(i).map_or("", String::as_str)));
        }
        o.push_str("</tr>");
    }
    o.push_str("</tbody></table>\n");
    o
}

fn risk_matrix(
    title: &str,
    rows: &[String],
    cols: &[String],
    levels: &[Vec<u8>],
    colours: &[String],
    marks: &[(usize, usize, String)],
) -> String {
    let mut o = String::from("<div class=\"chart\">");
    if !title.is_empty() {
        let _ = write!(o, "<h4>{}</h4>", esc(title));
    }
    o.push_str("<table class=\"risk\">");
    for (r, rname) in rows.iter().enumerate() {
        let _ = write!(o, "<tr><th>{}</th>", esc(rname));
        for c in 0..cols.len() {
            let lvl = levels.get(r).and_then(|v| v.get(c)).copied().unwrap_or(0) as usize;
            let col = colours
                .get(lvl)
                .and_then(|h| Template::parse_colour(h))
                .unwrap_or((0.9, 0.9, 0.9));
            let here: Vec<String> = marks
                .iter()
                .filter(|m| m.0 == r && m.1 == c)
                .map(|m| esc(&m.2))
                .collect();
            let ink = if dark(col) { "#fff" } else { "#000" };
            let _ = write!(
                o,
                "<td style=\"background:{};color:{ink}\">{}</td>",
                hex(col),
                here.join(", ")
            );
        }
        o.push_str("</tr>");
    }
    o.push_str("<tr><th></th>");
    for c in cols {
        let _ = write!(o, "<th class=\"col\">{}</th>", esc(c));
    }
    o.push_str("</tr></table></div>\n");
    o
}

fn kpis(cards: &[Kpi], accent: Rgb) -> String {
    let mut o = String::from("<div class=\"kpis\">");
    for k in cards {
        let _ = write!(
            o,
            "<div class=\"kpi\" style=\"background:{};border-top:3px solid {}\"><div class=\"kl\">{}</div><div class=\"kv\" style=\"color:{}\">{}</div><div class=\"kn\">{}</div></div>",
            hex(light(accent, 0.10)),
            hex(accent),
            esc(&k.label),
            hex(accent),
            esc(&k.value),
            esc(&k.note)
        );
    }
    o.push_str("</div>\n");
    o
}

fn callout(tone: Tone, title: &str, text: &str) -> String {
    let (fill, bar) = tone_colours(tone);
    let t = if title.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"ct\" style=\"color:{}\">{}</div>",
            hex(bar),
            esc(title)
        )
    };
    format!(
        "<div class=\"callout\" style=\"background:{};border-left:4px solid {}\">{t}<div>{}</div></div>\n",
        hex(fill),
        hex(bar),
        esc(text).replace('\n', "<br>")
    )
}

fn cover(c: &Cover, t: &Template) -> String {
    let colour = Template::parse_colour(&c.colour).unwrap_or_else(|| t.rgb());
    let ink = if dark(colour) { "#fff" } else { "#000" };
    let app = if c.app.is_empty() { "UTSim" } else { &c.app };
    let mut o = format!(
        "<section class=\"cover\"><div class=\"band\" style=\"background:{};color:{ink}\">",
        hex(colour)
    );
    if !c.icon_png.is_empty() {
        let _ = write!(
            o,
            "<img class=\"icon\" alt=\"\" src=\"data:image/png;base64,{}\">",
            base64(&c.icon_png)
        );
    }
    let _ = write!(o, "<div class=\"app\">{}</div>", esc(app));
    if !t.company.is_empty() {
        let _ = write!(o, "<div class=\"co\">{}</div>", esc(&t.company));
    }
    let _ = write!(o, "<h1>{}</h1></div><div class=\"cbody\">", esc(&c.title));
    if !c.subtitle.is_empty() {
        let _ = write!(o, "<p class=\"sub\">{}</p>", esc(&c.subtitle));
    }
    o.push_str("<dl>");
    for (k, v) in [
        ("Autor", &c.author),
        ("Data", &c.date),
        ("Origem dos dados", &c.source),
    ] {
        if !v.trim().is_empty() {
            let _ = write!(o, "<dt>{k}</dt><dd>{}</dd>", esc(v));
        }
    }
    let _ = writeln!(
        o,
        "</dl><hr style=\"border-color:{}\"></div></section>",
        hex(colour)
    );
    o
}

const CSS: &str = "\
*{box-sizing:border-box}body{margin:0;font-family:Helvetica,Arial,sans-serif;color:#000;background:#fff;font-size:10pt;line-height:1.35}\
.frame{width:100%;max-width:210mm;margin:0 auto;border-collapse:collapse}.frame>thead>tr>td,.frame>tfoot>tr>td,.frame>tbody>tr>td{padding:0 20mm;vertical-align:top}\
.hd{display:flex;justify-content:space-between;align-items:baseline;border-bottom:.8pt solid var(--a);padding:8mm 0 2mm;margin-bottom:4mm}\
.hd b{color:var(--a);font-size:9pt}.hd small{display:block;color:#595959;font-size:7.5pt}.hd span{color:#595959;font-size:8pt}\
.ft{display:flex;justify-content:space-between;border-top:.4pt solid #b3b3b3;padding:2mm 0 6mm;margin-top:4mm;color:#595959;font-size:7.5pt}\
h1.t{color:var(--a);font-size:16pt;margin:0 0 2pt}.subt{color:#595959;margin:0 0 8pt}\
h2{color:var(--a);font-size:12pt;margin:14pt 0 6pt;border-bottom:.6pt solid var(--a);padding-bottom:3pt;break-after:avoid}\
h4{font-size:9.5pt;margin:8pt 0 2pt;break-after:avoid}p{margin:0 0 5pt}\
dl.kv{display:grid;grid-template-columns:minmax(0,30%) 1fr;gap:2pt 8pt;font-size:9pt;margin:0 0 6pt}dl.kv dt{font-weight:bold}dl.kv dd{margin:0}\
table.data{width:100%;border-collapse:collapse;font-size:8pt;margin:0 0 8pt;table-layout:fixed}\
table.data th,table.data td{border:.4pt solid #999;padding:2.5pt;text-align:left;vertical-align:top;overflow-wrap:anywhere}\
table.data thead{display:table-header-group}table.data tr{break-inside:avoid}table.data tr.z td{background:#f4f5f7}\
ul{margin:0 0 6pt;padding-left:14pt}.sig{margin-top:30pt;width:220pt;border-top:.5pt solid #000;padding-top:3pt;font-size:9pt;break-inside:avoid}\
figure.chart,div.chart{margin:0 0 8pt;break-inside:avoid}.fig{width:100%;height:auto;display:block;font-family:Helvetica,Arial,sans-serif}\
.legend{display:flex;flex-wrap:wrap;gap:2pt 14pt;font-size:7.5pt;margin-top:2pt}.legend i{display:inline-block;width:7pt;height:7pt;margin-right:3pt}\
.callout{padding:6pt 10pt;margin:0 0 8pt;font-size:9.5pt;break-inside:avoid}.ct{font-weight:bold;font-size:10pt;margin-bottom:2pt}\
.kpis{display:flex;gap:8pt;margin:0 0 8pt;flex-wrap:wrap}.kpi{flex:1 1 0;min-width:90pt;padding:6pt 8pt;break-inside:avoid}\
.kl{font-size:8pt;color:#595959}.kv{font-size:18pt;font-weight:bold}.kn{font-size:7.5pt;color:#595959}\
table.risk{border-collapse:collapse;font-size:8pt}table.risk th{font-weight:normal;padding:2pt 6pt;text-align:left}table.risk th.col{text-align:center}\
table.risk td{border:.4pt solid #999;width:80pt;height:34pt;padding:2pt 3pt;font-weight:bold;font-size:7pt;vertical-align:top}\
nav.toc h2{font-size:16pt;border:0}nav.toc ol{list-style:none;padding:0}nav.toc li{padding:2pt 0}nav.toc a{color:#000;text-decoration:none}nav.toc a:hover{text-decoration:underline}\
.cover{break-after:page;height:270mm;overflow:hidden}.band{padding:60pt 42pt 36pt;height:40%;overflow:hidden}.band .icon{width:48pt;height:48pt;display:block;margin-bottom:10pt}\
.band .app{font-weight:bold;font-size:12pt}.band .co{font-size:10pt}.band h1{font-size:26pt;margin:60pt 0 0}\
.cbody{padding:20pt 42pt}.cbody .sub{font-size:14pt;color:#595959}.cbody dl{display:grid;grid-template-columns:120pt 1fr;gap:4pt 0;font-size:10pt}.cbody dt{font-weight:bold}.cbody dd{margin:0}\
.pb{break-after:page}\
@page{size:A4;margin:12mm}\
@media print{.frame{max-width:none}.frame>thead>tr>td,.frame>tfoot>tr>td,.frame>tbody>tr>td{padding:0 8mm}}\
@media screen{body{background:#eef0f3}.frame{background:#fff;box-shadow:0 0 8px #0003;margin:12px auto}.cover{max-width:210mm;margin:12px auto;background:#fff;box-shadow:0 0 8px #0003}}";

/// The document as one HTML page. `generated` goes in the footer (who and when printed it).
pub fn render(doc: &Document, t: &Template, generated: &str) -> String {
    let accent = t.rgb();
    let company = if t.company.is_empty() {
        "UTSim Seis Sigma"
    } else {
        t.company.as_str()
    };
    let mut o = String::new();
    let _ = write!(
        o,
        "<!DOCTYPE html>\n<html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>:root{{--a:{}}}{CSS}</style></head><body>\n",
        esc(&doc.title),
        hex(accent)
    );
    if let Some(c) = &doc.cover {
        o.push_str(&cover(c, t));
    }
    // Header and footer repeat on every printed page as thead and tfoot of the frame.
    let _ = write!(
        o,
        "<table class=\"frame\"><thead><tr><td><div class=\"hd\"><div><b>{}</b>{}</div><span>{}</span></div></td></tr></thead>",
        esc(company),
        if t.header.is_empty() {
            String::new()
        } else {
            format!("<small>{}</small>", esc(&t.header))
        },
        esc(&doc.title)
    );
    let foot = if t.footer.is_empty() {
        esc(generated)
    } else {
        format!("{} · {}", esc(&t.footer), esc(generated))
    };
    let _ = writeln!(
        o,
        "<tfoot><tr><td><div class=\"ft\"><span>{foot}</span></div></td></tr></tfoot><tbody><tr><td>"
    );
    let heads = doc.headings();
    if doc.toc && !heads.is_empty() {
        o.push_str("<nav class=\"toc\"><h2>Sumário</h2><ol>");
        for (i, h) in heads.iter().enumerate() {
            let _ = write!(o, "<li><a href=\"#sec-{}\">{}</a></li>", i + 1, esc(h));
        }
        o.push_str("</ol></nav><div class=\"pb\"></div>\n");
    }
    let _ = write!(o, "<h1 class=\"t\">{}</h1>", esc(&doc.title));
    if !doc.subtitle.is_empty() {
        let _ = write!(o, "<p class=\"subt\">{}</p>", esc(&doc.subtitle));
    }
    o.push('\n');
    let mut hi = 0;
    for b in &doc.blocks {
        match b {
            Block::Heading(s) => {
                hi += 1;
                let _ = writeln!(o, "<h2 id=\"sec-{hi}\">{}</h2>", esc(s));
            }
            Block::Paragraph(s) => {
                let _ = writeln!(o, "<p>{}</p>", esc(s).replace('\n', "<br>"));
            }
            Block::KeyValues(v) => {
                o.push_str("<dl class=\"kv\">");
                for (k, x) in v {
                    let _ = write!(
                        o,
                        "<dt>{}</dt><dd>{}</dd>",
                        esc(k),
                        if x.is_empty() { "—".into() } else { esc(x) }
                    );
                }
                o.push_str("</dl>\n");
            }
            Block::Table {
                header,
                rows,
                widths,
                ..
            } => o.push_str(&table(header, rows, widths, accent)),
            Block::Bullets(v) => {
                o.push_str("<ul>");
                for s in v {
                    let _ = write!(o, "<li>{}</li>", esc(s));
                }
                o.push_str("</ul>\n");
            }
            Block::Signatures(names) => {
                for s in names {
                    let _ = writeln!(o, "<div class=\"sig\">{}</div>", esc(s));
                }
            }
            Block::Space(p) => {
                let _ = writeln!(o, "<div style=\"height:{}pt\"></div>", n(p.max(0.0)));
            }
            Block::PageBreak => o.push_str("<div class=\"pb\"></div>\n"),
            Block::Chart(c) => o.push_str(&chart(c, accent)),
            Block::Callout { tone, title, text } => o.push_str(&callout(*tone, title, text)),
            Block::KpiCards(v) => o.push_str(&kpis(v, accent)),
            Block::Ishikawa { effect, categories } => {
                o.push_str(&ishikawa(effect, categories, accent));
            }
            Block::RiskMatrix {
                title,
                rows,
                cols,
                levels,
                colours,
                marks,
            } => o.push_str(&risk_matrix(title, rows, cols, levels, colours, marks)),
        }
    }
    o.push_str("</td></tr></tbody></table>\n</body></html>\n");
    o
}

#[cfg(test)]
mod tests {
    use super::super::tests::showcase;
    use super::*;

    #[test]
    fn showcase_html_has_everything() {
        let d = showcase();
        let h = render(&d, &Template::default(), "Gerado por Ana em 08/11/2026");
        assert!(h.contains("class=\"cover\""));
        assert!(h.contains("Redução de refugo na usinagem da tampa"));
        assert!(h.contains("Origem dos dados"));
        assert!(h.contains("Sumário"));
        for (i, s) in d.headings().iter().enumerate() {
            assert!(h.contains(&format!("href=\"#sec-{}\">{}</a>", i + 1, esc(s))));
            assert!(h.contains(&format!("id=\"sec-{}\"", i + 1)));
        }
        let figs = d
            .blocks
            .iter()
            .filter(|b| matches!(b, Block::Chart(_) | Block::Ishikawa { .. }))
            .count();
        assert!(figs >= 5);
        assert_eq!(h.matches("<svg").count(), figs);
        assert!(h.contains("class=\"risk\""));
        assert!(h.contains("Prensa sem trava"));
        assert!(h.contains("Furo fora de posição"));
        assert!(h.contains("Gabarito gasto"));
        assert!(h.contains("Gerado por Ana"));
        assert!(h.contains("R$ 48.000"));
        assert!(!h.contains("http://") && !h.contains("https://"));
    }

    #[test]
    fn html_is_escaped_and_self_contained() {
        let mut d = Document::new("<script>alert(1)</script>", "a & b \"c\"");
        d.cover = Some(Cover {
            title: "<b>x</b>".into(),
            icon_png: vec![137, 80, 78, 71, 13, 10, 26, 10],
            ..Cover::default()
        });
        d.heading("<script>h</script>");
        d.para("<script>p</script>");
        d.callout(Tone::Danger, "<i>", "<script>c</script>");
        let h = render(&d, &Template::default(), "g");
        assert!(!h.contains("<script"));
        assert!(h.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(h.contains("a &amp; b &quot;c&quot;"));
        assert!(h.contains("data:image/png;base64,iVBORw0KGgo="));
        assert!(!h.contains("http://") && !h.contains("https://"));
    }

    #[test]
    fn writes_the_demo_when_asked() {
        if let Some(dir) = std::env::var_os("UTSIM_CAPTURAS") {
            std::fs::create_dir_all(&dir).unwrap();
            let h = render(&showcase(), &Template::default(), "Gerado em 08/11/2026");
            std::fs::write(
                std::path::Path::new(&dir).join("relatorio_demonstracao.html"),
                h,
            )
            .unwrap();
        }
    }
}
