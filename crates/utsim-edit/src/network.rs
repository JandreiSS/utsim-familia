//! Editing the path network (`Model::network`): nodes, edges and the helpers the
//! editor draws with (edge length, served objects and their nearest node).
//!
//! The commands live in [`crate::Command`] (`AddNode`, `MoveNode`, `RenameNode`,
//! `RemoveNode`, `AddEdge`, `SetEdge`, `RemoveEdge`); this module executes them on a
//! model and refuses the ones that would leave a dangling reference. Whether the network
//! is connected is not checked here: [`utsim_model::Model::validate`] reports it as an
//! issue, so a network can be drawn one edge at a time.

use std::collections::BTreeSet;

use utsim_core::task::distance;
use utsim_model::{Model, ObjectKind};
use utsim_objects::network::{EdgeDef, NetworkDef, NodeDef};

/// Base of the names of new nodes (`no_1`, `no_2`...).
pub const NODE_BASE: &str = "no";

fn node_not_found(name: &str) -> String {
    format!("nó \"{name}\" não existe")
}

fn edge_not_found(from: &str, to: &str) -> String {
    format!("aresta {from} -> {to} não existe")
}

fn has_node(net: &NetworkDef, name: &str) -> bool {
    net.nodes.iter().any(|n| n.name == name)
}

/// Index of the edge stored as `from -> to`.
pub fn edge_index(net: &NetworkDef, from: &str, to: &str) -> Option<usize> {
    net.edges.iter().position(|e| e.from == from && e.to == to)
}

/// Whether `edge` can join `net` (ignoring the edge at `skip`): both ends exist and are
/// different, the capacity is at least 1, and no other edge already covers the same
/// direction (an opposite one-way edge is fine, a two-way one covers both directions).
fn check_edge(net: &NetworkDef, edge: &EdgeDef, skip: Option<usize>) -> Result<(), String> {
    for end in [&edge.from, &edge.to] {
        if !has_node(net, end) {
            return Err(node_not_found(end));
        }
    }
    if edge.from == edge.to {
        return Err("uma aresta precisa ligar dois nós diferentes".into());
    }
    if edge.capacity == Some(0) {
        return Err("a capacidade da aresta deve ser >= 1".into());
    }
    for (i, e) in net.edges.iter().enumerate() {
        if Some(i) == skip {
            continue;
        }
        let same = e.from == edge.from && e.to == edge.to;
        let reverse = e.from == edge.to && e.to == edge.from;
        if same || (reverse && (e.two_way || edge.two_way)) {
            return Err(format!(
                "já existe uma aresta entre {} e {}",
                edge.from, edge.to
            ));
        }
    }
    Ok(())
}

/// The network, created empty on first use.
fn network(model: &mut Model) -> &mut NetworkDef {
    model.network.get_or_insert_with(NetworkDef::default)
}

/// The existing network, or an error.
fn existing(model: &mut Model) -> Result<&mut NetworkDef, String> {
    model
        .network
        .as_mut()
        .ok_or_else(|| "o modelo não tem rede de caminhos".to_string())
}

/// Drops the network once it has nothing left, so the saved file stays as before.
fn tidy(model: &mut Model) {
    if model
        .network
        .as_ref()
        .is_some_and(|n| n.nodes.is_empty() && n.edges.is_empty())
    {
        model.network = None;
    }
}

pub(crate) fn add_node(model: &mut Model, node: &NodeDef) -> Result<(), String> {
    if node.name.trim().is_empty() {
        return Err("o nó precisa de um nome".into());
    }
    let net = network(model);
    if has_node(net, &node.name) {
        return Err(format!("já existe um nó chamado \"{}\"", node.name));
    }
    net.nodes.push(node.clone());
    Ok(())
}

pub(crate) fn move_node(model: &mut Model, name: &str, position: [f64; 3]) -> Result<(), String> {
    let node = existing(model)?
        .nodes
        .iter_mut()
        .find(|n| n.name == name)
        .ok_or_else(|| node_not_found(name))?;
    node.position = position;
    Ok(())
}

pub(crate) fn rename_node(model: &mut Model, from: &str, to: &str) -> Result<(), String> {
    if to.trim().is_empty() {
        return Err("o nome não pode ficar vazio".into());
    }
    let net = existing(model)?;
    if from != to && has_node(net, to) {
        return Err(format!("já existe um nó chamado \"{to}\""));
    }
    let node = net
        .nodes
        .iter_mut()
        .find(|n| n.name == from)
        .ok_or_else(|| node_not_found(from))?;
    node.name = to.to_string();
    for e in &mut net.edges {
        for end in [&mut e.from, &mut e.to] {
            if end == from {
                *end = to.to_string();
            }
        }
    }
    Ok(())
}

pub(crate) fn remove_node(model: &mut Model, name: &str) -> Result<(), String> {
    let net = existing(model)?;
    let before = net.nodes.len();
    net.nodes.retain(|n| n.name != name);
    if net.nodes.len() == before {
        return Err(node_not_found(name));
    }
    net.edges.retain(|e| e.from != name && e.to != name);
    tidy(model);
    Ok(())
}

pub(crate) fn add_edge(model: &mut Model, edge: &EdgeDef) -> Result<(), String> {
    let net = existing(model)?;
    check_edge(net, edge, None)?;
    net.edges.push(edge.clone());
    Ok(())
}

pub(crate) fn set_edge(
    model: &mut Model,
    from: &str,
    to: &str,
    edge: &EdgeDef,
) -> Result<(), String> {
    let net = existing(model)?;
    let i = edge_index(net, from, to).ok_or_else(|| edge_not_found(from, to))?;
    check_edge(net, edge, Some(i))?;
    net.edges[i] = edge.clone();
    Ok(())
}

pub(crate) fn remove_edge(model: &mut Model, from: &str, to: &str) -> Result<(), String> {
    let net = existing(model)?;
    let i = edge_index(net, from, to).ok_or_else(|| edge_not_found(from, to))?;
    net.edges.remove(i);
    tidy(model);
    Ok(())
}

/// Length of the edge `from -> to` (straight line between its nodes).
pub fn edge_length(net: &NetworkDef, from: &str, to: &str) -> Option<f64> {
    let at = |n: &str| net.nodes.iter().find(|x| x.name == n).map(|x| x.position);
    Some(distance(at(from)?, at(to)?))
}

/// The node a point enters and leaves the network by: the nearest one, ties going to
/// the node listed first (the rule of the router).
pub fn nearest_node(net: &NetworkDef, p: [f64; 3]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, n) in net.nodes.iter().enumerate() {
        let d = distance(p, n.position);
        if best.is_none_or(|(_, b)| d < b) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// Objects a mobile resource travels to, in model order: the objects with `transport`
/// and their destinations, the stations of `operators`, and the transporters themselves
/// (they start at their own position).
pub fn served_objects(model: &Model) -> Vec<String> {
    let mut served: BTreeSet<&str> = BTreeSet::new();
    for o in &model.objects {
        if o.transport.is_some() {
            served.insert(&o.name);
            for (_, to) in model.connections.iter().filter(|(f, _)| *f == o.name) {
                served.insert(to);
            }
        }
        match &o.kind {
            ObjectKind::Processor {
                operators: Some(_), ..
            }
            | ObjectKind::Transporter { .. } => {
                served.insert(&o.name);
            }
            _ => {}
        }
    }
    model
        .objects
        .iter()
        .filter(|o| served.contains(o.name.as_str()))
        .map(|o| o.name.clone())
        .collect()
}

/// For each served object, `(object, node)`: where it attaches to the network.
pub fn attachments(model: &Model) -> Vec<(String, String)> {
    let Some(net) = &model.network else {
        return Vec::new();
    };
    served_objects(model)
        .into_iter()
        .filter_map(|name| {
            let o = model.objects.iter().find(|o| o.name == name)?;
            let n = nearest_node(net, o.position)?;
            Some((name, net.nodes[n].name.clone()))
        })
        .collect()
}

/// Warnings about mobile resources that serve nobody: a transporter that is neither
/// named in a `transport`, nor a member of a team, nor an operator team, and a
/// dispatcher nobody uses. Messages in Portuguese.
pub fn idle_resources(model: &Model) -> Vec<String> {
    let mut used: BTreeSet<&str> = BTreeSet::new();
    for o in &model.objects {
        if let Some(t) = &o.transport {
            used.insert(t);
        }
        if let ObjectKind::Processor {
            operators: Some(op),
            ..
        } = &o.kind
        {
            used.insert(&op.team);
        }
    }
    // Members of a team that is used are used too.
    for o in &model.objects {
        if let ObjectKind::Dispatcher { team, .. } = &o.kind
            && used.contains(o.name.as_str())
        {
            used.extend(team.iter().map(String::as_str));
        }
    }
    model
        .objects
        .iter()
        .filter(|o| utsim_model::is_mobile(&o.kind) && !used.contains(o.name.as_str()))
        .map(|o| {
            format!(
                "'{}' não atende nenhum objeto: escolha-o no \"Transporte de saída\" de um \
                 objeto, numa equipe ou nos operadores de um processo",
                o.name
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, from_palette, template};

    fn node(name: &str, x: f64, y: f64) -> Command {
        Command::AddNode(NodeDef {
            name: name.into(),
            position: [x, y, 0.0],
        })
    }

    fn edge(from: &str, to: &str, two_way: bool) -> Command {
        Command::AddEdge(EdgeDef {
            from: from.into(),
            to: to.into(),
            two_way,
            capacity: None,
        })
    }

    fn square() -> Document {
        let mut doc = Document::new("rede");
        for (n, x, y) in [("a", 0.0, 0.0), ("b", 10.0, 0.0), ("c", 10.0, 10.0)] {
            doc.apply(node(n, x, y)).unwrap();
        }
        doc.apply(edge("a", "b", true)).unwrap();
        doc.apply(edge("b", "c", true)).unwrap();
        doc
    }

    fn net(doc: &Document) -> &NetworkDef {
        doc.model().network.as_ref().unwrap()
    }

    #[test]
    fn adding_nodes_creates_the_network_and_undo_removes_it() {
        let mut doc = Document::new("rede");
        assert!(doc.model().network.is_none());
        doc.apply(node("a", 1.0, 2.0)).unwrap();
        assert_eq!(net(&doc).nodes[0].position, [1.0, 2.0, 0.0]);
        assert!(doc.apply(node("a", 5.0, 5.0)).is_err(), "repeated name");
        assert!(doc.apply(node(" ", 5.0, 5.0)).is_err(), "empty name");
        assert_eq!(doc.unique_node_name(NODE_BASE), "no");
        doc.apply(node("no", 3.0, 0.0)).unwrap();
        assert_eq!(doc.unique_node_name(NODE_BASE), "no_2");
        doc.undo();
        doc.undo();
        assert!(doc.model().network.is_none());
        assert!(doc.to_text() == Document::new("rede").to_text());
    }

    #[test]
    fn moving_a_node_changes_the_edge_length_and_coalesces() {
        let mut doc = square();
        assert_eq!(edge_length(net(&doc), "a", "b"), Some(10.0));
        for x in 1..=4 {
            let c = Command::MoveNode {
                name: "a".into(),
                position: [-(x as f64), 0.0, 0.0],
            };
            doc.apply_coalescing(c, Some("no:a")).unwrap();
        }
        assert_eq!(edge_length(net(&doc), "a", "b"), Some(14.0));
        doc.undo();
        assert_eq!(edge_length(net(&doc), "a", "b"), Some(10.0));
        let missing = Command::MoveNode {
            name: "z".into(),
            position: [0.0; 3],
        };
        assert!(doc.apply(missing).is_err());
    }

    #[test]
    fn deleting_a_node_removes_its_edges() {
        let mut doc = square();
        doc.apply(Command::RemoveNode { name: "b".into() }).unwrap();
        assert_eq!(net(&doc).nodes.len(), 2);
        assert!(net(&doc).edges.is_empty());
        doc.undo();
        assert_eq!(net(&doc).edges.len(), 2);
        assert!(doc.apply(Command::RemoveNode { name: "z".into() }).is_err());
    }

    #[test]
    fn edges_must_reference_existing_distinct_nodes_once() {
        let mut doc = square();
        let before = doc.model().clone();
        for bad in [
            edge("a", "z", true),
            edge("a", "a", true),
            edge("a", "b", true),
            edge("b", "a", false),
        ] {
            assert!(doc.apply(bad.clone()).is_err(), "{bad:?}");
        }
        assert!(
            doc.apply(Command::AddEdge(EdgeDef {
                from: "a".into(),
                to: "c".into(),
                two_way: true,
                capacity: Some(0),
            }))
            .is_err()
        );
        assert_eq!(doc.model(), &before, "refused commands leave no trace");
        // Opposite one-way edges are two separate lanes.
        doc.apply(edge("a", "c", false)).unwrap();
        doc.apply(edge("c", "a", false)).unwrap();
        assert_eq!(net(&doc).edges.len(), 4);
        let issues = doc.issues();
        assert!(
            issues.iter().all(|i| !i.message.contains("network")),
            "{issues:?}"
        );
    }

    #[test]
    fn deleting_an_edge_and_validation_of_the_network() {
        let mut doc = square();
        doc.apply(Command::RemoveEdge {
            from: "b".into(),
            to: "c".into(),
        })
        .unwrap();
        // "c" is cut off: the model reports it.
        let issues = doc.issues();
        assert!(
            issues.iter().any(|i| i.message.contains("não alcança")),
            "{issues:?}"
        );
        assert!(
            doc.apply(Command::RemoveEdge {
                from: "b".into(),
                to: "c".into(),
            })
            .is_err()
        );
    }

    #[test]
    fn renaming_a_node_updates_its_edges() {
        let mut doc = square();
        doc.apply(Command::RenameNode {
            from: "b".into(),
            to: "meio".into(),
        })
        .unwrap();
        let n = net(&doc);
        assert_eq!(n.nodes[1].name, "meio");
        assert_eq!(
            (n.edges[0].to.as_str(), n.edges[1].from.as_str()),
            ("meio", "meio")
        );
        for bad in [("meio", "a"), ("meio", ""), ("z", "y")] {
            let c = Command::RenameNode {
                from: bad.0.into(),
                to: bad.1.into(),
            };
            assert!(doc.apply(c).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn edge_direction_and_capacity_change() {
        let mut doc = square();
        let one_way = EdgeDef {
            from: "b".into(),
            to: "a".into(),
            two_way: false,
            capacity: Some(2),
        };
        doc.apply(Command::SetEdge {
            from: "a".into(),
            to: "b".into(),
            edge: one_way.clone(),
        })
        .unwrap();
        assert_eq!(net(&doc).edges[0], one_way);
        assert!(
            doc.to_text().contains("capacity: Some(2)") || doc.to_text().contains("capacity: 2")
        );
        // The edge is now stored as b -> a.
        let back = Command::SetEdge {
            from: "a".into(),
            to: "b".into(),
            edge: one_way.clone(),
        };
        assert!(doc.apply(back).is_err());
        let zero = Command::SetEdge {
            from: "b".into(),
            to: "a".into(),
            edge: EdgeDef {
                capacity: Some(0),
                ..one_way.clone()
            },
        };
        assert!(doc.apply(zero).is_err());
        doc.undo();
        assert!(net(&doc).edges[0].two_way);
        assert_eq!(net(&doc).edges[0].capacity, None);
    }

    #[test]
    fn served_objects_attach_to_the_nearest_node() {
        let mut doc = square();
        let mut fila = template("Queue", "fila", [1.0, 1.0, 0.0]).unwrap();
        fila.transport = Some("emp".into());
        let maq = template("Processor", "maq", [9.0, 9.5, 0.0]).unwrap();
        let solta = template("Processor", "solta", [9.0, 1.0, 0.0]).unwrap();
        let emp = from_palette("Forklift", "emp", [10.0, 0.0, 0.0]).unwrap();
        for o in [fila, maq, solta, emp] {
            doc.apply(Command::Add(Box::new(o))).unwrap();
        }
        doc.apply(Command::Connect {
            from: "fila".into(),
            to: "maq".into(),
        })
        .unwrap();
        assert_eq!(served_objects(doc.model()), vec!["fila", "maq", "emp"]);
        let pairs: Vec<(String, String)> = attachments(doc.model());
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(
            pairs,
            vec![pair("fila", "a"), pair("maq", "c"), pair("emp", "b")]
        );
        // Ties go to the node listed first.
        assert_eq!(nearest_node(net(&doc), [5.0, 0.0, 0.0]), Some(0));
        assert_eq!(nearest_node(&NetworkDef::default(), [0.0; 3]), None);
        assert!(idle_resources(doc.model()).is_empty());
    }

    #[test]
    fn unused_mobile_resources_are_reported() {
        let mut doc = Document::new("t");
        let emp = from_palette("Forklift", "emp", [0.0; 3]).unwrap();
        doc.apply(Command::Add(Box::new(emp))).unwrap();
        let idle = idle_resources(doc.model());
        assert_eq!(idle.len(), 1);
        assert!(idle[0].contains("'emp'"));
        let mut d = template("Dispatcher", "frota", [0.0; 3]).unwrap();
        if let ObjectKind::Dispatcher { team, .. } = &mut d.kind {
            team.push("emp".into());
        }
        doc.apply(Command::Add(Box::new(d))).unwrap();
        let mut q = template("Queue", "q", [0.0; 3]).unwrap();
        q.transport = Some("frota".into());
        doc.apply(Command::Add(Box::new(q))).unwrap();
        assert!(idle_resources(doc.model()).is_empty());
    }
}
