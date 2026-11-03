//! Shared pieces of the diagram types: SVG document, HTML page, palette, plan view.

use std::fmt::Write as _;

use crate::html::PAGE_CSS;
use crate::svg::CSS;
use crate::text::esc;

/// Extra styles of the diagram types (on top of [`CSS`]).
pub const EXTRA_CSS: &str = r#"
svg.fluxo .raia{fill:var(--card);stroke:var(--line)}
svg.fluxo .b{font-weight:700}
svg.fluxo .mut{fill:var(--muted)}
svg.fluxo .etq{font-size:10px}
svg.fluxo .fino{stroke:var(--stroke);stroke-width:1.2;fill:none}
svg.fluxo .cruza{stroke:var(--hot);stroke-width:2.4;fill:none}
svg.fluxo .caixa2{fill:var(--card);stroke:var(--stroke);stroke-width:1.2}
svg.fluxo .eixo{stroke:var(--muted);stroke-width:1}
"#;

/// Colours that read well on light and dark backgrounds.
pub const PALETTE: [&str; 8] = [
    "#2563eb", "#c2410c", "#16a34a", "#9333ea", "#0891b2", "#ca8a04", "#db2777", "#6b7280",
];

/// Complete SVG document (or just the element, to embed).
pub fn svg_doc(w: f64, h: f64, title: &str, body: &str, standalone: bool) -> String {
    let mut s = String::new();
    if standalone {
        s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    }
    let _ = write!(
        s,
        "<svg class=\"fluxo\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" \
         width=\"{w:.0}\" height=\"{h:.0}\" role=\"img\" aria-label=\"{t}\"><title>{t}</title>",
        t = esc(title)
    );
    if standalone {
        let _ = write!(s, "<style>{CSS}{EXTRA_CSS}</style>");
    }
    s.push_str(
        "<defs><marker id=\"ponta\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" \
         markerHeight=\"8\" orient=\"auto-start-reverse\"><path class=\"ponta\" d=\"M0 0L10 5L0 10z\"/></marker>\
         <marker id=\"ponta-ret\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" \
         markerHeight=\"8\" orient=\"auto-start-reverse\"><path class=\"ponta-ret\" d=\"M0 0L10 5L0 10z\"/></marker></defs>",
    );
    let _ = write!(
        s,
        "<rect class=\"fundo\" width=\"{w:.0}\" height=\"{h:.0}\"/>"
    );
    s.push_str(body);
    s.push_str("</svg>");
    s
}

/// Text element.
pub fn texto(x: f64, y: f64, class: &str, t: &str) -> String {
    format!(
        "<text class=\"{class}\" x=\"{x:.1}\" y=\"{y:.1}\">{}</text>",
        esc(t)
    )
}

/// Text cut to `max` characters, with an ellipsis.
pub fn curto(t: &str, max: usize) -> String {
    if t.chars().count() <= max {
        t.to_string()
    } else {
        let mut s: String = t.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

/// HTML page with the footer that says what the diagram is and where the data came from.
pub fn pagina(titulo: &str, o_que_e: &str, corpo: &str, origem: &str) -> String {
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{t}</title><style>{CSS}{EXTRA_CSS}{PAGE_CSS}</style></head><body>\
         <h1>{t}</h1><p class=\"muted\">{w}</p>{corpo}\
         <hr><p class=\"muted\"><b>Origem dos dados:</b> {o}</p></body></html>",
        t = esc(titulo),
        w = esc(o_que_e),
        o = esc(origem)
    )
}

/// Scale from model coordinates (x, y of the plan) to a canvas.
pub struct Plano {
    pub w: f64,
    pub h: f64,
    min: (f64, f64),
    scale: f64,
}

const MARGIN: f64 = 80.0;

impl Plano {
    /// Fits the points into about 1000 x 700; a single point or a line still gets a canvas.
    pub fn new(points: &[(f64, f64)]) -> Plano {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if points.is_empty() {
            (x0, y0, x1, y1) = (0.0, 0.0, 1.0, 1.0);
        }
        let (dx, dy) = ((x1 - x0).max(1e-9), (y1 - y0).max(1e-9));
        let scale = ((1000.0 - 2.0 * MARGIN) / dx).min((620.0 - 2.0 * MARGIN) / dy);
        // A flat plan (one line) keeps its proportions but gets some height.
        let scale = scale.min(60.0);
        Plano {
            w: (dx * scale + 2.0 * MARGIN).max(400.0),
            h: (dy * scale + 2.0 * MARGIN).max(260.0),
            min: (x0, y0),
            scale,
        }
    }

    pub fn at(&self, p: (f64, f64)) -> (f64, f64) {
        (
            MARGIN + (p.0 - self.min.0) * self.scale,
            MARGIN + (p.1 - self.min.1) * self.scale,
        )
    }
}

/// What a diagram type produces before it becomes a page.
pub struct Parte {
    /// The drawing, as a standalone SVG document (none for a side-by-side page).
    pub svg: Option<String>,
    /// HTML under the drawing: tables, notes, or the whole body when there is no `svg`.
    pub tabelas: String,
    pub csv: Option<String>,
}
