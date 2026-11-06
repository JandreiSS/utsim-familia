//! PDF 1.4 writer of a [`Document`]: the standard Helvetica and Helvetica-Bold fonts with
//! the WinAnsi encoding (every letter of Portuguese), text wrapped by the fonts' widths,
//! page breaks, the header of a table repeated on the next page, the company's header and
//! footer and "página n de N". No compression and no date inside: the same document gives
//! the same bytes.

use std::fmt::Write as _;

use super::{Block, Document, Template};

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
    Fill {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        grey: f64,
    },
    Frame {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
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
        let draw_row = |l: &mut Layout, cells: &[Vec<String>], h: f64, bold: bool, fill: bool| {
            let mut x = MARGIN;
            let top = l.y;
            if fill {
                l.op(Op::Fill {
                    x: MARGIN,
                    y: top - h,
                    w: l.inner(),
                    h,
                    grey: 0.9,
                });
            }
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
                        rgb: BLACK,
                        s: s.clone(),
                    });
                }
                x += cols[i];
            }
            l.y -= h;
        };
        self.need(head_h + lh + 2.0 * pad);
        draw_row(self, &head, head_h, true, true);
        for r in rows {
            let cells = row_lines(r, false);
            let h = cells.iter().map(Vec::len).max().unwrap_or(1) as f64 * lh + 2.0 * pad;
            if self.y - h < self.bottom() {
                self.new_page();
                draw_row(self, &head, head_h, true, true);
            }
            draw_row(self, &cells, h, false, false);
        }
        self.y -= 6.0;
    }
}

fn lay_out(doc: &Document, t: &Template) -> Layout {
    let (w, h) = doc.size.points();
    let mut l = Layout {
        w,
        h,
        accent: t.rgb(),
        pages: Vec::new(),
        y: 0.0,
    };
    l.new_page();
    let accent = l.accent;
    let inner = l.inner();
    l.lines(MARGIN, inner, 16.0, true, accent, &doc.title);
    if !doc.subtitle.is_empty() {
        l.lines(MARGIN, inner, 10.0, false, GREY, &doc.subtitle);
    }
    l.y -= 8.0;
    for b in &doc.blocks {
        match b {
            Block::Heading(s) => {
                l.need(40.0);
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
    // Header: the company and the document's title, with the accent rule.
    let top = h - MARGIN;
    let company = if t.company.is_empty() {
        "UTSim Seis Sigma"
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
            Op::Fill { x, y, w, h, grey } => {
                let _ = writeln!(
                    c,
                    "{} g {} {} {} {} re f 0 g",
                    num(*grey),
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
            "<< /Title ({}) /Producer (UTSim Seis Sigma) >>",
            esc(&encode(&doc.title))
        )
        .into_bytes(),
    );
    for (i, ops) in l.pages.iter().enumerate() {
        let content = page_content(ops, i, n, doc, t, l.w, l.h, generated);
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
