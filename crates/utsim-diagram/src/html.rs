//! Self-contained HTML page: legend, flowchart (inline SVG), resources and process chart.

use std::fmt::Write as _;

use crate::carta::{self, Carta};
use crate::graph::{FlowGraph, Symbol};
use crate::svg::{CSS, glyph, svg_inline};
use crate::text::esc;

pub(crate) const PAGE_CSS: &str = r#"
body{font:15px/1.5 system-ui,sans-serif;background:var(--bg);color:var(--fg);max-width:1200px;margin:2rem auto;padding:0 1rem}
h1{font-size:1.5rem}h2{font-size:1.15rem;margin-top:2rem}
table{border-collapse:collapse;width:100%;font-size:.85rem}
th,td{padding:.3rem .5rem;border-bottom:1px solid var(--line);text-align:left;vertical-align:top}
th{color:var(--muted);font-weight:600}
td.n,th.n{text-align:right;font-variant-numeric:tabular-nums}
td.sim svg{vertical-align:middle}
tr.total td{font-weight:700;background:var(--card)}
.muted{color:var(--muted)}
.legenda{display:flex;flex-wrap:wrap;gap:.4rem 1.4rem;margin:.5rem 0}
.legenda span{display:inline-flex;align-items:center;gap:.4rem;font-size:.85rem}
.rolagem{overflow:auto;border:1px solid var(--line);border-radius:6px}
.traco{display:inline-block;width:2.2rem;border-top:2px solid var(--stroke)}
.traco.ret{border-top:2px dashed var(--hot)}
@media print{body{max-width:none}.rolagem{overflow:visible;border:none}}
"#;

pub(crate) fn legend() -> String {
    let mut h = String::from("<div class=\"legenda\">");
    for s in Symbol::ALL {
        let _ = write!(
            h,
            "<span><svg class=\"fluxo\" width=\"34\" height=\"24\" viewBox=\"0 0 34 24\" aria-hidden=\"true\">\
             <g class=\"s-{id}\">{g}</g></svg><b>{nome}</b>: {sig}</span>",
            id = s.id(),
            g = glyph(s, 17.0, 12.0, 9.0),
            nome = s.nome(),
            sig = s.significado()
        );
    }
    h.push_str(
        "<span><i class=\"traco\"></i>sequência do processo</span>\
         <span><i class=\"traco ret\"></i><b>retrabalho</b>: o item volta a uma etapa anterior</span>\
         </div>",
    );
    h
}

/// The whole page: flowchart and process chart.
pub fn html(graph: &FlowGraph) -> String {
    let carta = carta::build(graph);
    page(graph, &carta)
}

fn page(graph: &FlowGraph, carta: &Carta) -> String {
    let mut h = String::new();
    let branches = graph.nodes.iter().filter(|n| n.out_degree > 1).count();
    let joins = graph.nodes.iter().filter(|n| n.in_degree > 1).count();
    let loops = graph.edges.iter().filter(|e| e.rework).count();
    let _ = write!(
        h,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Fluxograma: {t}</title><style>{CSS}{PAGE_CSS}</style></head><body>\
         <h1>Fluxograma do processo: {t}</h1>",
        t = esc(&graph.name)
    );
    if !graph.description.trim().is_empty() {
        let _ = write!(h, "<p class=\"muted\">{}</p>", esc(&graph.description));
    }
    let _ = write!(
        h,
        "<p>{} etapas, {} ligações, {} ramificações, {} junções, {} retrabalhos e {} recursos \
         móveis.{}</p>",
        graph.nodes.len(),
        graph.edges.len(),
        branches,
        joins,
        loops,
        graph.resources.len(),
        if graph.has_stats {
            " As caixas trazem números de uma rodada (médias entre as replicações)."
        } else {
            ""
        }
    );
    h.push_str("<h2>Legenda</h2>");
    h.push_str(&legend());
    h.push_str("<h2>Fluxograma</h2><div class=\"rolagem\">");
    h.push_str(&svg_inline(graph));
    h.push_str("</div>");
    h.push_str(
        "<p class=\"muted\">Tempos e distâncias são médias e valores do modelo; o tempo de um \
         transporte feito por empilhadeira ou AGV é uma estimativa que não inclui espera por \
         veículo livre. As porcentagens vêm das regras de saída (pesos, alternado, aleatório).</p>",
    );
    h.push_str("<h2>Carta de processo (caminho principal)</h2>");
    h.push_str(&carta::html_table(carta));
    h.push_str("</body></html>");
    h
}

/// Only the process chart, as a page.
pub fn carta_html(graph: &FlowGraph) -> String {
    let carta = carta::build(graph);
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Carta de processo: {t}</title><style>{CSS}{PAGE_CSS}</style></head><body>\
         <h1>Carta de processo: {t}</h1>{table}</body></html>",
        t = esc(&graph.name),
        table = carta::html_table(&carta)
    )
}
