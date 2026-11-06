//! Reports (`docs/plano_lean_seis_sigma.md`, 4.6 and 4.7; decision of section 7: the PDF is
//! made by the program itself, without a browser): a small document model (title, meta,
//! headings, paragraphs, key-value lists, tables, bullets, signatures), the company's
//! template (name, header, footer and accent colour) and two renderers, both pure and
//! deterministic (the same document gives the same bytes):
//!
//! - [`pdf`]: PDF 1.4 with the standard Helvetica fonts (no font embedded, nothing to
//!   install), WinAnsi encoding for the Portuguese letters, A4 portrait or A3 landscape,
//!   automatic page breaks, table headers repeated on each page and "página n de N";
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
