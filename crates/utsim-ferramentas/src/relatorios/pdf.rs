//! PDF 1.4 writer of a [`Document`]: the standard Helvetica and Helvetica-Bold fonts with
//! the WinAnsi encoding (every letter of Portuguese), text wrapped by the fonts' widths,
//! page breaks, the header of a table repeated on the next page, the company's header and
//! footer and "página n de N". No compression and no date inside: the same document gives
//! the same bytes.

use std::fmt::Write as _;

use super::{Block, Chart, ChartKind, Cover, Document, Kpi, Template, Tone};

const MARGIN: f64 = 42.0;
const HEADER_H: f64 = 26.0;
const FOOTER_H: f64 = 24.0;

/// Widths of Helvetica (1/1000 em) for the printable ASCII range, from the font's metrics.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278,
    278, // ' '..'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // digits
    278, 278, 584, 584, 584, 556, 1015, // ':'..'@'
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667,
    611, 722, 667, 944, 667, 667, 611, // 'A'..'Z'
    278, 278, 278, 469, 556, 333, // '['..'`'
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500,
    278, 556, 500, 722, 500, 500, 500, // 'a'..'z'
    334, 260, 334, 584, // '{'..'~'
];

/// Characters outside WinAnsi written as close ASCII.
fn substitute(c: char) -> Option<&'static str> {
    Some(match c {
        '→' => "->",
        '←' => "<-",
        '≥' => ">=",
        '≤' => "<=",
        '≠' => "!=",
        '−' | '‒' => "-",
        '✖' => "x",
        '✕' | '✗' => "x",
        '✓' | '✔' => "v",
        'σ' => "s",
        'μ' => "u",
        'Δ' => "D",
        '\t' => "    ",
        _ => return None,
    })
}

/// The WinAnsi byte of a character (`None`: not in the encoding).
pub fn winansi(c: char) -> Option<u8> {
    let u = c as u32;
    if (0x20..=0x7E).contains(&u) || (0xA0..=0xFF).contains(&u) {
        return Some(u as u8);
    }
    Some(match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        _ => return None,
    })
}

/// The text as WinAnsi bytes (`?` for what has no place in it).
pub fn encode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if let Some(sub) = substitute(c) {
            out.extend(sub.bytes());
        } else {
            out.push(winansi(c).unwrap_or(b'?'));
        }
    }
    out
}

/// Width of one WinAnsi byte in 1/1000 em.
fn byte_width(b: u8) -> u16 {
    match b {
        0x20..=0x7E => HELVETICA[(b - 0x20) as usize],
        0xC0..=0xC5 => 667, // À..Å
        0xC6 => 1000,       // Æ
        0xC7 => 722,        // Ç
        0xC8..=0xCB => 667, // È..Ë
        0xCC..=0xCF => 278, // Ì..Ï
        0xD0 => 722,
        0xD1 => 722,               // Ñ
        0xD2..=0xD6 | 0xD8 => 778, // Ò..Ö, Ø
        0xD7 => 584,               // ×
        0xD9..=0xDC => 722,        // Ù..Ü
        0xDD => 667,
        0xDE => 667,
        0xDF => 611,        // ß
        0xE0..=0xE5 => 556, // à..å
        0xE6 => 889,
        0xE7 => 500,               // ç
        0xE8..=0xEB => 556,        // è..ë
        0xEC..=0xEF => 278,        // ì..ï
        0xF0..=0xF6 | 0xF8 => 556, // ð..ö, ø
        0xF7 => 584,
        0xF9..=0xFC => 556,
        0xFD | 0xFF => 500,
        0xFE => 556,
        0xA0 => 278,
        0xB0 => 400,        // °
        0xB1 => 584,        // ±
        0xAA | 0xBA => 370, // ª º
        0xAB | 0xBB => 556,
        0xB7 => 278,
        0x85 | 0x97 => 1000,
        0x96 => 556,
        0x95 => 350,
        0x91 | 0x92 => 222,
        0x93 | 0x94 => 333,
        0x80 => 556,
        _ => 556,
    }
}

/// Width of a text in points.
pub fn width(s: &str, size: f64, bold: bool) -> f64 {
    let w: f64 = encode(s).iter().map(|b| byte_width(*b) as f64).sum();
    // The bold face is about 6 % wider; a little slack keeps the wrap on the safe side.
    w * size / 1000.0 * if bold { 1.06 } else { 1.0 }
}

/// Wraps a text into lines of at most `max` points (words longer than a line are cut).
pub fn wrap(s: &str, max: f64, size: f64, bold: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let cand = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if width(&cand, size, bold) <= max {
                cur = cand;
                continue;
            }
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            if width(word, size, bold) <= max {
                cur = word.to_string();
            } else {
                // Cut a long word.
                let mut piece = String::new();
                for ch in word.chars() {
                    let next = format!("{piece}{ch}");
                    if width(&next, size, bold) > max && !piece.is_empty() {
                        lines.push(std::mem::take(&mut piece));
                        piece.push(ch);
                    } else {
                        piece = next;
                    }
                }
                cur = piece;
            }
        }
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// A drawing operation of a page.
#[derive(Debug, Clone, PartialEq)]
enum Op {
    Text {
        x: f64,
        y: f64,
        size: f64,
        bold: bool,
        rgb: (f64, f64, f64),
        s: String,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        w: f64,
        rgb: (f64, f64, f64),
    },
    Frame {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
    FillRgb {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        rgb: (f64, f64, f64),
    },
}

const BLACK: (f64, f64, f64) = (0.0, 0.0, 0.0);
const GREY: (f64, f64, f64) = (0.35, 0.35, 0.35);

struct Layout {
    w: f64,
    h: f64,
    accent: (f64, f64, f64),
    pages: Vec<Vec<Op>>,
    y: f64,
    /// Pages drawn without the header and the footer (the cover).
    plain: Vec<bool>,
    /// Page (from 1) of each heading, in order.
    heading_pages: Vec<usize>,
}

impl Layout {
    fn top(&self) -> f64 {
        self.h - MARGIN - HEADER_H
    }

    fn bottom(&self) -> f64 {
        MARGIN + FOOTER_H
    }

    fn inner(&self) -> f64 {
        self.w - 2.0 * MARGIN
    }

    fn new_page(&mut self) {
        self.pages.push(Vec::new());
        self.plain.push(false);
        self.y = self.top();
    }

    fn need(&mut self, h: f64) {
        if self.y - h < self.bottom() {
            self.new_page();
        }
    }

    fn op(&mut self, o: Op) {
        self.pages.last_mut().expect("page").push(o);
    }

    fn text(&mut self, x: f64, size: f64, bold: bool, rgb: (f64, f64, f64), s: &str) {
        let y = self.y;
        self.op(Op::Text {
            x,
            y,
            size,
            bold,
            rgb,
            s: s.to_string(),
        });
    }

    fn lines(&mut self, x: f64, max: f64, size: f64, bold: bool, rgb: (f64, f64, f64), s: &str) {
        let lh = size * 1.3;
        for l in wrap(s, max, size, bold) {
            self.need(lh);
            self.y -= lh;
            self.text(x, size, bold, rgb, &l);
        }
    }

    fn table(&mut self, header: &[String], rows: &[Vec<String>], widths: &[f64]) {
        let n = header
            .len()
            .max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if n == 0 {
            return;
        }
        let total: f64 = (0..n)
            .map(|i| widths.get(i).copied().filter(|w| *w > 0.0).unwrap_or(1.0))
            .sum();
        let cols: Vec<f64> = (0..n)
            .map(|i| {
                widths.get(i).copied().filter(|w| *w > 0.0).unwrap_or(1.0) / total * self.inner()
            })
            .collect();
        let size = 8.0;
        let lh = 10.0;
        let pad = 2.5;
        let row_lines = |cells: &[String], bold: bool| -> Vec<Vec<String>> {
            (0..n)
                .map(|i| {
                    wrap(
                        cells.get(i).map_or("", String::as_str),
                        cols[i] - 2.0 * pad,
                        size,
                        bold,
                    )
                })
                .collect()
        };
        let head = row_lines(header, true);
        let head_h = head.iter().map(Vec::len).max().unwrap_or(1) as f64 * lh + 2.0 * pad;
        let accent = self.accent;
        let draw_row = |l: &mut Layout,
                        cells: &[Vec<String>],
                        h: f64,
                        bold: bool,
                        fill: Option<(f64, f64, f64)>| {
            let mut x = MARGIN;
            let top = l.y;
            if let Some(rgb) = fill {
                l.op(Op::FillRgb {
                    x: MARGIN,
                    y: top - h,
                    w: l.inner(),
                    h,
                    rgb,
                });
            }
            let ink = if bold && fill.is_some_and(dark) {
                WHITE
            } else {
                BLACK
            };
            for (i, c) in cells.iter().enumerate() {
                l.op(Op::Frame {
                    x,
                    y: top - h,
                    w: cols[i],
                    h,
                });
                for (k, s) in c.iter().enumerate() {
                    l.op(Op::Text {
                        x: x + pad,
                        y: top - pad - (k as f64 + 1.0) * lh + 2.0,
                        size,
                        bold,
                        rgb: ink,
                        s: s.clone(),
                    });
                }
                x += cols[i];
            }
            l.y -= h;
        };
        self.need(head_h + lh + 2.0 * pad);
        draw_row(self, &head, head_h, true, Some(accent));
        for (ri, r) in rows.iter().enumerate() {
            let cells = row_lines(r, false);
            let h = cells.iter().map(Vec::len).max().unwrap_or(1) as f64 * lh + 2.0 * pad;
            if self.y - h < self.bottom() {
                self.new_page();
                draw_row(self, &head, head_h, true, Some(accent));
            }
            let stripe = (ri % 2 == 1).then_some((0.955, 0.96, 0.97));
            draw_row(self, &cells, h, false, stripe);
        }
        self.y -= 6.0;
    }
}

/// Colours of the series of a chart.
pub(crate) const PALETTE: [(f64, f64, f64); 6] = [
    (0.12, 0.42, 0.68),
    (0.85, 0.40, 0.05),
    (0.18, 0.55, 0.20),
    (0.75, 0.15, 0.15),
    (0.50, 0.35, 0.68),
    (0.45, 0.33, 0.25),
];
pub(crate) const SIGNAL: (f64, f64, f64) = (0.80, 0.05, 0.05);
pub(crate) const REFERENCE: (f64, f64, f64) = (0.70, 0.12, 0.12);
/// Width of the reference lines (limits, 80 %) and of their legend dash.
pub(crate) const LINE_W: f64 = 1.2;

/// A number on a chart axis, with a decimal comma.
pub(crate) fn axis(x: f64) -> String {
    let a = x.abs();
    let s = if a >= 1000.0 || a == 0.0 {
        format!("{x:.0}")
    } else if a >= 10.0 {
        format!("{x:.1}")
    } else if a >= 0.1 {
        format!("{x:.2}")
    } else {
        format!("{x:.4}")
    };
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    s.replace('.', ",")
}

/// The first characters of `s` that fit in `max` points.
fn fit(s: &str, max: f64, size: f64) -> String {
    if width(s, size, false) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        let next = format!("{out}{c}…");
        if width(&next, size, false) > max {
            break;
        }
        out.push(c);
    }
    format!("{out}…")
}

impl Layout {
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, rgb: (f64, f64, f64)) {
        self.op(Op::FillRgb { x, y, w, h, rgb });
    }

    fn line(&mut self, a: (f64, f64), b: (f64, f64), w: f64, rgb: (f64, f64, f64)) {
        self.op(Op::Line {
            x1: a.0,
            y1: a.1,
            x2: b.0,
            y2: b.1,
            w,
            rgb,
        });
    }

    /// A bar or line chart with its axes, reference lines, marked points and legend.
    fn chart(&mut self, c: &Chart) {
        match c.drawn_kind() {
            ChartKind::Pareto => return self.pareto(c),
            ChartKind::Histogram => return self.histogram(c),
            ChartKind::Scatter => return self.scatter(c),
            _ => {}
        }
        let bars = c.drawn_kind() == ChartKind::Bars;
        let n = c
            .series
            .iter()
            .map(|(_, v)| v.len())
            .max()
            .unwrap_or(0)
            .max(c.labels.len());
        let plot_h = 150.0;
        let named = c.series.iter().filter(|(s, _)| !s.is_empty()).count();
        let legend = named + c.hlines.len() + c.bands.len();
        let legend_h = if legend > 1 || !c.hlines.is_empty() || !c.bands.is_empty() {
            12.0 * legend.div_ceil(3) as f64
        } else {
            0.0
        };
        self.need(plot_h + 44.0 + legend_h);
        if !c.title.is_empty() {
            self.y -= 12.0;
            self.text(MARGIN, 9.5, true, BLACK, &c.title);
        }
        let inner = self.inner();
        let left = MARGIN + 44.0;
        let right = MARGIN + inner - 4.0;
        let top = self.y - 8.0;
        let bottom = top - plot_h;
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
        let mut lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
        let mut hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !lo.is_finite() || !hi.is_finite() {
            lo = 0.0;
            hi = 1.0;
        }
        if hi - lo < 1e-12 {
            hi = lo + 1.0;
        }
        let pad = 0.06 * (hi - lo);
        if !(bars && lo == 0.0) {
            lo -= pad;
        }
        hi += pad;
        let yy = |v: f64| bottom + (v - lo) / (hi - lo) * plot_h;
        let w = right - left;
        self.op(Op::Frame {
            x: left,
            y: bottom,
            w,
            h: plot_h,
        });
        for i in 0..=4 {
            let v = lo + (hi - lo) * f64::from(i) / 4.0;
            let y = yy(v);
            if i > 0 && i < 4 {
                self.line((left, y), (right, y), 0.2, (0.85, 0.85, 0.85));
            }
            let s = axis(v);
            let sw = width(&s, 7.0, false);
            let keep = self.y;
            self.y = y - 2.5;
            self.text(left - 4.0 - sw, 7.0, false, GREY, &s);
            self.y = keep;
        }
        for (bi, (_, a, b)) in c.bands.iter().enumerate() {
            if a.is_finite() && b.is_finite() {
                let (y0, y1) = (yy(a.min(*b)), yy(a.max(*b)));
                self.rect(
                    left,
                    y0,
                    w,
                    y1 - y0,
                    light(PALETTE[(bi + 2) % PALETTE.len()], 0.16),
                );
            }
        }
        if n > 0 {
            let slot = w / n as f64;
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
                        let x = left + i as f64 * slot + (slot - group) / 2.0 + si as f64 * bw;
                        let (a, b) = (yy(0.0f64.max(lo)), yy(*y));
                        self.rect(x, a.min(b), bw * 0.92, (b - a).abs(), rgb);
                    }
                } else {
                    let pts: Vec<Option<(f64, f64)>> = v
                        .iter()
                        .enumerate()
                        .map(|(i, y)| {
                            y.is_finite()
                                .then(|| (left + (i as f64 + 0.5) * slot, yy(*y)))
                        })
                        .collect();
                    for pair in pts.windows(2) {
                        if let (Some(a), Some(b)) = (pair[0], pair[1]) {
                            self.line(a, b, 0.9, rgb);
                        }
                    }
                    if n <= 60 {
                        for (x, y) in pts.iter().flatten() {
                            self.rect(x - 1.3, y - 1.3, 2.6, 2.6, rgb);
                        }
                    }
                }
            }
            if let Some((_, v)) = c.series.first() {
                for i in &c.marks {
                    if let Some(y) = v.get(*i).filter(|y| y.is_finite()) {
                        let x = left + (*i as f64 + 0.5) * slot;
                        self.rect(x - 2.4, yy(*y) - 2.4, 4.8, 4.8, SIGNAL);
                    }
                }
            }
            // The labels under the axis, every k-th when they do not fit.
            let every = (n as f64 * 22.0 / w).ceil().max(1.0) as usize;
            for (i, l) in c.labels.iter().enumerate() {
                if i % every != 0 {
                    continue;
                }
                let s = fit(l, slot * every as f64 - 2.0, 6.5);
                let sw = width(&s, 6.5, false);
                let x = left + (i as f64 + 0.5) * slot - sw / 2.0;
                let keep = self.y;
                self.y = bottom - 9.0;
                self.text(x, 6.5, false, GREY, &s);
                self.y = keep;
            }
        }
        for (_, v) in &c.hlines {
            if v.is_finite() {
                let y = yy(*v);
                self.line((left, y), (right, y), LINE_W, REFERENCE);
            }
        }
        // Legend.
        self.y = bottom - 16.0;
        if legend_h > 0.0 {
            let col = inner / 3.0;
            let _ = col;
            let entries: Vec<(String, (f64, f64, f64), bool)> = c
                .series
                .iter()
                .enumerate()
                .filter(|(_, (s, _))| !s.is_empty())
                .map(|(i, (s, _))| (s.clone(), PALETTE[i % PALETTE.len()], !bars))
                .chain(
                    c.hlines
                        .iter()
                        .map(|(s, v)| (format!("{s} = {}", axis(*v)), REFERENCE, true)),
                )
                .chain(c.bands.iter().enumerate().map(|(bi, (s, _, _))| {
                    (
                        s.clone(),
                        light(PALETTE[(bi + 2) % PALETTE.len()], 0.35),
                        false,
                    )
                }))
                .collect();
            self.legend_rows(&entries);
            self.y += 8.0;
        }
        self.y -= 8.0;
    }
}

/// The fill and the bar colour of a callout's tone.
pub(crate) fn tone_colours(t: Tone) -> ((f64, f64, f64), (f64, f64, f64)) {
    match t {
        Tone::Info => ((0.91, 0.95, 0.99), (0.12, 0.42, 0.68)),
        Tone::Ok => ((0.91, 0.97, 0.92), (0.18, 0.55, 0.20)),
        Tone::Warn => ((1.0, 0.96, 0.88), (0.85, 0.55, 0.05)),
        Tone::Danger => ((0.99, 0.91, 0.91), (0.75, 0.15, 0.15)),
    }
}

/// A light version of a colour (for fills).
pub(crate) fn light((r, g, b): (f64, f64, f64), k: f64) -> (f64, f64, f64) {
    (
        1.0 - (1.0 - r) * k,
        1.0 - (1.0 - g) * k,
        1.0 - (1.0 - b) * k,
    )
}

/// Whether text on a colour should be white (the colour is dark).
pub(crate) fn dark((r, g, b): (f64, f64, f64)) -> bool {
    0.299 * r + 0.587 * g + 0.114 * b < 0.55
}

const WHITE: (f64, f64, f64) = (1.0, 1.0, 1.0);

impl Layout {
    /// The cover page: a band in the colour with the app and the title, then the subtitle,
    /// the author, the date and where the data came from.
    fn cover(&mut self, c: &Cover, t: &Template) {
        let colour = Template::parse_colour(&c.colour).unwrap_or_else(|| t.rgb());
        let band = self.h * 0.42;
        let (w, h) = (self.w, self.h);
        self.rect(0.0, h - band, w, band, colour);
        let ink = if dark(colour) { WHITE } else { BLACK };
        let inner = self.inner();
        self.y = h - 70.0;
        let app = if c.app.is_empty() {
            "UTSim"
        } else {
            c.app.as_str()
        };
        self.text(MARGIN, 12.0, true, ink, app);
        if !t.company.is_empty() {
            self.y -= 16.0;
            self.text(MARGIN, 10.0, false, ink, &t.company);
        }
        self.y = h - band * 0.55;
        for l in wrap(&c.title, inner, 26.0, true) {
            self.y -= 31.0;
            self.text(MARGIN, 26.0, true, ink, &l);
        }
        self.y = h - band - 36.0;
        if !c.subtitle.is_empty() {
            for l in wrap(&c.subtitle, inner, 14.0, false) {
                self.y -= 18.0;
                self.text(MARGIN, 14.0, false, GREY, &l);
            }
            self.y -= 18.0;
        }
        let kv: Vec<(&str, &str)> = [
            ("Autor", c.author.as_str()),
            ("Data", c.date.as_str()),
            ("Origem dos dados", c.source.as_str()),
        ]
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .collect();
        for (k, v) in kv {
            self.y -= 18.0;
            self.text(MARGIN, 10.0, true, BLACK, k);
            for (i, l) in wrap(v, inner - 120.0, 10.0, false).iter().enumerate() {
                if i > 0 {
                    self.y -= 13.0;
                }
                self.text(MARGIN + 120.0, 10.0, false, BLACK, l);
            }
        }
        self.line((MARGIN, 90.0), (w - MARGIN, 90.0), 0.6, colour);
        let _ = h;
    }

    /// The table of contents: each heading with its page, with dot leaders.
    fn toc(&mut self, entries: &[(String, usize)]) {
        let accent = self.accent;
        let inner = self.inner();
        self.lines(MARGIN, inner, 16.0, true, accent, "Sumário");
        self.y -= 10.0;
        for (s, p) in entries {
            self.need(16.0);
            self.y -= 16.0;
            let num = p.to_string();
            let nw = width(&num, 10.0, false);
            let text = fit(s, inner - nw - 30.0, 10.0);
            let tw = width(&text, 10.0, false);
            self.text(MARGIN, 10.0, false, BLACK, &text);
            let dot = width(".", 10.0, false);
            let from = MARGIN + tw + 6.0;
            let to = MARGIN + inner - nw - 6.0;
            if to > from {
                let n = ((to - from) / dot).floor() as usize;
                self.text(from, 10.0, false, GREY, &".".repeat(n));
            }
            self.text(MARGIN + inner - nw, 10.0, false, BLACK, &num);
        }
    }

    fn callout(&mut self, tone: Tone, title: &str, text: &str) {
        let inner = self.inner();
        let (fill, bar) = tone_colours(tone);
        let lines = wrap(text, inner - 24.0, 9.5, false);
        let h = 10.0 + if title.is_empty() { 0.0 } else { 14.0 } + lines.len() as f64 * 12.5 + 6.0;
        self.need(h + 6.0);
        let top = self.y;
        self.rect(MARGIN, top - h, inner, h, fill);
        self.rect(MARGIN, top - h, 4.0, h, bar);
        self.y = top - 6.0;
        if !title.is_empty() {
            self.y -= 12.0;
            self.text(MARGIN + 14.0, 10.0, true, bar, title);
        }
        for l in &lines {
            self.y -= 12.5;
            self.text(MARGIN + 14.0, 9.5, false, BLACK, l);
        }
        self.y = top - h - 8.0;
    }

    fn kpi_cards(&mut self, cards: &[Kpi]) {
        let inner = self.inner();
        let per = cards.len().clamp(1, 4);
        let gap = 8.0;
        let w = (inner - gap * (per as f64 - 1.0)) / per as f64;
        let h = 62.0;
        let accent = self.accent;
        for row in cards.chunks(per) {
            self.need(h + 8.0);
            let top = self.y;
            for (i, k) in row.iter().enumerate() {
                let x = MARGIN + i as f64 * (w + gap);
                self.rect(x, top - h, w, h, light(accent, 0.10));
                self.rect(x, top - 3.0, w, 3.0, accent);
                self.y = top - 16.0;
                self.text(x + 8.0, 8.0, false, GREY, &fit(&k.label, w - 16.0, 8.0));
                self.y = top - 38.0;
                self.text(x + 8.0, 18.0, true, accent, &fit(&k.value, w - 16.0, 18.0));
                self.y = top - 53.0;
                self.text(x + 8.0, 7.5, false, GREY, &fit(&k.note, w - 16.0, 7.5));
            }
            self.y = top - h - 8.0;
        }
    }

    /// The fishbone: the spine to the head with the effect, a bone per category alternately
    /// above and below, the causes written along each bone.
    fn ishikawa(&mut self, effect: &str, cats: &[(String, Vec<String>)]) {
        let inner = self.inner();
        let max_causes = cats
            .iter()
            .map(|c| c.1.len())
            .max()
            .unwrap_or(0)
            .clamp(2, 6);
        let half = 30.0 + max_causes as f64 * 11.0;
        let h = 2.0 * half + 20.0;
        self.need(h + 24.0);
        self.y -= 14.0;
        let accent = self.accent;
        let top = self.y;
        let mid = top - h / 2.0;
        let head_w = (inner * 0.22).clamp(90.0, 170.0);
        let head_x = MARGIN + inner - head_w;
        let spine_from = MARGIN + 6.0;
        self.line((spine_from, mid), (head_x, mid), 1.6, accent);
        // The head: the effect.
        let lines = wrap(effect, head_w - 12.0, 9.0, true);
        let hh = (lines.len() as f64 * 11.0 + 12.0).max(34.0);
        self.rect(head_x, mid - hh / 2.0, head_w, hh, light(accent, 0.18));
        self.rect(head_x, mid - hh / 2.0, 3.0, hh, accent);
        self.y = mid + hh / 2.0 - 4.0;
        for l in &lines {
            self.y -= 11.0;
            self.text(head_x + 8.0, 9.0, true, BLACK, l);
        }
        let n_top = cats.len().div_ceil(2);
        let n_bottom = cats.len() - n_top;
        let place = |k: usize, n: usize| -> f64 {
            let span = head_x - spine_from - 40.0;
            spine_from + 40.0 + span * (k as f64 + 1.0) / (n as f64 + 1.0)
        };
        for (i, (cat, causes)) in cats.iter().enumerate() {
            let up = i % 2 == 0;
            let (k, n) = if up {
                (i / 2, n_top)
            } else {
                (i / 2, n_bottom)
            };
            let x_spine = place(k, n);
            let dx = half * 0.55;
            let end_y = if up { mid + half } else { mid - half };
            let x_end = x_spine - dx;
            self.line((x_end, end_y), (x_spine, mid), 1.0, accent);
            // The category at the end of the bone.
            self.y = if up { end_y + 4.0 } else { end_y - 11.0 };
            let col_w = (head_x - spine_from) / (n_top.max(1) as f64 + 1.0);
            self.text(x_end - 4.0, 9.0, true, accent, &fit(cat, col_w + 20.0, 9.0));
            // The causes, from the end towards the spine.
            for (j, c) in causes.iter().take(max_causes).enumerate() {
                let f = (j as f64 + 1.0) / (max_causes as f64 + 1.0);
                let y = end_y + (mid - end_y) * f;
                let x = x_end + (x_spine - x_end) * f;
                self.line((x - 4.0, y), (x, y), 0.5, GREY);
                self.y = y - 2.5;
                let txt = fit(c, col_w - 4.0, 7.5);
                let tw = width(&txt, 7.5, false);
                self.text(x - 6.0 - tw, 7.5, false, BLACK, &txt);
            }
            if causes.len() > max_causes {
                self.y = mid + if up { 4.0 } else { -10.0 };
                self.text(
                    x_spine + 3.0,
                    7.0,
                    false,
                    GREY,
                    &format!("+{}", causes.len() - max_causes),
                );
            }
        }
        self.y = top - h - 8.0;
    }

    /// The risk matrix: a cell per row and column filled with the colour of its level and
    /// the marks written in it.
    #[allow(clippy::too_many_arguments)]
    fn risk_matrix(
        &mut self,
        title: &str,
        rows: &[String],
        cols: &[String],
        levels: &[Vec<u8>],
        colours: &[String],
        marks: &[(usize, usize, String)],
    ) {
        let inner = self.inner();
        let label_w = (inner * 0.2).min(110.0);
        let nc = cols.len().max(1);
        let cw = ((inner - label_w) / nc as f64).min(90.0);
        let ch = 34.0;
        let h = ch * rows.len() as f64 + 26.0 + if title.is_empty() { 0.0 } else { 16.0 };
        self.need(h + 8.0);
        if !title.is_empty() {
            self.y -= 12.0;
            self.text(MARGIN, 9.5, true, BLACK, title);
            self.y -= 4.0;
        }
        let top = self.y;
        for (r, rname) in rows.iter().enumerate() {
            let y0 = top - (r as f64 + 1.0) * ch;
            self.y = y0 + ch / 2.0 - 3.0;
            self.text(MARGIN, 8.0, false, BLACK, &fit(rname, label_w - 6.0, 8.0));
            for c in 0..cols.len() {
                let lvl = levels.get(r).and_then(|v| v.get(c)).copied().unwrap_or(0) as usize;
                let col = colours
                    .get(lvl)
                    .and_then(|h| Template::parse_colour(h))
                    .unwrap_or((0.9, 0.9, 0.9));
                let x0 = MARGIN + label_w + c as f64 * cw;
                self.rect(x0, y0, cw, ch, col);
                self.op(Op::Frame {
                    x: x0,
                    y: y0,
                    w: cw,
                    h: ch,
                });
                let here: Vec<&str> = marks
                    .iter()
                    .filter(|m| m.0 == r && m.1 == c)
                    .map(|m| m.2.as_str())
                    .collect();
                if !here.is_empty() {
                    let ink = if dark(col) { WHITE } else { BLACK };
                    let ls = wrap(&here.join(", "), cw - 6.0, 7.0, true);
                    for (i, l) in ls.iter().take(3).enumerate() {
                        self.y = y0 + ch - 10.0 - i as f64 * 8.5;
                        self.text(x0 + 3.0, 7.0, true, ink, l);
                    }
                }
            }
        }
        let bottom = top - rows.len() as f64 * ch;
        for (c, cname) in cols.iter().enumerate() {
            let x0 = MARGIN + label_w + c as f64 * cw;
            self.y = bottom - 10.0;
            let s = fit(cname, cw - 4.0, 7.5);
            let sw = width(&s, 7.5, false);
            self.text(x0 + (cw - sw) / 2.0, 7.5, false, BLACK, &s);
        }
        self.y = bottom - 22.0;
    }
}

/// The plot area of a chart: its edges and the scale of y.
struct Area {
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
    lo: f64,
    hi: f64,
}

impl Area {
    fn y(&self, v: f64) -> f64 {
        self.bottom + (v - self.lo) / (self.hi - self.lo) * (self.top - self.bottom)
    }
}

/// `lo` and `hi` of the values, padded, never empty.
pub(crate) fn range(vals: &[f64], from_zero: bool) -> (f64, f64) {
    let mut lo = vals
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min);
    let mut hi = vals
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    if from_zero {
        lo = lo.min(0.0);
    }
    if hi - lo < 1e-12 {
        hi = lo + 1.0;
    }
    let pad = 0.06 * (hi - lo);
    if !(from_zero && lo == 0.0) {
        lo -= pad;
    }
    (lo, hi + pad)
}

impl Layout {
    /// Room, title, frame, grid and the y labels of a chart; the area to draw in.
    fn chart_area(&mut self, title: &str, lo: f64, hi: f64, extra: f64) -> Area {
        let plot_h = 150.0;
        self.need(plot_h + 44.0 + extra);
        if !title.is_empty() {
            self.y -= 12.0;
            self.text(MARGIN, 9.5, true, BLACK, title);
        }
        let inner = self.inner();
        let a = Area {
            left: MARGIN + 44.0,
            right: MARGIN + inner - 30.0,
            top: self.y - 8.0,
            bottom: self.y - 8.0 - plot_h,
            lo,
            hi,
        };
        self.op(Op::Frame {
            x: a.left,
            y: a.bottom,
            w: a.right - a.left,
            h: plot_h,
        });
        for i in 0..=4 {
            let v = lo + (hi - lo) * f64::from(i) / 4.0;
            let y = a.y(v);
            if i > 0 && i < 4 {
                self.line((a.left, y), (a.right, y), 0.2, (0.85, 0.85, 0.85));
            }
            let s = axis(v);
            let sw = width(&s, 7.0, false);
            let keep = self.y;
            self.y = y - 2.5;
            self.text(a.left - 4.0 - sw, 7.0, false, GREY, &s);
            self.y = keep;
        }
        a
    }

    /// Numeric labels under the x axis of `x0..x1`, five ticks.
    fn x_ticks(&mut self, a: &Area, x0: f64, x1: f64) {
        for i in 0..=4 {
            let v = x0 + (x1 - x0) * f64::from(i) / 4.0;
            let x = a.left + (a.right - a.left) * f64::from(i) / 4.0;
            let s = axis(v);
            let sw = width(&s, 6.5, false);
            let keep = self.y;
            self.y = a.bottom - 9.0;
            self.text(x - sw / 2.0, 6.5, false, GREY, &s);
            self.y = keep;
        }
    }

    /// A legend under a chart: a square for areas (bars, points, zones), a dash in the
    /// line's colour and width for lines (series of a line chart, limits, the 80 %).
    fn legend(&mut self, a: &Area, entries: &[(String, (f64, f64, f64), bool)]) {
        self.y = a.bottom - 16.0;
        self.legend_rows(entries);
    }

    fn legend_rows(&mut self, entries: &[(String, (f64, f64, f64), bool)]) {
        let col = self.inner() / 3.0;
        for (k, (s, rgb, line)) in entries.iter().enumerate() {
            let x = MARGIN + (k % 3) as f64 * col;
            if k % 3 == 0 {
                self.y -= 12.0;
            }
            let y = self.y;
            if *line {
                self.line((x - 1.0, y + 3.5), (x + 9.0, y + 3.5), LINE_W, *rgb);
            } else {
                self.rect(x, y, 7.0, 7.0, *rgb);
            }
            self.text(x + 12.0, 7.5, false, BLACK, &fit(s, col - 16.0, 7.5));
        }
        self.y -= 8.0;
    }

    /// Pareto: bars in the given order on a scale whose top is the total, the cumulative
    /// line on the same scale (so it reads as a percentage, right axis) and the 80 % line.
    fn pareto(&mut self, c: &Chart) {
        let v: Vec<f64> = c
            .series
            .first()
            .map(|s| s.1.iter().map(|x| x.max(0.0)).collect())
            .unwrap_or_default();
        let total: f64 = v.iter().filter(|x| x.is_finite()).sum();
        let total = if total > 0.0 { total } else { 1.0 };
        let a = self.chart_area(&c.title, 0.0, total, 24.0);
        let n = v.len().max(1);
        let slot = (a.right - a.left) / n as f64;
        let accent = self.accent;
        let mut cum = 0.0;
        let mut prev: Option<(f64, f64)> = None;
        for (i, x) in v.iter().enumerate() {
            let x0 = a.left + i as f64 * slot;
            self.rect(
                x0 + slot * 0.12,
                a.bottom,
                slot * 0.76,
                a.y(*x) - a.bottom,
                accent,
            );
            cum += x;
            let p = (x0 + slot / 2.0, a.y(cum));
            if let Some(q) = prev {
                self.line(q, p, 1.0, PALETTE[1]);
            }
            self.rect(p.0 - 1.6, p.1 - 1.6, 3.2, 3.2, PALETTE[1]);
            prev = Some(p);
            let s = fit(c.labels.get(i).map_or("", String::as_str), slot - 2.0, 6.5);
            let sw = width(&s, 6.5, false);
            let keep = self.y;
            self.y = a.bottom - 9.0;
            self.text(x0 + (slot - sw) / 2.0, 6.5, false, GREY, &s);
            self.y = keep;
        }
        let y80 = a.y(0.8 * total);
        self.line((a.left, y80), (a.right, y80), LINE_W, REFERENCE);
        for pct in [0, 50, 80, 100] {
            let y = a.y(total * f64::from(pct) / 100.0);
            let keep = self.y;
            self.y = y - 2.5;
            self.text(a.right + 3.0, 7.0, false, GREY, &format!("{pct} %"));
            self.y = keep;
        }
        let name = c.series.first().map_or(String::new(), |s| s.0.clone());
        self.legend(
            &a,
            &[
                (
                    if name.is_empty() {
                        "valor".into()
                    } else {
                        name
                    },
                    accent,
                    false,
                ),
                ("acumulado (%)".into(), PALETTE[1], true),
                ("80 %".into(), REFERENCE, true),
            ],
        );
    }

    /// Histogram of the first series by Sturges' rule, with the `hlines` as vertical limits.
    fn histogram(&mut self, c: &Chart) {
        let data: Vec<f64> = c
            .series
            .first()
            .map(|s| s.1.iter().copied().filter(|x| x.is_finite()).collect())
            .unwrap_or_default();
        let mut xs: Vec<f64> = data.clone();
        xs.extend(c.hlines.iter().map(|h| h.1).filter(|x| x.is_finite()));
        let (x0, x1) = range(&xs, false);
        let k = ((data.len().max(1) as f64).log2().ceil() as usize + 1).clamp(3, 30);
        let bw = (x1 - x0) / k as f64;
        let mut counts = vec![0usize; k];
        for x in &data {
            let i = (((x - x0) / bw).floor() as usize).min(k - 1);
            counts[i] += 1;
        }
        let top = counts.iter().copied().max().unwrap_or(1).max(1) as f64;
        let a = self.chart_area(&c.title, 0.0, top * 1.08, 24.0);
        let px = |v: f64| a.left + (v - x0) / (x1 - x0) * (a.right - a.left);
        let accent = self.accent;
        for (i, n) in counts.iter().enumerate() {
            let l = px(x0 + i as f64 * bw);
            let r = px(x0 + (i as f64 + 1.0) * bw);
            if *n > 0 {
                self.rect(
                    l + 0.5,
                    a.bottom,
                    r - l - 1.0,
                    a.y(*n as f64) - a.bottom,
                    accent,
                );
            }
        }
        self.x_ticks(&a, x0, x1);
        let mut legend = vec![(
            c.series
                .first()
                .map_or(String::new(), |s| format!("{} (n = {})", s.0, data.len())),
            accent,
            false,
        )];
        for (name, v) in &c.hlines {
            if v.is_finite() {
                let x = px(*v);
                self.line((x, a.bottom), (x, a.top), LINE_W, REFERENCE);
                let keep = self.y;
                self.y = a.top - 9.0;
                self.text(x + 2.0, 7.0, true, REFERENCE, name);
                self.y = keep;
                legend.push((format!("{name} = {}", axis(*v)), REFERENCE, true));
            }
        }
        self.legend(&a, &legend);
    }

    /// Scatter of the second series against the first, with the least squares line.
    fn scatter(&mut self, c: &Chart) {
        let (Some(xs), Some(ys)) = (c.series.first(), c.series.get(1)) else {
            return;
        };
        let pts: Vec<(f64, f64)> =
            xs.1.iter()
                .zip(&ys.1)
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .map(|(x, y)| (*x, *y))
                .collect();
        let (x0, x1) = range(&pts.iter().map(|p| p.0).collect::<Vec<_>>(), false);
        let (lo, hi) = range(&pts.iter().map(|p| p.1).collect::<Vec<_>>(), false);
        let a = self.chart_area(&c.title, lo, hi, 24.0);
        let px = |v: f64| a.left + (v - x0) / (x1 - x0) * (a.right - a.left);
        let accent = self.accent;
        for (x, y) in &pts {
            self.rect(px(*x) - 1.8, a.y(*y) - 1.8, 3.6, 3.6, accent);
        }
        self.x_ticks(&a, x0, x1);
        let n = pts.len() as f64;
        let mut legend = vec![(format!("{} × {}", ys.0, xs.0), accent, false)];
        if pts.len() >= 2 {
            let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
            let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
            let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
            if sxx > 0.0 {
                let b = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx;
                let b0 = my - b * mx;
                let clamp = |y: f64| y.clamp(lo, hi);
                self.line(
                    (px(x0), a.y(clamp(b0 + b * x0))),
                    (px(x1), a.y(clamp(b0 + b * x1))),
                    1.0,
                    PALETTE[1],
                );
                legend.push((
                    format!("y = {} + {}·x", axis(b0), axis(b)),
                    PALETTE[1],
                    true,
                ));
            }
        }
        self.legend(&a, &legend);
    }
}

/// The room the start of a block needs below its heading.
fn keep_with(next: Option<&Block>) -> f64 {
    match next {
        Some(Block::Chart(_)) => 200.0,
        Some(Block::Ishikawa { .. }) => 220.0,
        Some(Block::RiskMatrix { rows, .. }) => 40.0 + 34.0 * rows.len() as f64,
        Some(Block::KpiCards(_)) => 70.0,
        Some(Block::Table { .. }) | Some(Block::Callout { .. }) => 50.0,
        Some(_) => 24.0,
        None => 0.0,
    }
}

/// The layout: the cover, the contents (two passes: the first finds the page of each
/// heading with the contents already taking their room) and the blocks.
fn lay_out(doc: &Document, t: &Template) -> Layout {
    if !doc.toc || doc.headings().is_empty() {
        return lay_out_with(doc, t, None);
    }
    let blank: Vec<(String, usize)> = doc.headings().iter().map(|h| (h.to_string(), 0)).collect();
    let first = lay_out_with(doc, t, Some(&blank));
    let entries: Vec<(String, usize)> = doc
        .headings()
        .iter()
        .zip(&first.heading_pages)
        .map(|(h, p)| (h.to_string(), *p))
        .collect();
    lay_out_with(doc, t, Some(&entries))
}

fn lay_out_with(doc: &Document, t: &Template, toc: Option<&[(String, usize)]>) -> Layout {
    let (w, h) = doc.size.points();
    let mut l = Layout {
        w,
        h,
        accent: t.rgb(),
        pages: Vec::new(),
        y: 0.0,
        plain: Vec::new(),
        heading_pages: Vec::new(),
    };
    l.new_page();
    if let Some(c) = &doc.cover {
        *l.plain.last_mut().expect("page") = true;
        l.cover(c, t);
        l.new_page();
    }
    if let Some(entries) = toc {
        l.toc(entries);
        l.new_page();
    }
    let accent = l.accent;
    let inner = l.inner();
    l.lines(MARGIN, inner, 16.0, true, accent, &doc.title);
    if !doc.subtitle.is_empty() {
        l.lines(MARGIN, inner, 10.0, false, GREY, &doc.subtitle);
    }
    l.y -= 8.0;
    for (bi, b) in doc.blocks.iter().enumerate() {
        match b {
            Block::Heading(s) => {
                // A heading stays on the page of what follows it.
                l.need(40.0 + keep_with(doc.blocks.get(bi + 1)));
                l.heading_pages.push(l.pages.len());
                l.y -= 8.0;
                l.lines(MARGIN, inner, 12.0, true, accent, s);
                l.y -= 3.0;
                let y = l.y;
                l.op(Op::Line {
                    x1: MARGIN,
                    y1: y,
                    x2: MARGIN + inner,
                    y2: y,
                    w: 0.6,
                    rgb: accent,
                });
                l.y -= 4.0;
            }
            Block::Paragraph(s) => {
                l.lines(MARGIN, inner, 10.0, false, BLACK, s);
                l.y -= 4.0;
            }
            Block::KeyValues(v) => {
                let kw = (inner * 0.3).min(170.0);
                for (k, x) in v {
                    let ks = wrap(k, kw - 6.0, 9.0, true);
                    let vs = wrap(if x.is_empty() { "—" } else { x }, inner - kw, 9.0, false);
                    let n = ks.len().max(vs.len());
                    let lh = 12.0;
                    l.need(n as f64 * lh);
                    let top = l.y;
                    for (i, s) in ks.iter().enumerate() {
                        l.y = top - (i as f64 + 1.0) * lh;
                        l.text(MARGIN, 9.0, true, BLACK, s);
                    }
                    for (i, s) in vs.iter().enumerate() {
                        l.y = top - (i as f64 + 1.0) * lh;
                        l.text(MARGIN + kw, 9.0, false, BLACK, s);
                    }
                    l.y = top - n as f64 * lh - 2.0;
                }
                l.y -= 4.0;
            }
            Block::Table {
                header,
                rows,
                widths,
                ..
            } => l.table(header, rows, widths),
            Block::Bullets(v) => {
                for s in v {
                    let ls = wrap(s, inner - 14.0, 10.0, false);
                    for (i, x) in ls.iter().enumerate() {
                        l.need(13.0);
                        l.y -= 13.0;
                        if i == 0 {
                            l.text(MARGIN + 2.0, 10.0, false, BLACK, "•");
                        }
                        l.text(MARGIN + 14.0, 10.0, false, BLACK, x);
                    }
                }
                l.y -= 4.0;
            }
            Block::Signatures(names) => {
                for n in names {
                    l.need(48.0);
                    l.y -= 36.0;
                    let y = l.y;
                    l.op(Op::Line {
                        x1: MARGIN,
                        y1: y,
                        x2: MARGIN + 220.0,
                        y2: y,
                        w: 0.5,
                        rgb: BLACK,
                    });
                    l.y -= 11.0;
                    l.text(MARGIN, 9.0, false, BLACK, n);
                }
                l.y -= 4.0;
            }
            Block::Space(p) => l.y -= p,
            Block::PageBreak => l.new_page(),
            Block::Chart(c) => l.chart(c),
            Block::Callout { tone, title, text } => l.callout(*tone, title, text),
            Block::KpiCards(v) => l.kpi_cards(v),
            Block::Ishikawa { effect, categories } => l.ishikawa(effect, categories),
            Block::RiskMatrix {
                title,
                rows,
                cols,
                levels,
                colours,
                marks,
            } => l.risk_matrix(title, rows, cols, levels, colours, marks),
        }
    }
    l
}

fn esc(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() + 2);
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                s.push('\\');
                s.push(b as char);
            }
            0x20..=0x7E => s.push(b as char),
            _ => {
                let _ = write!(s, "\\{b:03o}");
            }
        }
    }
    s
}

fn num(x: f64) -> String {
    let s = format!("{x:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

#[allow(clippy::too_many_arguments)]
fn page_content(
    ops: &[Op],
    page: usize,
    pages: usize,
    doc: &Document,
    t: &Template,
    w: f64,
    h: f64,
    generated: &str,
    plain: bool,
) -> String {
    let mut c = String::new();
    let text =
        |c: &mut String, x: f64, y: f64, size: f64, bold: bool, rgb: (f64, f64, f64), s: &str| {
            let _ = writeln!(
                c,
                "BT {} {} {} rg /{} {} Tf {} {} Td ({}) Tj ET",
                num(rgb.0),
                num(rgb.1),
                num(rgb.2),
                if bold { "F2" } else { "F1" },
                num(size),
                num(x),
                num(y),
                esc(&encode(s))
            );
        };
    if !plain {
        // Header: the company and the document's title, with the accent rule.
        let top = h - MARGIN;
        let company = if t.company.is_empty() {
            doc.app_name()
        } else {
            t.company.as_str()
        };
        text(&mut c, MARGIN, top - 9.0, 9.0, true, t.rgb(), company);
        if !t.header.is_empty() {
            text(&mut c, MARGIN, top - 19.0, 7.5, false, GREY, &t.header);
        }
        let tw = width(&doc.title, 8.0, false);
        if page > 0 {
            text(
                &mut c,
                w - MARGIN - tw,
                top - 9.0,
                8.0,
                false,
                GREY,
                &doc.title,
            );
        }
        let (r, g, b) = t.rgb();
        let _ = writeln!(
            c,
            "{} {} {} RG 0.8 w {} {} m {} {} l S",
            num(r),
            num(g),
            num(b),
            num(MARGIN),
            num(top - HEADER_H + 4.0),
            num(w - MARGIN),
            num(top - HEADER_H + 4.0)
        );
    }
    for o in ops {
        match o {
            Op::Text {
                x,
                y,
                size,
                bold,
                rgb,
                s,
            } => text(&mut c, *x, *y, *size, *bold, *rgb, s),
            Op::Line {
                x1,
                y1,
                x2,
                y2,
                w,
                rgb,
            } => {
                let _ = writeln!(
                    c,
                    "{} {} {} RG {} w {} {} m {} {} l S",
                    num(rgb.0),
                    num(rgb.1),
                    num(rgb.2),
                    num(*w),
                    num(*x1),
                    num(*y1),
                    num(*x2),
                    num(*y2)
                );
            }
            Op::FillRgb { x, y, w, h, rgb } => {
                let _ = writeln!(
                    c,
                    "{} {} {} rg {} {} {} {} re f 0 g",
                    num(rgb.0),
                    num(rgb.1),
                    num(rgb.2),
                    num(*x),
                    num(*y),
                    num(*w),
                    num(*h)
                );
            }
            Op::Frame { x, y, w, h } => {
                let _ = writeln!(
                    c,
                    "0.6 G 0.4 w {} {} {} {} re S",
                    num(*x),
                    num(*y),
                    num(*w),
                    num(*h)
                );
            }
        }
    }
    if !plain {
        // Footer.
        let fy = MARGIN - 4.0;
        let _ = writeln!(
            c,
            "0.7 G 0.4 w {} {} m {} {} l S",
            num(MARGIN),
            num(fy + 12.0),
            num(w - MARGIN),
            num(fy + 12.0)
        );
        let left = if t.footer.is_empty() {
            generated.to_string()
        } else {
            format!("{} · {}", t.footer, generated)
        };
        text(&mut c, MARGIN, fy, 7.5, false, GREY, &left);
        let pn = format!("página {} de {}", page + 1, pages);
        let pw = width(&pn, 7.5, false);
        text(&mut c, w - MARGIN - pw, fy, 7.5, false, GREY, &pn);
    }
    c
}

/// The document as PDF bytes. `generated` goes in the footer (who and when printed it).
pub fn render(doc: &Document, t: &Template, generated: &str) -> Vec<u8> {
    let l = lay_out(doc, t);
    let n = l.pages.len();
    // Objects: 1 catalog, 2 pages, 3 and 4 fonts, 5 info, then a page and its content each.
    let mut objs: Vec<Vec<u8>> = Vec::new();
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 6 + 2 * i)).collect();
    objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).into_bytes());
    for f in ["Helvetica", "Helvetica-Bold"] {
        objs.push(
            format!("<< /Type /Font /Subtype /Type1 /BaseFont /{f} /Encoding /WinAnsiEncoding >>")
                .into_bytes(),
        );
    }
    objs.push(
        format!(
            "<< /Title ({}) /Producer ({}) >>",
            esc(&encode(&doc.title)),
            esc(&encode(doc.app_name()))
        )
        .into_bytes(),
    );
    for (i, ops) in l.pages.iter().enumerate() {
        let content = page_content(ops, i, n, doc, t, l.w, l.h, generated, l.plain[i]);
        objs.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                num(l.w),
                num(l.h),
                7 + 2 * i
            )
            .into_bytes(),
        );
        let mut s = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        s.extend(content.as_bytes());
        s.extend(b"endstream");
        objs.push(s);
    }
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objs.len());
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Number of pages of a rendered document (for the tests and the screen).
pub fn page_count(doc: &Document, t: &Template) -> usize {
    lay_out(doc, t).pages.len()
}
