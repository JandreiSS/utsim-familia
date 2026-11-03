//! Process chart ("carta de processo"): the sequence of steps along the main path, with
//! symbol, time and distance, as a table (HTML) and as CSV.

use std::fmt::Write as _;

use utsim_model::csv::row as csv_row;

use crate::graph::{Edge, FlowGraph, Symbol};
use crate::svg::glyph;
use crate::text::{esc, num, num_dot};

/// One line of the chart.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Position in the sequence, from 1.
    pub n: usize,
    pub symbol: Symbol,
    pub descricao: String,
    /// Object the step comes from (empty for transport rows between two objects).
    pub objeto: Option<String>,
    pub tempo: Option<f64>,
    pub distancia: Option<f64>,
    pub recursos: String,
    pub observacao: String,
}

/// Counts and sums of the chart.
#[derive(Clone, Debug, PartialEq)]
pub struct Totals {
    /// How many rows of each symbol (only the symbols that occur).
    pub contagem: Vec<(Symbol, usize)>,
    /// Sum of the known times.
    pub tempo: f64,
    /// Sum of the known distances.
    pub distancia: f64,
    /// Rows (not terminators) with no time: the sum is then a lower bound.
    pub sem_tempo: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Carta {
    pub rows: Vec<Row>,
    pub totals: Totals,
}

/// The arrow the main path follows out of a step: the most likely one, otherwise the
/// first port; loops (rework) are never followed.
fn main_edge(graph: &FlowGraph, i: usize) -> Option<&Edge> {
    let out: Vec<&Edge> = graph
        .outgoing(i)
        .into_iter()
        .filter(|e| !e.rework && e.from != e.to)
        .collect();
    let mut best: Option<&Edge> = None;
    for e in &out {
        let better = match best {
            None => true,
            Some(b) => e.probability.unwrap_or(0.0) > b.probability.unwrap_or(0.0) + 1e-12,
        };
        if better {
            best = Some(e);
        }
    }
    best
}

/// Indices of the steps on the main path: from the first entry, following the most
/// likely arrow, until an exit, a dead end or a repeated step.
pub fn main_path(graph: &FlowGraph) -> Vec<usize> {
    let start = graph
        .nodes
        .iter()
        .position(|n| n.symbol == Symbol::Inicio)
        .or_else(|| graph.nodes.iter().position(|n| n.in_degree == 0));
    let Some(mut cur) = start else {
        return Vec::new();
    };
    let mut seen = vec![false; graph.nodes.len()];
    let mut path = Vec::new();
    loop {
        seen[cur] = true;
        path.push(cur);
        match main_edge(graph, cur) {
            Some(e) if !seen[e.to] => cur = e.to,
            _ => break,
        }
    }
    path
}

fn branch_note(graph: &FlowGraph, i: usize) -> Option<String> {
    let out = graph.outgoing(i);
    if out.len() < 2 {
        return None;
    }
    let parts: Vec<String> = out
        .iter()
        .map(|e| {
            let to = &graph.nodes[e.to].name;
            let how = e
                .label
                .as_deref()
                .map(|l| format!("{l} → "))
                .unwrap_or_default();
            if e.rework {
                format!("{how}{to} (retrabalho)")
            } else {
                format!("{how}{to}")
            }
        })
        .collect();
    Some(format!("ramifica: {}", parts.join("; ")))
}

/// Builds the chart of the main path.
pub fn build(graph: &FlowGraph) -> Carta {
    let path = main_path(graph);
    let mut rows: Vec<Row> = Vec::new();
    for (k, &i) in path.iter().enumerate() {
        let node = &graph.nodes[i];
        let tempo = node.time.or_else(|| {
            if node.symbol == Symbol::Espera {
                node.stats.as_ref().and_then(|s| s.mean_stay)
            } else {
                None
            }
        });
        let mut obs: Vec<String> = node
            .details
            .iter()
            .filter(|d| !d.starts_with("tempo"))
            .cloned()
            .collect();
        obs.extend(node.notes.iter().cloned());
        obs.extend(branch_note(graph, i));
        rows.push(Row {
            n: rows.len() + 1,
            symbol: node.symbol,
            descricao: format!("{} ({})", node.name, node.kind.nome()),
            objeto: Some(node.name.clone()),
            tempo: tempo.filter(|_| graph.show_times),
            distancia: None,
            recursos: node.resources.join("; "),
            observacao: obs.join("; "),
        });
        let hop = path.get(k + 1).and_then(|&next| {
            graph
                .outgoing(i)
                .into_iter()
                .find(|e| e.to == next && !e.rework)
                .and_then(|e| e.transport.as_ref().map(|t| (next, t)))
        });
        if let Some((next, t)) = hop {
            rows.push(Row {
                n: rows.len() + 1,
                symbol: Symbol::Transporte,
                descricao: format!("Transporte de {} até {}", node.name, graph.nodes[next].name),
                objeto: None,
                tempo: t.time,
                distancia: t.distance,
                recursos: t.by.clone(),
                observacao: String::new(),
            });
        }
    }
    let mut contagem: Vec<(Symbol, usize)> = Vec::new();
    for s in Symbol::ALL {
        let c = rows.iter().filter(|r| r.symbol == s).count();
        if c > 0 {
            contagem.push((s, c));
        }
    }
    let totals = Totals {
        contagem,
        tempo: rows.iter().filter_map(|r| r.tempo).sum(),
        distancia: rows.iter().filter_map(|r| r.distancia).sum(),
        sem_tempo: rows
            .iter()
            .filter(|r| !matches!(r.symbol, Symbol::Inicio | Symbol::Fim) && r.tempo.is_none())
            .count(),
    };
    Carta { rows, totals }
}

/// The chart as CSV (comma, dot decimals, header in the first line).
pub fn csv(carta: &Carta) -> String {
    let mut out =
        String::from("etapa,simbolo,descricao,objeto,tempo_medio,distancia,recursos,observacao\n");
    for r in &carta.rows {
        csv_row(
            &mut out,
            &[
                r.n.to_string(),
                r.symbol.nome().to_string(),
                r.descricao.clone(),
                r.objeto.clone().unwrap_or_default(),
                r.tempo.map(num_dot).unwrap_or_default(),
                r.distancia.map(num_dot).unwrap_or_default(),
                r.recursos.clone(),
                r.observacao.clone(),
            ],
        );
    }
    out
}

/// The chart as an HTML table plus the summary line (no page around it).
pub fn html_table(carta: &Carta) -> String {
    let mut h = String::new();
    h.push_str(
        "<table class=\"carta\"><thead><tr><th>#</th><th>Símbolo</th><th>Etapa</th>\
         <th class=\"n\">Tempo médio</th><th class=\"n\">Distância</th><th>Recursos</th>\
         <th>Observações</th></tr></thead><tbody>",
    );
    for r in &carta.rows {
        let _ = write!(
            h,
            "<tr><td class=\"n\">{}</td><td class=\"sim\"><svg class=\"fluxo\" width=\"26\" height=\"22\" \
             viewBox=\"0 0 26 22\" aria-hidden=\"true\">{}</svg> {}</td><td>{}</td>\
             <td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}</td><td>{}</td></tr>",
            r.n,
            glyph(r.symbol, 13.0, 11.0, 8.0),
            esc(r.symbol.nome()),
            esc(&r.descricao),
            r.tempo.map(num).unwrap_or_else(|| "-".into()),
            r.distancia.map(num).unwrap_or_else(|| "-".into()),
            esc(&r.recursos),
            esc(&r.observacao),
        );
    }
    let _ = write!(
        h,
        "<tr class=\"total\"><td></td><td colspan=\"2\">Total do caminho principal</td>\
         <td class=\"n\">{}</td><td class=\"n\">{}</td><td colspan=\"2\"></td></tr></tbody></table>",
        num(carta.totals.tempo),
        num(carta.totals.distancia),
    );
    h.push_str("<p class=\"muted\">");
    let counts: Vec<String> = carta
        .totals
        .contagem
        .iter()
        .map(|(s, c)| format!("{}: {c}", s.nome()))
        .collect();
    h.push_str(&esc(&counts.join(" · ")));
    if carta.totals.sem_tempo > 0 {
        let _ = write!(
            h,
            ". {} etapa(s) sem tempo definido no modelo (por exemplo, as esperas): o total de \
             tempo é um mínimo. Gere o fluxograma com resultados para incluir a espera média",
            carta.totals.sem_tempo
        );
    }
    h.push_str(".</p>");
    h
}
