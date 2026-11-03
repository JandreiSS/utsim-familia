//! Operation process chart: only operations and inspections (numbered O-1, I-1...), with
//! the material lines entering the assemblies and leaving the separations, in order.
//! Waits, transports and storages are folded into the arrows between the steps.

use std::collections::HashSet;

use crate::graph::{Edge, FlowGraph, Symbol, mark_rework};
use crate::layout;

/// The reduced graph, numbered and laid out.
pub fn grafo(full: &FlowGraph) -> FlowGraph {
    let keep: Vec<bool> = full
        .nodes
        .iter()
        .map(|n| {
            matches!(
                n.symbol,
                Symbol::Operacao | Symbol::Inspecao | Symbol::Inicio | Symbol::Fim
            )
        })
        .collect();
    let mut new_index = vec![usize::MAX; full.nodes.len()];
    let mut nodes = Vec::new();
    for (i, n) in full.nodes.iter().enumerate() {
        if keep[i] {
            new_index[i] = nodes.len();
            let mut c = n.clone();
            c.resources.clear();
            c.in_degree = 0;
            c.out_degree = 0;
            nodes.push(c);
        }
    }
    let mut edges: Vec<Edge> = Vec::new();
    let mut seen_pairs: HashSet<(usize, usize)> = HashSet::new();
    for a in 0..full.nodes.len() {
        if !keep[a] {
            continue;
        }
        // Walk through the folded steps until the next kept one.
        let mut visited: HashSet<usize> = HashSet::new();
        let mut stack: Vec<(usize, Option<String>)> = full
            .outgoing(a)
            .into_iter()
            .filter(|e| !e.rework)
            .map(|e| (e.to, e.label.clone()))
            .collect();
        stack.reverse();
        while let Some((x, label)) = stack.pop() {
            if keep[x] {
                let (na, nb) = (new_index[a], new_index[x]);
                if na != nb && seen_pairs.insert((na, nb)) {
                    edges.push(Edge {
                        from: na,
                        to: nb,
                        out_port: 0,
                        in_port: 0,
                        label,
                        probability: None,
                        transport: None,
                        rework: false,
                        via: Vec::new(),
                    });
                }
            } else if visited.insert(x) {
                let mut next: Vec<(usize, Option<String>)> = full
                    .outgoing(x)
                    .into_iter()
                    .filter(|e| !e.rework)
                    .map(|e| (e.to, e.label.clone().or_else(|| label.clone())))
                    .collect();
                next.reverse();
                stack.extend(next);
            }
        }
    }
    let n = nodes.len();
    let (mut outc, mut inc) = (vec![0usize; n], vec![0usize; n]);
    for e in &mut edges {
        e.out_port = outc[e.from];
        outc[e.from] += 1;
        e.in_port = inc[e.to];
        inc[e.to] += 1;
    }
    mark_rework(&mut edges, n);
    for (i, node) in nodes.iter_mut().enumerate() {
        node.out_degree = outc[i];
        node.in_degree = inc[i];
        node.notes
            .retain(|t| !t.starts_with("ramifica") && !t.starts_with("junção"));
        if inc[i] > 1 {
            node.notes.push(format!("junção de {} entradas", inc[i]));
        }
    }
    let mut g = FlowGraph {
        name: full.name.clone(),
        description: full.description.clone(),
        nodes,
        edges,
        resources: Vec::new(),
        direction: full.direction,
        width: 0.0,
        height: 0.0,
        has_stats: full.has_stats,
        show_times: full.show_times,
    };
    layout::apply(&mut g);
    // Numbering follows the layout order: layer, then position across the flow.
    let mut order: Vec<usize> = (0..g.nodes.len())
        .filter(|&i| matches!(g.nodes[i].symbol, Symbol::Operacao | Symbol::Inspecao))
        .collect();
    order.sort_by(|&a, &b| {
        let (na, nb) = (&g.nodes[a], &g.nodes[b]);
        na.layer
            .cmp(&nb.layer)
            .then(na.x.total_cmp(&nb.x))
            .then(na.y.total_cmp(&nb.y))
    });
    let (mut o, mut ins) = (0, 0);
    for i in order {
        let n = &mut g.nodes[i];
        if n.symbol == Symbol::Operacao {
            o += 1;
            n.name = format!("O-{o} {}", n.name);
        } else {
            ins += 1;
            n.name = format!("I-{ins} {}", n.name);
        }
    }
    layout::apply(&mut g);
    g
}
