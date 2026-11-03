//! The flowchart as SVG: boxes with the standard symbol, curved arrows with the routing
//! and the transport on them, loops (rework) dashed, and a side panel with the resources.

use std::fmt::Write as _;

use crate::graph::{Direction, Edge, FlowGraph, Node, Resource, Symbol};
use crate::layout::{GLYPH_AREA, LineStyle, PAD, lines};
use crate::text::{esc, text_width};

/// Styles of the drawing and the page; light and dark.
pub const CSS: &str = r#"
:root{--bg:#fff;--fg:#1c1f24;--muted:#5d6672;--line:#d8dde3;--accent:#2563eb;--hot:#c2410c;--card:#f5f7fa;--stroke:#475569;
--c-inicio:#e5e7eb;--c-fim:#e5e7eb;--c-operacao:#dbeafe;--c-transporte:#dcfce7;--c-inspecao:#fef3c7;--c-espera:#fee2e2;--c-armazenagem:#ede9fe}
@media(prefers-color-scheme:dark){:root{--bg:#14171b;--fg:#e6e9ee;--muted:#9aa4b0;--line:#2d343c;--accent:#6ea0ff;--hot:#fb923c;--card:#1d2228;--stroke:#94a3b8;
--c-inicio:#2b313a;--c-fim:#2b313a;--c-operacao:#1e3a5f;--c-transporte:#17402a;--c-inspecao:#4a3b12;--c-espera:#4a1d1d;--c-armazenagem:#33284f}}
svg.fluxo{font-family:system-ui,sans-serif}
svg.fluxo .fundo{fill:var(--bg)}
svg.fluxo .caixa{stroke:var(--stroke);stroke-width:1.3}
svg.fluxo .s-inicio{fill:var(--c-inicio)}svg.fluxo .s-fim{fill:var(--c-fim)}
svg.fluxo .s-operacao{fill:var(--c-operacao)}svg.fluxo .s-transporte{fill:var(--c-transporte)}
svg.fluxo .s-inspecao{fill:var(--c-inspecao)}svg.fluxo .s-espera{fill:var(--c-espera)}
svg.fluxo .s-armazenagem{fill:var(--c-armazenagem)}
svg.fluxo .simbolo{fill:var(--bg);stroke:var(--stroke);stroke-width:1.5}
svg.fluxo .seta{fill:none;stroke:var(--stroke);stroke-width:1.4}
svg.fluxo .retrabalho{stroke:var(--hot);stroke-dasharray:6 4}
svg.fluxo .ponta{fill:var(--stroke)}
svg.fluxo .ponta-ret{fill:var(--hot)}
svg.fluxo text{fill:var(--fg);font-size:11px}
svg.fluxo .t-titulo{font-size:12.5px;font-weight:700}
svg.fluxo .t-tipo{fill:var(--muted)}
svg.fluxo .t-nota{fill:var(--muted);font-style:italic}
svg.fluxo .t-recurso{fill:var(--accent)}
svg.fluxo .t-estat{fill:var(--hot);font-weight:600}
svg.fluxo .rotulo-fundo{fill:var(--bg);stroke:var(--line);stroke-width:1}
svg.fluxo .rotulo-ret{fill:var(--hot)}
svg.fluxo .painel{fill:var(--card);stroke:var(--line);stroke-width:1}
svg.fluxo .painel-titulo{font-weight:700;font-size:12.5px}
"#;

/// The symbol as a small drawing centred on `(cx, cy)`, `r` being half its size.
pub fn glyph(symbol: Symbol, cx: f64, cy: f64, r: f64) -> String {
    match symbol {
        Symbol::Operacao => {
            format!("<circle class=\"simbolo\" cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"/>")
        }
        Symbol::Inspecao => format!(
            "<rect class=\"simbolo\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            cx - r,
            cy - r,
            2.0 * r,
            2.0 * r
        ),
        Symbol::Transporte => {
            let pts = [
                (cx - r, cy - r * 0.45),
                (cx + r * 0.1, cy - r * 0.45),
                (cx + r * 0.1, cy - r),
                (cx + r, cy),
                (cx + r * 0.1, cy + r),
                (cx + r * 0.1, cy + r * 0.45),
                (cx - r, cy + r * 0.45),
            ];
            polygon(&pts)
        }
        Symbol::Espera => format!(
            "<path class=\"simbolo\" d=\"M{} {} H{} A{r} {r} 0 0 1 {} {} H{} Z\"/>",
            cx - r,
            cy - r,
            cx,
            cx,
            cy + r,
            cx - r
        ),
        Symbol::Armazenagem => polygon(&[
            (cx - r, cy - r * 0.85),
            (cx + r, cy - r * 0.85),
            (cx, cy + r),
        ]),
        Symbol::Inicio | Symbol::Fim => format!(
            "<rect class=\"simbolo\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/>",
            cx - r * 1.3,
            cy - r * 0.65,
            2.6 * r,
            1.3 * r,
            r * 0.65
        ),
    }
}

fn polygon(pts: &[(f64, f64)]) -> String {
    let p: Vec<String> = pts.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect();
    format!("<polygon class=\"simbolo\" points=\"{}\"/>", p.join(" "))
}

/// Complete SVG document (opens by itself in a browser).
pub fn svg(graph: &FlowGraph) -> String {
    render(graph, true)
}

/// The SVG element only, to embed in an HTML page.
pub fn svg_inline(graph: &FlowGraph) -> String {
    render(graph, false)
}

const PANEL_W: f64 = 250.0;

fn render(graph: &FlowGraph, standalone: bool) -> String {
    let panel = !graph.resources.is_empty();
    let panel_h = if panel { panel_height(graph) } else { 0.0 };
    let width = graph.width + if panel { PANEL_W + 30.0 } else { 0.0 };
    let height = graph.height.max(panel_h + PAD);
    let mut s = String::new();
    if standalone {
        s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    }
    let _ = write!(
        s,
        "<svg class=\"fluxo\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" \
         width=\"{w:.0}\" height=\"{h:.0}\" role=\"img\" aria-label=\"Fluxograma de {t}\">",
        w = width,
        h = height,
        t = esc(&graph.name)
    );
    let _ = write!(s, "<title>Fluxograma de {}</title>", esc(&graph.name));
    if standalone {
        let _ = write!(s, "<style>{CSS}</style>");
    }
    s.push_str(
        "<defs><marker id=\"ponta\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" \
         markerHeight=\"8\" orient=\"auto-start-reverse\"><path class=\"ponta\" d=\"M0 0L10 5L0 10z\"/></marker>\
         <marker id=\"ponta-ret\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" \
         markerHeight=\"8\" orient=\"auto-start-reverse\"><path class=\"ponta-ret\" d=\"M0 0L10 5L0 10z\"/></marker></defs>",
    );
    let _ = write!(
        s,
        "<rect class=\"fundo\" width=\"{width:.0}\" height=\"{height:.0}\"/>"
    );
    let mut labels = String::new();
    for e in &graph.edges {
        edge(graph, e, &mut s, &mut labels);
    }
    for node in &graph.nodes {
        draw_node(node, &mut s);
    }
    s.push_str(&labels);
    if panel {
        draw_panel(graph, graph.width + 30.0, &mut s);
    }
    s.push_str("</svg>");
    s
}

fn draw_node(node: &Node, s: &mut String) {
    let class = format!("caixa s-{}", node.symbol.id());
    let terminator = matches!(node.symbol, Symbol::Inicio | Symbol::Fim);
    let rx = if terminator { node.h / 2.0 } else { 6.0 };
    let _ = write!(
        s,
        "<g data-etapa=\"{}\"><rect class=\"{class}\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"{rx:.1}\"/>",
        esc(&node.name),
        node.x,
        node.y,
        node.w,
        node.h
    );
    let text_x = if terminator {
        node.x + 22.0
    } else {
        s.push_str(&glyph(
            node.symbol,
            node.x + GLYPH_AREA / 2.0 + 2.0,
            node.y + 24.0,
            9.0,
        ));
        node.x + GLYPH_AREA + 4.0
    };
    let mut y = node.y + 20.0;
    for (i, (style, text)) in lines(node).iter().enumerate() {
        let class = match style {
            LineStyle::Title => "t-titulo",
            LineStyle::Kind => "t-tipo",
            LineStyle::Detail => "t-detalhe",
            LineStyle::Note => "t-nota",
            LineStyle::Resource => "t-recurso",
            LineStyle::Stat => "t-estat",
        };
        if i == 1 {
            y += 1.0;
        }
        let _ = write!(
            s,
            "<text class=\"{class}\" x=\"{text_x:.1}\" y=\"{y:.1}\">{}</text>",
            esc(text)
        );
        y += if i == 0 { 16.0 } else { 14.0 };
    }
    s.push_str("</g>");
}

/// Bezier control points of an arrow.
fn curve(graph: &FlowGraph, e: &Edge) -> [(f64, f64); 4] {
    let (a, b) = (&graph.nodes[e.from], &graph.nodes[e.to]);
    let vertical = graph.direction == Direction::Vertical;
    if e.from == e.to {
        // Self loop on the right (or bottom) side.
        return if vertical {
            [
                (a.x + a.w, a.y + a.h * 0.3),
                (a.x + a.w + 60.0, a.y + a.h * 0.1),
                (a.x + a.w + 60.0, a.y + a.h * 0.9),
                (a.x + a.w, a.y + a.h * 0.7),
            ]
        } else {
            [
                (a.x + a.w * 0.3, a.y + a.h),
                (a.x + a.w * 0.1, a.y + a.h + 60.0),
                (a.x + a.w * 0.9, a.y + a.h + 60.0),
                (a.x + a.w * 0.7, a.y + a.h),
            ]
        };
    }
    if e.rework {
        let off = 70.0;
        return if vertical {
            let p0 = (a.x + a.w, a.y + a.h / 2.0);
            let p3 = (b.x + b.w, b.y + b.h / 2.0);
            let right = p0.0.max(p3.0) + off;
            [p0, (right, p0.1), (right, p3.1), p3]
        } else {
            let p0 = (a.x + a.w / 2.0, a.y + a.h);
            let p3 = (b.x + b.w / 2.0, b.y + b.h);
            let low = p0.1.max(p3.1) + off;
            [p0, (p0.0, low), (p3.0, low), p3]
        };
    }
    if vertical {
        let p0 = (a.x + a.w / 2.0, a.y + a.h);
        let p3 = (b.x + b.w / 2.0, b.y);
        let k = ((p3.1 - p0.1) / 2.0).max(24.0);
        [p0, (p0.0, p0.1 + k), (p3.0, p3.1 - k), p3]
    } else {
        let p0 = (a.x + a.w, a.y + a.h / 2.0);
        let p3 = (b.x, b.y + b.h / 2.0);
        let k = ((p3.0 - p0.0) / 2.0).max(24.0);
        [p0, (p0.0 + k, p0.1), (p3.0 - k, p3.1), p3]
    }
}

fn edge(graph: &FlowGraph, e: &Edge, s: &mut String, labels: &mut String) {
    let c = curve(graph, e);
    let class = if e.rework { "seta retrabalho" } else { "seta" };
    let marker = if e.rework { "ponta-ret" } else { "ponta" };
    let vertical = graph.direction == Direction::Vertical;
    let (d, mx, my) = if e.via.is_empty() || e.rework || e.from == e.to {
        (
            format!(
                "M{:.1} {:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                c[0].0, c[0].1, c[1].0, c[1].1, c[2].0, c[2].1, c[3].0, c[3].1
            ),
            // Middle of the curve (t = 0.5).
            (c[0].0 + 3.0 * c[1].0 + 3.0 * c[2].0 + c[3].0) / 8.0,
            (c[0].1 + 3.0 * c[1].1 + 3.0 * c[2].1 + c[3].1) / 8.0,
        )
    } else {
        // Through the virtual points of the layers it crosses, one S curve per stretch.
        let mut pts = vec![c[0]];
        pts.extend(e.via.iter().copied());
        pts.push(c[3]);
        let mut d = format!("M{:.1} {:.1}", pts[0].0, pts[0].1);
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if vertical {
                let k = ((b.1 - a.1) / 2.0).max(8.0);
                let _ = write!(
                    d,
                    " C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                    a.0,
                    a.1 + k,
                    b.0,
                    b.1 - k,
                    b.0,
                    b.1
                );
            } else {
                let k = ((b.0 - a.0) / 2.0).max(8.0);
                let _ = write!(
                    d,
                    " C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
                    a.0 + k,
                    a.1,
                    b.0 - k,
                    b.1,
                    b.0,
                    b.1
                );
            }
        }
        let mid = e.via[e.via.len() / 2];
        (d, mid.0, mid.1)
    };
    let _ = write!(
        s,
        "<path class=\"{class}\" marker-end=\"url(#{marker})\" d=\"{d}\"/>"
    );
    let mut rows: Vec<(String, bool, bool)> = Vec::new();
    if e.rework {
        rows.push(("retrabalho".to_string(), false, true));
    }
    if let Some(l) = &e.label {
        rows.push((l.clone(), false, false));
    }
    if let Some(t) = &e.transport {
        rows.push((t.text(), true, false));
    }
    if rows.is_empty() {
        return;
    }
    let widest = rows
        .iter()
        .map(|(t, arrow, _)| text_width(t, 10.0) + if *arrow { 18.0 } else { 0.0 })
        .fold(0.0, f64::max);
    let w = widest + 12.0;
    let h = 15.0 * rows.len() as f64 + 4.0;
    let (x0, y0) = (mx - w / 2.0, my - h / 2.0);
    let _ = write!(
        labels,
        "<rect class=\"rotulo-fundo\" x=\"{x0:.1}\" y=\"{y0:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"4\"/>"
    );
    for (k, (text, arrow, hot)) in rows.iter().enumerate() {
        let ty = y0 + 14.0 + 15.0 * k as f64 - 1.0;
        let mut tx = x0 + 6.0;
        if *arrow {
            labels.push_str(&glyph(Symbol::Transporte, tx + 6.0, ty - 3.5, 5.5));
            tx += 18.0;
        }
        let class = if *hot { " class=\"rotulo-ret\"" } else { "" };
        let _ = write!(
            labels,
            "<text{class} x=\"{tx:.1}\" y=\"{ty:.1}\" font-size=\"10\" style=\"font-size:10px\">{}</text>",
            esc(text)
        );
    }
}

fn resource_lines(graph: &FlowGraph, r: &Resource) -> Vec<String> {
    let mut v = vec![format!("{} · {}", r.kind.nome(), r.detail)];
    if !r.members.is_empty() {
        v.push(format!("membros: {}", r.members.join(", ")));
    }
    if !r.used_by.is_empty() {
        let names: Vec<&str> = r
            .used_by
            .iter()
            .map(|&i| graph.nodes[i].name.as_str())
            .collect();
        v.push(format!("usado em: {}", names.join(", ")));
    }
    v
}

fn panel_rows(graph: &FlowGraph, r: &Resource) -> Vec<String> {
    resource_lines(graph, r)
        .iter()
        .flat_map(|l| wrap(l, 38))
        .collect()
}

fn panel_height(graph: &FlowGraph) -> f64 {
    PAD + 16.0
        + graph
            .resources
            .iter()
            .map(|r| 36.0 + 13.0 * panel_rows(graph, r).len() as f64)
            .sum::<f64>()
}

/// Cuts a text into lines of at most `max` characters, at spaces or commas.
fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_inclusive([' ', ',']) {
        if cur.chars().count() + word.chars().count() > max && !cur.is_empty() {
            out.push(cur.trim_end().to_string());
            cur = String::new();
        }
        cur.push_str(word);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim_end().to_string());
    }
    out
}

fn draw_panel(graph: &FlowGraph, x: f64, s: &mut String) {
    let mut y = PAD;
    let _ = write!(
        s,
        "<text class=\"painel-titulo\" x=\"{:.1}\" y=\"{:.1}\">Recursos</text>",
        x,
        y + 4.0
    );
    y += 16.0;
    for r in &graph.resources {
        let rows = panel_rows(graph, r);
        let h = 26.0 + 13.0 * rows.len() as f64;
        let _ = write!(
            s,
            "<rect class=\"painel\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{PANEL_W}\" height=\"{h:.1}\" rx=\"6\"/>\
             <text class=\"t-titulo\" x=\"{:.1}\" y=\"{:.1}\">{}</text>",
            x + 10.0,
            y + 17.0,
            esc(&r.name)
        );
        for (k, row) in rows.iter().enumerate() {
            let _ = write!(
                s,
                "<text class=\"t-tipo\" x=\"{:.1}\" y=\"{:.1}\">{}</text>",
                x + 10.0,
                y + 32.0 + 13.0 * k as f64,
                esc(row)
            );
        }
        y += h + 10.0;
    }
}
