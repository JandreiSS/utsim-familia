//! Path networks (phase 4). Mobile resources travel along the network instead of
//! straight lines when the model defines one.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use utsim_core::Router;
use utsim_core::task::{StraightLine, distance};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeDef {
    pub name: String,
    pub position: [f64; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeDef {
    pub from: String,
    pub to: String,
    /// Travel allowed in both directions.
    #[serde(default = "yes")]
    pub two_way: bool,
    /// Vehicles allowed on this edge at the same time (AGV traffic control);
    /// `None` = unlimited (vehicles pass through each other).
    #[serde(default)]
    pub capacity: Option<usize>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NetworkDef {
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<EdgeDef>,
}

impl NetworkDef {
    /// Names unique, edges between existing nodes, and every node able to reach every
    /// other one (one-way loops are fine); messages in Portuguese.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for n in &self.nodes {
            if !seen.insert(n.name.as_str()) {
                errors.push(format!("network: nó repetido: '{}'", n.name));
            }
        }
        for e in &self.edges {
            for end in [&e.from, &e.to] {
                if !seen.contains(end.as_str()) {
                    errors.push(format!(
                        "network: aresta {} -> {}: nó '{end}' não existe",
                        e.from, e.to
                    ));
                }
            }
        }
        // Only meaningful when the references above are sound.
        if errors.is_empty() {
            let graph = Graph::build(self);
            for (i, node) in self.nodes.iter().enumerate() {
                let reach = graph.reachable_from(i);
                let missing: Vec<&str> = self
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| !reach[*j])
                    .map(|(_, n)| n.name.as_str())
                    .collect();
                if !missing.is_empty() {
                    errors.push(format!(
                        "network: o nó '{}' não alcança: {} (a rede precisa permitir ir de qualquer nó a qualquer outro)",
                        node.name,
                        missing.join(", ")
                    ));
                }
            }
        }
        errors
    }
}

/// Directed graph with Euclidean edge lengths; indices follow `NetworkDef::nodes`.
struct Graph {
    /// `adj[u]` = `(v, length)` in edge-definition order.
    adj: Vec<Vec<(usize, f64)>>,
}

impl Graph {
    fn build(def: &NetworkDef) -> Self {
        let mut index = HashMap::new();
        for (i, n) in def.nodes.iter().enumerate() {
            index.entry(n.name.as_str()).or_insert(i);
        }
        let mut adj = vec![Vec::new(); def.nodes.len()];
        for e in &def.edges {
            let (Some(&a), Some(&b)) = (index.get(e.from.as_str()), index.get(e.to.as_str()))
            else {
                continue;
            };
            let len = distance(def.nodes[a].position, def.nodes[b].position);
            adj[a].push((b, len));
            if e.two_way {
                adj[b].push((a, len));
            }
        }
        Graph { adj }
    }

    fn reachable_from(&self, start: usize) -> Vec<bool> {
        let mut seen = vec![false; self.adj.len()];
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(u) = stack.pop() {
            for &(v, _) in &self.adj[u] {
                if !seen[v] {
                    seen[v] = true;
                    stack.push(v);
                }
            }
        }
        seen
    }
}

/// Shortest-path router over a [`NetworkDef`].
///
/// Snapping policy: a point joins the network through its nearest *node* (straight
/// segment; ties go to the node listed first). Shortest paths between nodes are
/// precomputed from every source with Dijkstra (O(n²) selection). Determinism: the
/// next node to settle is the one with the smallest distance, lowest index on ties,
/// and a predecessor only changes for a strictly shorter distance, so among equally
/// short paths the one found first (by node order) wins.
///
/// If the two attachment nodes are not connected, `route` falls back to a straight
/// line; `NetworkDef::validate` rejects such networks, so this only happens for
/// models that were not validated.
pub struct NetworkRouter {
    positions: Vec<[f64; 3]>,
    /// `prev[s][v]` = node before `v` on the shortest path from `s` (`usize::MAX` = none).
    prev: Vec<Vec<usize>>,
    reachable: Vec<Vec<bool>>,
}

impl NetworkRouter {
    pub fn new(def: &NetworkDef) -> Self {
        let graph = Graph::build(def);
        let n = def.nodes.len();
        let mut prev = Vec::with_capacity(n);
        let mut reachable = Vec::with_capacity(n);
        for s in 0..n {
            let mut dist = vec![f64::INFINITY; n];
            let mut pr = vec![usize::MAX; n];
            let mut done = vec![false; n];
            dist[s] = 0.0;
            loop {
                let mut u = usize::MAX;
                for v in 0..n {
                    if !done[v] && dist[v].is_finite() && (u == usize::MAX || dist[v] < dist[u]) {
                        u = v;
                    }
                }
                if u == usize::MAX {
                    break;
                }
                done[u] = true;
                for &(v, w) in &graph.adj[u] {
                    if dist[u] + w < dist[v] {
                        dist[v] = dist[u] + w;
                        pr[v] = u;
                    }
                }
            }
            reachable.push(dist.iter().map(|d| d.is_finite()).collect());
            prev.push(pr);
        }
        NetworkRouter {
            positions: def.nodes.iter().map(|n| n.position).collect(),
            prev,
            reachable,
        }
    }

    fn nearest(&self, p: [f64; 3]) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (i, &q) in self.positions.iter().enumerate() {
            let d = distance(p, q);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }
}

/// Route over the network as a node sequence (AGV traffic control).
#[derive(Clone, Debug, PartialEq)]
pub struct NodeRoute {
    /// Node indices (into `NetworkDef::nodes`), at least one.
    pub nodes: Vec<usize>,
    /// Length of the edge between `nodes[i]` and `nodes[i + 1]`.
    pub edge_lengths: Vec<f64>,
}

impl NetworkRouter {
    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.positions.len()
    }

    /// Position of node `i`.
    pub fn node_position(&self, i: usize) -> [f64; 3] {
        self.positions[i]
    }

    /// Nodes (and edge lengths) of the shortest route between the nodes nearest to
    /// `from` and `to`; same snapping and tie-breaking as [`Router::route`]. `None`
    /// when the network is empty or the two nodes are not connected.
    pub fn node_route(&self, from: [f64; 3], to: [f64; 3]) -> Option<NodeRoute> {
        let (a, b) = (self.nearest(from)?, self.nearest(to)?);
        if !self.reachable[a][b] {
            return None;
        }
        let mut nodes = vec![b];
        let mut cur = b;
        while cur != a {
            cur = self.prev[a][cur];
            nodes.push(cur);
        }
        nodes.reverse();
        let edge_lengths = nodes
            .windows(2)
            .map(|w| distance(self.positions[w[0]], self.positions[w[1]]))
            .collect();
        Some(NodeRoute {
            nodes,
            edge_lengths,
        })
    }
}

impl Router for NetworkRouter {
    fn route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]> {
        if from == to {
            return vec![from];
        }
        let (Some(a), Some(b)) = (self.nearest(from), self.nearest(to)) else {
            return vec![from, to];
        };
        if !self.reachable[a][b] {
            return vec![from, to];
        }
        let mut nodes = vec![b];
        let mut cur = b;
        while cur != a {
            cur = self.prev[a][cur];
            nodes.push(cur);
        }
        let mut path = vec![from];
        for &i in nodes.iter().rev() {
            let p = self.positions[i];
            if path.last() != Some(&p) {
                path.push(p);
            }
        }
        if path.last() != Some(&to) {
            path.push(to);
        }
        path
    }
}

/// The router for a model's network (straight lines when the model has none).
pub fn router(network: Option<&NetworkDef>) -> Arc<dyn Router> {
    match network {
        Some(def) => Arc::new(NetworkRouter::new(def)),
        None => Arc::new(StraightLine),
    }
}

/// The network router itself (not type-erased), for AGV traffic control.
pub fn network_router(network: Option<&NetworkDef>) -> Option<Arc<NetworkRouter>> {
    network.map(|def| Arc::new(NetworkRouter::new(def)))
}
