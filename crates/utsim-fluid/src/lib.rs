//! Continuous flow inside a discrete-event model (phase 7, contract).
//!
//! A fluid network (tanks, pipes, sources, sinks and converters to/from discrete
//! items) runs as one simulation block. Between events every rate is constant, so
//! tank levels change linearly and the next interesting instant (a tank becoming
//! empty or full, a converter completing a batch) can be computed exactly: the
//! network schedules it, recomputes all rates there, and so on. See
//! `docs/arquitetura.md`, "Fluidos". The fluid front implements [`FluidNet`].

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, BlockId, Ctx, Item, State};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FluidDef {
    /// Name of the network's block (reports, trace).
    #[serde(default = "default_name")]
    pub name: String,
    pub nodes: Vec<FluidNode>,
    /// Directed links `(from, to)` between node names; the graph must be acyclic.
    pub links: Vec<(String, String)>,
}

fn default_name() -> String {
    "fluidos".to_string()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FluidNode {
    pub name: String,
    pub kind: FluidKind,
    #[serde(default)]
    pub position: [f64; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FluidKind {
    /// Supplies up to `rate` per time unit, without limit of volume.
    Source { rate: f64 },
    /// Stores up to `capacity`; optional limits on the inflow and outflow rates.
    Tank {
        capacity: f64,
        #[serde(default)]
        initial: f64,
        #[serde(default)]
        max_in: Option<f64>,
        #[serde(default)]
        max_out: Option<f64>,
    },
    /// Passes fluid through without storing it, up to `max_rate`.
    Pipe { max_rate: f64 },
    /// Absorbs up to `max_rate` (unlimited when `None`).
    Sink {
        #[serde(default)]
        max_rate: Option<f64>,
    },
    /// Every item that enters discrete object `object` (normally a Sink) becomes
    /// `volume` of fluid delivered at once into the downstream tank(s).
    FromItems { object: String, volume: f64 },
    /// Collects fluid (up to `max_rate`) and, every `volume` collected, creates one
    /// item inside discrete object `object`.
    ToItems {
        object: String,
        volume: f64,
        #[serde(default)]
        max_rate: Option<f64>,
    },
}

impl FluidDef {
    /// Structural checks against the model's discrete object names (Portuguese).
    pub fn validate(&self, objects: &HashSet<&str>) -> Vec<String> {
        let mut errors = Vec::new();
        let mut names = HashSet::new();
        for n in &self.nodes {
            if !names.insert(n.name.as_str()) {
                errors.push(format!("fluidos: nó repetido '{}'", n.name));
            }
            let at = format!("fluidos.{}", n.name);
            let positive = |v: f64| v.is_finite() && v > 0.0;
            let bad = match &n.kind {
                FluidKind::Source { rate } => !positive(*rate),
                FluidKind::Tank {
                    capacity,
                    initial,
                    max_in,
                    max_out,
                } => {
                    !positive(*capacity)
                        || !(0.0..=*capacity).contains(initial)
                        || max_in.is_some_and(|r| !positive(r))
                        || max_out.is_some_and(|r| !positive(r))
                }
                FluidKind::Pipe { max_rate } => !positive(*max_rate),
                FluidKind::Sink { max_rate } => max_rate.is_some_and(|r| !positive(r)),
                FluidKind::FromItems { object, volume }
                | FluidKind::ToItems { object, volume, .. } => {
                    if !objects.contains(object.as_str()) {
                        errors.push(format!("{at}: objeto '{object}' não existe"));
                    }
                    !positive(*volume)
                }
            };
            if bad {
                errors.push(format!("{at}: parâmetros inválidos (valores devem ser > 0; nível inicial dentro da capacidade)"));
            }
        }
        for (a, b) in &self.links {
            for end in [a, b] {
                if !names.contains(end.as_str()) {
                    errors.push(format!(
                        "fluidos: ligação ({a} -> {b}): nó '{end}' não existe"
                    ));
                }
            }
        }
        if errors.is_empty() && has_cycle(self) {
            errors
                .push("fluidos: as ligações formam um ciclo (a rede precisa ser acíclica)".into());
        }
        errors
    }
}

fn has_cycle(def: &FluidDef) -> bool {
    let index: HashMap<&str, usize> = def
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_str(), i))
        .collect();
    let mut adj = vec![Vec::new(); def.nodes.len()];
    let mut indeg = vec![0usize; def.nodes.len()];
    for (a, b) in &def.links {
        adj[index[a.as_str()]].push(index[b.as_str()]);
        indeg[index[b.as_str()]] += 1;
    }
    let mut stack: Vec<usize> = (0..def.nodes.len()).filter(|&i| indeg[i] == 0).collect();
    let mut seen = 0;
    while let Some(i) = stack.pop() {
        seen += 1;
        for &j in &adj[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                stack.push(j);
            }
        }
    }
    seen != def.nodes.len()
}

/// The fluid network block. Contract stub — the fluid front implements it.
pub struct FluidNet {
    pub def: FluidDef,
    /// Discrete object name → block id (converters).
    pub objects: HashMap<String, BlockId>,
}

impl FluidNet {
    pub fn new(def: FluidDef, objects: HashMap<String, BlockId>) -> Self {
        FluidNet { def, objects }
    }
}

impl Behavior for FluidNet {
    fn kind(&self) -> &'static str {
        "fluid"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        cx.set_state(State::Idle);
        cx.error("rede de fluidos: em construção na fase 7");
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(links: Vec<(&str, &str)>) -> FluidDef {
        FluidDef {
            name: "f".into(),
            nodes: ["a", "b", "c"]
                .iter()
                .map(|n| FluidNode {
                    name: n.to_string(),
                    kind: FluidKind::Pipe { max_rate: 1.0 },
                    position: [0.0; 3],
                })
                .collect(),
            links: links
                .into_iter()
                .map(|(x, y)| (x.to_string(), y.to_string()))
                .collect(),
        }
    }

    #[test]
    fn cycles_and_bad_references_are_rejected() {
        let objects = HashSet::new();
        assert!(
            net(vec![("a", "b"), ("b", "c")])
                .validate(&objects)
                .is_empty()
        );
        let e = net(vec![("a", "b"), ("b", "c"), ("c", "a")]).validate(&objects);
        assert!(e.iter().any(|m| m.contains("ciclo")), "{e:?}");
        let e = net(vec![("a", "z")]).validate(&objects);
        assert!(e.iter().any(|m| m.contains("'z'")), "{e:?}");
    }
}
