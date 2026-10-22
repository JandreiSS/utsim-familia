//! Path networks (phase 4, contract). Mobile resources travel along the network
//! instead of straight lines when the model defines one.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use utsim_core::Router;
use utsim_core::task::StraightLine;

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
    /// Names unique, edges between existing nodes; messages in Portuguese.
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
        errors
    }
}

/// The router for a model's network (straight lines until the network front lands).
pub fn router(network: Option<&NetworkDef>) -> Arc<dyn Router> {
    let _ = network;
    Arc::new(StraightLine)
}
