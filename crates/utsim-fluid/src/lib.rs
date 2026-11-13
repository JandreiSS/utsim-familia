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
mod net;
pub mod solver;

/// Most items a `ToItems` converter may create per time unit at its `max_rate`.
const MAX_ITEMS_PER_TIME: f64 = 1e6;

pub use net::FluidNet;

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
                    if let FluidKind::ToItems {
                        max_rate: Some(r), ..
                    } = &n.kind
                        && positive(*volume)
                        && r / volume > MAX_ITEMS_PER_TIME
                    {
                        errors.push(format!(
                            "{at}: vazão {r} com volume por item {volume} criaria mais de \
                             {MAX_ITEMS_PER_TIME:.0} itens por unidade de tempo; aumente o volume por item"
                        ));
                    }
                    !positive(*volume)
                }
            };
            if bad {
                errors.push(format!("{at}: parâmetros inválidos (valores devem ser > 0; nível inicial dentro da capacidade)"));
            }
        }
        let mut targets = HashSet::new();
        for n in &self.nodes {
            if let FluidKind::ToItems { object, .. } = &n.kind
                && !targets.insert(object.as_str())
            {
                errors.push(format!(
                    "fluidos.{}: o objeto '{object}' já recebe itens de outro conversor ToItems",
                    n.name
                ));
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

    #[test]
    fn tiny_converter_volume_is_rejected() {
        let objects: HashSet<&str> = ["fila"].into_iter().collect();
        let mut def = net(vec![("a", "b")]);
        def.nodes[1].kind = FluidKind::ToItems {
            object: "fila".into(),
            volume: 1e-9,
            max_rate: Some(2.0),
        };
        let e = def.validate(&objects);
        assert!(
            e.iter().any(|m| m.contains("por unidade de tempo")),
            "{e:?}"
        );
        def.nodes[1].kind = FluidKind::ToItems {
            object: "fila".into(),
            volume: 0.5,
            max_rate: Some(2.0),
        };
        assert!(def.validate(&objects).is_empty());
    }
}
