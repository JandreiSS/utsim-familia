//! Layered layout: each step goes to the layer of the longest path that reaches it, and
//! inside a layer the order reduces crossings (barycenter sweeps). Loops (rework arrows)
//! are left out of the layering. The result depends only on the graph.

use crate::graph::{Direction, FlowGraph, Node, Symbol};
use crate::text::{num, pct, text_width};

/// Margin around the drawing.
pub const PAD: f64 = 28.0;
const FONT: f64 = 11.0;
const LINE_H: f64 = 14.0;
const GAP_CROSS: f64 = 44.0;
/// Width of a virtual point of an arrow that crosses a layer.
const DUMMY: f64 = 12.0;
const GAP_MAIN: f64 = 78.0;
const MIN_W: f64 = 176.0;
const MAX_W: f64 = 360.0;
/// Space taken by the symbol drawn at the left of a box.
pub const GLYPH_AREA: f64 = 40.0;

/// How a line of a box is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    Title,
    Kind,
    Detail,
    Note,
    Resource,
    Stat,
}

/// The text lines of a box, top to bottom.
pub fn lines(node: &Node) -> Vec<(LineStyle, String)> {
    let mut v = vec![
        (LineStyle::Title, node.name.clone()),
        (LineStyle::Kind, node.kind.nome().to_string()),
    ];
    v.extend(node.details.iter().map(|d| (LineStyle::Detail, d.clone())));
    v.extend(node.notes.iter().map(|d| (LineStyle::Note, d.clone())));
    v.extend(
        node.resources
            .iter()
            .map(|d| (LineStyle::Resource, d.clone())),
    );
    if let Some(s) = &node.stats {
        if let Some(u) = s.utilization {
            v.push((LineStyle::Stat, format!("ocupação {}", pct(u))));
        }
        match (s.mean_content, s.mean_stay, node.kind) {
            (Some(c), Some(t), crate::graph::NodeKind::Queue) => v.push((
                LineStyle::Stat,
                format!("fila média {} · espera média {}", num(c), num(t)),
            )),
            (c, t, _) => {
                if let Some(c) = c {
                    v.push((LineStyle::Stat, format!("conteúdo médio {}", num(c))));
                }
                if let Some(t) = t {
                    v.push((LineStyle::Stat, format!("permanência média {}", num(t))));
                }
            }
        }
        if let Some(i) = s.items {
            v.push((LineStyle::Stat, format!("itens por replicação: {}", num(i))));
        }
    }
    v
}

fn size_of(node: &Node) -> (f64, f64) {
    let ls = lines(node);
    let widest = ls
        .iter()
        .map(|(style, t)| {
            text_width(
                t,
                if *style == LineStyle::Title {
                    12.5
                } else {
                    FONT
                },
            )
        })
        .fold(0.0, f64::max);
    let terminator = matches!(node.symbol, Symbol::Inicio | Symbol::Fim);
    let extra = if terminator { 36.0 } else { GLYPH_AREA + 16.0 };
    let w = (widest + extra).clamp(MIN_W, MAX_W);
    let h = (22.0 + 16.0 + LINE_H * (ls.len() as f64 - 1.0)).max(54.0);
    (w, if terminator { h + 6.0 } else { h })
}

/// Assigns layers, order, sizes and coordinates; sets `graph.width` and `graph.height`.
pub fn apply(graph: &mut FlowGraph) {
    let n = graph.nodes.len();
    for node in &mut graph.nodes {
        let (w, h) = size_of(node);
        node.w = w;
        node.h = h;
    }
    if n == 0 {
        graph.width = 2.0 * PAD;
        graph.height = 2.0 * PAD;
        return;
    }
    // Arrows that go down the flow; the ones that skip layers get a virtual point in each
    // layer they cross, so that they are routed around the boxes.
    let forward: Vec<(usize, usize, usize)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.rework && e.from != e.to)
        .map(|(k, e)| (k, e.from, e.to))
        .collect();
    let plain: Vec<(usize, usize)> = forward.iter().map(|&(_, a, b)| (a, b)).collect();
    let layer = layers(n, &plain);
    for (node, l) in graph.nodes.iter_mut().zip(&layer) {
        node.layer = *l;
    }
    let mut layer_all = layer.clone();
    let mut chains: Vec<Vec<usize>> = vec![Vec::new(); graph.edges.len()];
    let mut ext: Vec<(usize, usize)> = Vec::new();
    for &(k, a, b) in &forward {
        let mut prev = a;
        if layer[b] > layer[a] + 1 {
            for l in layer[a] + 1..layer[b] {
                let d = layer_all.len();
                layer_all.push(l);
                chains[k].push(d);
                ext.push((prev, d));
                prev = d;
            }
        }
        ext.push((prev, b));
    }
    let total = layer_all.len();
    let n_layers = layer_all.iter().copied().max().map_or(0, |m| m + 1);
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); n_layers];
    for (i, &l) in layer_all.iter().enumerate() {
        rows[l].push(i);
    }
    order(&mut rows, &ext, total);

    let vertical = graph.direction == Direction::Vertical;
    // `main` runs along the flow, `cross` across it; virtual points have no main size.
    let nodes = &graph.nodes;
    let cross_of = |i: usize| match nodes.get(i) {
        Some(n) => {
            if vertical {
                n.w
            } else {
                n.h
            }
        }
        None => DUMMY,
    };
    let main_of = |i: usize| match nodes.get(i) {
        Some(n) => {
            if vertical {
                n.h
            } else {
                n.w
            }
        }
        None => 0.0,
    };
    let row_width = |r: &Vec<usize>| {
        r.iter().map(|&i| cross_of(i)).sum::<f64>() + GAP_CROSS * (r.len().saturating_sub(1)) as f64
    };
    let mut cross_pos = vec![0.0f64; total];
    let widest = rows.iter().map(row_width).fold(0.0, f64::max);
    for r in &rows {
        let mut at = (widest - row_width(r)) / 2.0;
        for &i in r {
            cross_pos[i] = at;
            at += cross_of(i) + GAP_CROSS;
        }
    }
    // Pull each node towards its neighbours of the previous (then the next) layer,
    // keeping the order and the gap.
    let preds = adjacency(total, &ext, true);
    let succs = adjacency(total, &ext, false);
    for pass in 0..2 {
        let layer_ids: Vec<usize> = if pass == 0 {
            (1..n_layers).collect()
        } else {
            (0..n_layers.saturating_sub(1)).rev().collect()
        };
        for l in layer_ids {
            let neighbours = if pass == 0 { &preds } else { &succs };
            let mut prev_end = f64::NEG_INFINITY;
            for &i in &rows[l] {
                let size = cross_of(i);
                let near: Vec<f64> = neighbours[i]
                    .iter()
                    .map(|&j| cross_pos[j] + cross_of(j) / 2.0)
                    .collect();
                let want = if near.is_empty() {
                    cross_pos[i]
                } else {
                    near.iter().sum::<f64>() / near.len() as f64 - size / 2.0
                };
                let at = want.max(prev_end + GAP_CROSS);
                cross_pos[i] = at;
                prev_end = at + size;
            }
        }
    }
    let min_cross = cross_pos.iter().copied().fold(f64::INFINITY, f64::min);
    let max_cross = (0..total)
        .map(|i| cross_pos[i] + cross_of(i))
        .fold(0.0, f64::max);

    let mut main_at = PAD;
    let mut main_pos = vec![0.0f64; total];
    let mut main_mid = vec![0.0f64; total];
    for r in &rows {
        let size = r.iter().map(|&i| main_of(i)).fold(0.0, f64::max);
        for &i in r {
            // Center the boxes of a layer on the same line.
            main_pos[i] = main_at + (size - main_of(i)) / 2.0;
            main_mid[i] = main_at + size / 2.0;
        }
        main_at += size + GAP_MAIN;
    }
    let point = |i: usize| {
        let c = PAD + cross_pos[i] - min_cross + cross_of(i) / 2.0;
        if vertical {
            (c, main_mid[i])
        } else {
            (main_mid[i], c)
        }
    };
    for (k, chain) in chains.iter().enumerate() {
        graph.edges[k].via = chain.iter().map(|&d| point(d)).collect();
    }
    for i in 0..n {
        let c = PAD + cross_pos[i] - min_cross;
        let (x, y) = if vertical {
            (c, main_pos[i])
        } else {
            (main_pos[i], c)
        };
        graph.nodes[i].x = x;
        graph.nodes[i].y = y;
    }
    let cross_total = PAD * 2.0 + max_cross - min_cross;
    let main_total = main_at - GAP_MAIN + PAD;
    (graph.width, graph.height) = if vertical {
        (cross_total, main_total)
    } else {
        (main_total, cross_total)
    };
    // Room for the loops that go around the boxes.
    if graph.edges.iter().any(|e| e.rework) {
        if vertical {
            graph.width += 90.0;
        } else {
            graph.height += 90.0;
        }
    }
}

fn adjacency(n: usize, edges: &[(usize, usize)], preds: bool) -> Vec<Vec<usize>> {
    let mut v = vec![Vec::new(); n];
    for &(a, b) in edges {
        if preds {
            v[b].push(a);
        } else {
            v[a].push(b);
        }
    }
    v
}

/// Longest-path layers of a graph without cycles; blocks with no neighbours get layer 0.
/// A cycle that is left in the input is cut at its first repeated block (it never hangs).
pub fn layers(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut layer = vec![0usize; n];
    let succs = adjacency(n, edges, false);
    let mut indeg = vec![0usize; n];
    for &(_, b) in edges {
        indeg[b] += 1;
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut done = vec![false; n];
    let mut head = 0;
    loop {
        while head < ready.len() {
            let v = ready[head];
            head += 1;
            done[v] = true;
            for &w in &succs[v] {
                layer[w] = layer[w].max(layer[v] + 1);
                if indeg[w] > 0 {
                    indeg[w] -= 1;
                    if indeg[w] == 0 {
                        ready.push(w);
                    }
                }
            }
        }
        // Only a cycle can remain: release the first block still waiting.
        match (0..n).find(|&i| !done[i] && indeg[i] > 0) {
            Some(i) => {
                indeg[i] = 0;
                ready.push(i);
            }
            None => break,
        }
    }
    layer
}

/// Barycenter sweeps over the rows, down then up, a fixed number of times.
fn order(rows: &mut [Vec<usize>], edges: &[(usize, usize)], n: usize) {
    let preds = adjacency(n, edges, true);
    let succs = adjacency(n, edges, false);
    let mut pos = vec![0usize; n];
    let refresh = |rows: &[Vec<usize>], pos: &mut [usize]| {
        for r in rows {
            for (k, &i) in r.iter().enumerate() {
                pos[i] = k;
            }
        }
    };
    refresh(rows, &mut pos);
    for _ in 0..4 {
        for down in [true, false] {
            let idx: Vec<usize> = if down {
                (1..rows.len()).collect()
            } else {
                (0..rows.len().saturating_sub(1)).rev().collect()
            };
            for l in idx {
                let neigh = if down { &preds } else { &succs };
                let mut keyed: Vec<(f64, usize, usize)> = rows[l]
                    .iter()
                    .map(|&i| {
                        let ns = &neigh[i];
                        let key = if ns.is_empty() {
                            pos[i] as f64
                        } else {
                            ns.iter().map(|&j| pos[j] as f64).sum::<f64>() / ns.len() as f64
                        };
                        (key, pos[i], i)
                    })
                    .collect();
                keyed.sort_by(|a, b| {
                    a.0.partial_cmp(&b.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.1.cmp(&b.1))
                });
                rows[l] = keyed.into_iter().map(|(_, _, i)| i).collect();
                refresh(rows, &mut pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_path_decides_the_layer() {
        assert_eq!(
            layers(4, &[(0, 1), (0, 2), (1, 3), (2, 3), (0, 3)]),
            vec![0, 1, 1, 2]
        );
    }

    #[test]
    fn a_cycle_does_not_hang() {
        let l = layers(4, &[(0, 1), (1, 2), (2, 1), (2, 3)]);
        assert_eq!(l.len(), 4);
        assert!(l[0] < l[1]);
    }
}
