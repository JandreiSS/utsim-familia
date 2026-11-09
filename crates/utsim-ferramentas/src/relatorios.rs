//! Reports (`docs/plano_lean_seis_sigma.md`, 4.6 and 4.7; decision of section 7: the PDF is
//! made by the program itself, without a browser): a small document model (title, meta,
//! headings, paragraphs, key-value lists, tables, bullets, signatures and bar or line
//! charts), the company's
//! template (name, header, footer and accent colour) and two renderers, both pure and
//! deterministic (the same document gives the same bytes):
//!
//! - [`pdf`]: PDF 1.4 with the standard Helvetica fonts (no font embedded, nothing to
//!   install), WinAnsi encoding for the Portuguese letters, A4 portrait or A3 landscape,
//!   automatic page breaks, table headers repeated on each page and "página n de N"; an
//!   optional cover page and table of contents; tables with the header in the accent colour
//!   and striped rows; charts of bars, lines, Pareto, histogram and scatter; the Ishikawa
//!   diagram, the coloured risk matrix, highlighted boxes and indicator cards;
//! - [`csv`]: the tables of a document, `;` separated with a decimal comma (spreadsheets in
//!   Portuguese open it directly).
//!
//! The `.xlsx` is written by the store (one sheet per table).

pub mod pdf;

use serde::{Deserialize, Serialize};

/// A table of a document: `(name, header, rows)`.
pub type TableRef<'a> = (&'a str, &'a [String], &'a [Vec<String>]);

/// A block of a document.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(String),
    Paragraph(String),
    /// Label and value pairs (the label in bold).
    KeyValues(Vec<(String, String)>),
    /// A table: header, rows and the relative width of each column (empty: equal).
    Table {
        name: String,
        header: Vec<String>,
        rows: Vec<Vec<String>>,
        widths: Vec<f64>,
    },
    Bullets(Vec<String>),
    /// Signature lines: one per name.
    Signatures(Vec<String>),
    /// Vertical space, in points.
    Space(f64),
    /// Starts a new page.
    PageBreak,
    /// A chart drawn with lines and rectangles (no image).
    Chart(Chart),
    /// A highlighted box: a conclusion, a warning.
    Callout {
        tone: Tone,
        title: String,
        text: String,
    },
    /// Indicators side by side, the value in large type.
    KpiCards(Vec<Kpi>),
    /// The cause and effect diagram: the effect at the head and the causes on each bone.
    Ishikawa {
        effect: String,
        /// `(category, causes)`, in drawing order.
        categories: Vec<(String, Vec<String>)>,
    },
    /// A risk matrix coloured by level: `levels[row][col]` indexes `colours` (`#RRGGBB`),
    /// rows from top to bottom; `marks` puts a label in a cell `(row, col, text)`.
    RiskMatrix {
        title: String,
        rows: Vec<String>,
        cols: Vec<String>,
        levels: Vec<Vec<u8>>,
        colours: Vec<String>,
        marks: Vec<(usize, usize, String)>,
    },
}

/// The tone of a [`Block::Callout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Info,
    Ok,
    Warn,
    Danger,
}

/// An indicator of a [`Block::KpiCards`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Kpi {
    pub label: String,
    pub value: String,
    /// A line under the value (target, trend, unit).
    pub note: String,
}

/// The kind of a [`Chart`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    /// Bars when [`Chart::bars`], else lines (the charts made before the kinds existed).
    #[default]
    Auto,
    Bars,
    Lines,
    /// Bars of the first series in the given order, the cumulative percentage as a line on
    /// a scale of 0 to 100 % and the line of 80 %.
    Pareto,
    /// The first series is the raw data; the classes by Sturges' rule; `hlines` are the
    /// limits on the x axis (LIE, LSE, target).
    Histogram,
    /// The first series is x and the second y, with the least squares line.
    Scatter,
}

/// The cover page of a document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cover {
    /// The app that made the document (`UTSim Seis Sigma`...).
    pub app: String,
    pub title: String,
    pub subtitle: String,
    pub author: String,
    /// As it is shown (`08/11/2026`).
    pub date: String,
    /// Where the data came from (the model and run, a data set...).
    pub source: String,
    /// The colour of the band, `#RRGGBB` (the template's when empty).
    pub colour: String,
    /// The app's icon as PNG (used by the HTML; the PDF draws the band without it).
    pub icon_png: Vec<u8>,
}

/// A bar or line chart of a document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chart {
    pub title: String,
    /// Bars (grouped when there are several series) instead of lines.
    pub bars: bool,
    /// The labels of the points, under the axis.
    pub labels: Vec<String>,
    /// `(name, values)`; an empty name stays out of the legend.
    pub series: Vec<(String, Vec<f64>)>,
    /// Horizontal reference lines: limits, takt, target.
    pub hlines: Vec<(String, f64)>,
    /// Points of the first series to mark (signals of a control chart).
    pub marks: Vec<usize>,
    /// What to draw ([`ChartKind::Auto`] follows `bars`).
    pub kind: ChartKind,
    /// Shaded horizontal zones `(name, from, to)`: the zones of a control chart, the
    /// tolerance of a capability.
    pub bands: Vec<(String, f64, f64)>,
}

impl Chart {
    /// The kind actually drawn.
    pub fn drawn_kind(&self) -> ChartKind {
        match self.kind {
            ChartKind::Auto if self.bars => ChartKind::Bars,
            ChartKind::Auto => ChartKind::Lines,
            k => k,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PageSize {
    #[default]
    #[serde(rename = "a4")]
    A4Portrait,
    #[serde(rename = "a3_paisagem")]
    A3Landscape,
}

impl PageSize {
    /// Width and height in points.
    pub fn points(self) -> (f64, f64) {
        match self {
            PageSize::A4Portrait => (595.28, 841.89),
            PageSize::A3Landscape => (1190.55, 841.89),
        }
    }
}

/// A document to print.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub title: String,
    pub subtitle: String,
    pub size: PageSize,
    pub blocks: Vec<Block>,
    /// A cover page before the content.
    pub cover: Option<Cover>,
    /// A table of contents of the headings, with their pages, after the cover.
    pub toc: bool,
}

impl Document {
    pub fn new(title: &str, subtitle: &str) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            ..Self::default()
        }
    }

    pub fn heading(&mut self, s: &str) -> &mut Self {
        self.blocks.push(Block::Heading(s.into()));
        self
    }

    pub fn para(&mut self, s: &str) -> &mut Self {
        if !s.trim().is_empty() {
            self.blocks.push(Block::Paragraph(s.into()));
        }
        self
    }

    pub fn kv(&mut self, pairs: Vec<(String, String)>) -> &mut Self {
        self.blocks.push(Block::KeyValues(pairs));
        self
    }

    pub fn table(
        &mut self,
        name: &str,
        header: &[&str],
        rows: Vec<Vec<String>>,
        widths: &[f64],
    ) -> &mut Self {
        self.blocks.push(Block::Table {
            name: name.into(),
            header: header.iter().map(|s| s.to_string()).collect(),
            rows,
            widths: widths.to_vec(),
        });
        self
    }

    pub fn bullets(&mut self, items: Vec<String>) -> &mut Self {
        if !items.is_empty() {
            self.blocks.push(Block::Bullets(items));
        }
        self
    }

    pub fn chart(&mut self, c: Chart) -> &mut Self {
        self.blocks.push(Block::Chart(c));
        self
    }

    pub fn callout(&mut self, tone: Tone, title: &str, text: &str) -> &mut Self {
        self.blocks.push(Block::Callout {
            tone,
            title: title.into(),
            text: text.into(),
        });
        self
    }

    pub fn kpis(&mut self, cards: Vec<Kpi>) -> &mut Self {
        if !cards.is_empty() {
            self.blocks.push(Block::KpiCards(cards));
        }
        self
    }

    /// The headings of the document, in order (the entries of the table of contents).
    pub fn headings(&self) -> Vec<&str> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading(s) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn signatures(&mut self, names: Vec<String>) -> &mut Self {
        self.blocks.push(Block::Signatures(names));
        self
    }

    /// The tables of the document: `(name, header, rows)`.
    pub fn tables(&self) -> Vec<TableRef<'_>> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Table {
                    name, header, rows, ..
                } => Some((name.as_str(), header.as_slice(), rows.as_slice())),
                _ => None,
            })
            .collect()
    }

    /// Every text of the document, for the tests and the search.
    pub fn plain_text(&self) -> String {
        let mut out = vec![self.title.clone(), self.subtitle.clone()];
        for b in &self.blocks {
            match b {
                Block::Heading(s) | Block::Paragraph(s) => out.push(s.clone()),
                Block::KeyValues(v) => out.extend(v.iter().map(|(k, x)| format!("{k}: {x}"))),
                Block::Table { header, rows, .. } => {
                    out.push(header.join(" | "));
                    out.extend(rows.iter().map(|r| r.join(" | ")));
                }
                Block::Bullets(v) | Block::Signatures(v) => out.extend(v.iter().cloned()),
                Block::Chart(c) => out.push(c.title.clone()),
                Block::Callout { title, text, .. } => {
                    out.push(title.clone());
                    out.push(text.clone());
                }
                Block::KpiCards(v) => out.extend(
                    v.iter()
                        .map(|k| format!("{}: {} {}", k.label, k.value, k.note)),
                ),
                Block::Ishikawa { effect, categories } => {
                    out.push(effect.clone());
                    for (c, cs) in categories {
                        out.push(c.clone());
                        out.extend(cs.iter().cloned());
                    }
                }
                Block::RiskMatrix { title, marks, .. } => {
                    out.push(title.clone());
                    out.extend(marks.iter().map(|m| m.2.clone()));
                }
                Block::Space(_) | Block::PageBreak => {}
            }
        }
        out.join("\n")
    }
}

/// The company's template of the reports (section 4.7): kept in the portfolio or program
/// folder's `modelos/relatorio.json`; the default has no company name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    #[serde(rename = "empresa", default)]
    pub company: String,
    /// A line under the company name (unit, address...).
    #[serde(rename = "cabecalho", default)]
    pub header: String,
    #[serde(rename = "rodape", default)]
    pub footer: String,
    /// Accent colour of the titles and rules, `#RRGGBB`.
    #[serde(rename = "cor", default = "default_colour")]
    pub colour: String,
    /// Version of the template (a project records the one it was printed with).
    #[serde(rename = "versao", default = "one")]
    pub version: u32,
}

fn default_colour() -> String {
    "#1C62B4".into()
}

fn one() -> u32 {
    1
}

impl Default for Template {
    fn default() -> Self {
        Self {
            company: String::new(),
            header: String::new(),
            footer: String::new(),
            colour: default_colour(),
            version: 1,
        }
    }
}

impl Template {
    pub fn from_json(text: &str) -> Result<Template, String> {
        serde_json::from_str(text).map_err(|e| format!("modelo de relatório inválido: {e}"))
    }

    /// A colour `#RRGGBB` as RGB fractions (`None` when it is not one).
    pub fn parse_colour(text: &str) -> Option<(f64, f64, f64)> {
        let h = text.trim().trim_start_matches('#');
        let p = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        match (h.len(), p(0), p(2), p(4)) {
            (6, Some(r), Some(g), Some(b)) => {
                Some((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))
            }
            _ => None,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("template")
    }

    /// The accent colour as RGB fractions (the default when the text is not `#RRGGBB`).
    pub fn rgb(&self) -> (f64, f64, f64) {
        let h = self.colour.trim().trim_start_matches('#');
        let p = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        match (h.len(), p(0), p(2), p(4)) {
            (6, Some(r), Some(g), Some(b)) => {
                (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)
            }
            _ => (28.0 / 255.0, 98.0 / 255.0, 180.0 / 255.0),
        }
    }
}

/// The tables of a document as CSV (one block per table, `;` separated, decimal comma
/// kept as written).
pub fn csv(doc: &Document) -> String {
    let mut out = String::new();
    for (name, header, rows) in doc.tables() {
        if !out.is_empty() {
            out.push('\n');
        }
        if !name.is_empty() {
            out.push_str(&cell(name));
            out.push('\n');
        }
        out.push_str(&header.iter().map(|c| cell(c)).collect::<Vec<_>>().join(";"));
        out.push('\n');
        for r in rows {
            out.push_str(&r.iter().map(|c| cell(c)).collect::<Vec<_>>().join(";"));
            out.push('\n');
        }
    }
    out
}

fn cell(s: &str) -> String {
    if s.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests;
