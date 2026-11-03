//! Cross-functional (swimlane) flowchart: steps inside the lane of whoever does them, and
//! the handovers between lanes highlighted.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::desenho::{curto, svg_doc, texto};
use crate::graph::{FlowGraph, NodeKind, Symbol};
use crate::mermaid::{q, shape};
use crate::svg::glyph;

pub struct Passo {
    pub nome: String,
    pub symbol: Symbol,
    pub raia: usize,
    /// Horizontal position in half layers.
    pub col: f64,
}

pub struct Ligacao {
    pub de: usize,
    pub para: usize,
    pub cruza: bool,
    pub retrabalho: bool,
}

pub struct Raias {
    pub raias: Vec<String>,
    pub passos: Vec<Passo>,
    pub ligacoes: Vec<Ligacao>,
}

/// Name of the lane of a step: the area chosen by the user, the team that works on it, or
/// the kind of resource that does it.
fn raia_de(graph: &FlowGraph, i: usize, areas: &BTreeMap<String, String>) -> String {
    let n = &graph.nodes[i];
    if let Some(a) = areas.get(&n.name) {
        return a.clone();
    }
    if let Some(r) = n
        .resources
        .iter()
        .find_map(|r| r.strip_prefix("operadores: "))
    {
        let team = r.split(' ').next().unwrap_or(r);
        return format!("Equipe {team}");
    }
    match n.kind {
        NodeKind::Source | NodeKind::Sink => "Fornecedor e cliente",
        NodeKind::Queue | NodeKind::Rack => "Filas e armazenagem",
        NodeKind::Conveyor => "Esteiras",
        _ => "Estações",
    }
    .to_string()
}

pub fn montar(graph: &FlowGraph, areas: &BTreeMap<String, String>) -> Raias {
    let mut raias: Vec<String> = Vec::new();
    let lane_id = |name: String, raias: &mut Vec<String>| {
        raias.iter().position(|r| *r == name).unwrap_or_else(|| {
            raias.push(name);
            raias.len() - 1
        })
    };
    let mut passos: Vec<Passo> = Vec::new();
    for i in 0..graph.nodes.len() {
        let r = lane_id(raia_de(graph, i, areas), &mut raias);
        passos.push(Passo {
            nome: graph.nodes[i].name.clone(),
            symbol: graph.nodes[i].symbol,
            raia: r,
            col: graph.nodes[i].layer as f64 * 2.0,
        });
    }
    let mut ligacoes = Vec::new();
    for e in &graph.edges {
        match (&e.transport, e.rework) {
            (Some(t), false) => {
                let lane = if t.by == "transferência" {
                    "Transferências".to_string()
                } else {
                    format!("Frota {}", t.by)
                };
                let r = lane_id(lane, &mut raias);
                let v = passos.len();
                let col = (passos[e.from].col + passos[e.to].col) / 2.0;
                passos.push(Passo {
                    nome: "Transporte".into(),
                    symbol: Symbol::Transporte,
                    raia: r,
                    col,
                });
                for (a, b) in [(e.from, v), (v, e.to)] {
                    ligacoes.push(Ligacao {
                        de: a,
                        para: b,
                        cruza: passos[a].raia != passos[b].raia,
                        retrabalho: false,
                    });
                }
            }
            _ => ligacoes.push(Ligacao {
                de: e.from,
                para: e.to,
                cruza: passos[e.from].raia != passos[e.to].raia,
                retrabalho: e.rework,
            }),
        }
    }
    Raias {
        raias,
        passos,
        ligacoes,
    }
}

impl Raias {
    /// Handovers between lanes: `(from lane, to lane, count)`.
    pub fn passagens(&self) -> Vec<(String, String, usize)> {
        let mut m: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for l in self.ligacoes.iter().filter(|l| l.cruza) {
            *m.entry((self.passos[l.de].raia, self.passos[l.para].raia))
                .or_default() += 1;
        }
        m.into_iter()
            .map(|((a, b), c)| (self.raias[a].clone(), self.raias[b].clone(), c))
            .collect()
    }
}

const LABEL_W: f64 = 150.0;
const NODE_W: f64 = 150.0;
const NODE_H: f64 = 40.0;
const HALF_COL: f64 = 100.0;

pub fn svg(r: &Raias, titulo: &str, standalone: bool) -> String {
    // Stack of boxes that share lane and column.
    let key = |p: &Passo| (p.raia, (p.col * 2.0).round() as i64);
    let mut slots: BTreeMap<(usize, i64), usize> = BTreeMap::new();
    let mut slot_of = Vec::new();
    for p in &r.passos {
        let c = slots.entry(key(p)).or_default();
        slot_of.push(*c);
        *c += 1;
    }
    let mut band_h = vec![0usize; r.raias.len()];
    for (&(lane, _), &c) in &slots {
        band_h[lane] = band_h[lane].max(c);
    }
    let mut band_y = Vec::new();
    let mut y = 20.0;
    for &c in &band_h {
        band_y.push(y);
        y += c.max(1) as f64 * (NODE_H + 14.0) + 16.0;
    }
    let max_col = r.passos.iter().map(|p| p.col).fold(0.0, f64::max);
    let width = LABEL_W + 60.0 + max_col * HALF_COL + NODE_W;
    let height = y + 10.0;
    let pos = |i: usize| {
        let p = &r.passos[i];
        (
            LABEL_W + 30.0 + p.col * HALF_COL,
            band_y[p.raia] + 10.0 + slot_of[i] as f64 * (NODE_H + 14.0),
        )
    };
    let mut b = String::new();
    for (k, name) in r.raias.iter().enumerate() {
        let h = band_h[k].max(1) as f64 * (NODE_H + 14.0) + 16.0;
        let _ = write!(
            b,
            "<rect class=\"raia\" x=\"4\" y=\"{:.1}\" width=\"{:.1}\" height=\"{h:.1}\"/>",
            band_y[k],
            width - 8.0
        );
        let _ = write!(
            b,
            "<line class=\"eixo\" x1=\"{x}\" y1=\"{:.1}\" x2=\"{x}\" y2=\"{:.1}\"/>",
            band_y[k],
            band_y[k] + h,
            x = LABEL_W
        );
        b.push_str(&texto(
            12.0,
            band_y[k] + h / 2.0 + 4.0,
            "b",
            &curto(name, 20),
        ));
    }
    for l in &r.ligacoes {
        let ((xa, ya), (xb, yb)) = (pos(l.de), pos(l.para));
        let (p0, p3, c1, c2);
        if xb >= xa + NODE_W - 1.0 {
            p0 = (xa + NODE_W, ya + NODE_H / 2.0);
            p3 = (xb, yb + NODE_H / 2.0);
            let k = ((p3.0 - p0.0) / 2.0).max(16.0);
            c1 = (p0.0 + k, p0.1);
            c2 = (p3.0 - k, p3.1);
        } else {
            let down = yb >= ya;
            p0 = (xa + NODE_W / 2.0, if down { ya + NODE_H } else { ya });
            p3 = (xb + NODE_W / 2.0, if down { yb } else { yb + NODE_H });
            let k = 20.0 * if down { 1.0 } else { -1.0 };
            c1 = (p0.0 + 30.0, p0.1 + k);
            c2 = (p3.0 + 30.0, p3.1 - k);
        }
        let class = if l.cruza { "cruza" } else { "fino" };
        let marker = if l.cruza { "ponta-ret" } else { "ponta" };
        let dash = if l.retrabalho {
            " style=\"stroke-dasharray:6 4\""
        } else {
            ""
        };
        let _ = write!(
            b,
            "<path class=\"{class}\"{dash} marker-end=\"url(#{marker})\" d=\"M{:.1} {:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}\"/>",
            p0.0, p0.1, c1.0, c1.1, c2.0, c2.1, p3.0, p3.1
        );
    }
    for (i, p) in r.passos.iter().enumerate() {
        let (x, y) = pos(i);
        let _ = write!(
            b,
            "<rect class=\"caixa s-{}\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{NODE_W}\" height=\"{NODE_H}\" rx=\"6\"/>",
            p.symbol.id()
        );
        b.push_str(&glyph(p.symbol, x + 18.0, y + NODE_H / 2.0, 8.0));
        b.push_str(&texto(x + 34.0, y + 24.0, "", &curto(&p.nome, 18)));
    }
    svg_doc(width, height, titulo, &b, standalone)
}

pub fn mermaid(r: &Raias) -> String {
    let mut s = String::from("flowchart LR\n");
    for (k, lane) in r.raias.iter().enumerate() {
        let _ = writeln!(s, "    subgraph L{k}[\"{}\"]", q(lane));
        for (i, p) in r.passos.iter().enumerate().filter(|(_, p)| p.raia == k) {
            let label = format!("\"{}\"", q(&p.nome));
            let _ = writeln!(s, "        r{i}{}", shape(p.symbol, &label));
        }
        s.push_str("    end\n");
    }
    for l in &r.ligacoes {
        let arrow = match (l.cruza, l.retrabalho) {
            (_, true) => "-.->",
            (true, _) => "==>",
            _ => "-->",
        };
        let _ = writeln!(s, "    r{} {arrow} r{}", l.de, l.para);
    }
    s
}
