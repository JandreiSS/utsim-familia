//! Editing the fluid network (`Model::fluids`): nodes, links and their parameters.
//!
//! The commands live in [`crate::Command`] (`AddFluidNode`, `MoveFluidNode`,
//! `RenameFluidNode`, `SetFluidKind`, `RemoveFluidNode`, `AddFluidLink`,
//! `RemoveFluidLink`). References that must exist (link ends) are checked here; the
//! rest (acyclic graph, at most one `ToItems` per object, converter objects, positive
//! parameters) is reported live by [`utsim_model::Model::validate`], so a network can
//! be built step by step.

use utsim_fluid::{FluidDef, FluidKind, FluidNode};
use utsim_model::Model;

/// One kind of node offered by the "Fluidos" tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidEntry {
    /// Identifier (the `FluidKind` variant).
    pub id: &'static str,
    /// Portuguese label.
    pub label: &'static str,
    /// Base of the names of new nodes.
    pub name_base: &'static str,
}

/// Node kinds of the fluid tool, in display order.
pub const FLUID_PALETTE: [FluidEntry; 6] = [
    FluidEntry {
        id: "Source",
        label: "Fonte",
        name_base: "fonte",
    },
    FluidEntry {
        id: "Tank",
        label: "Tanque",
        name_base: "tanque",
    },
    FluidEntry {
        id: "Pipe",
        label: "Tubulação",
        name_base: "tubo",
    },
    FluidEntry {
        id: "Sink",
        label: "Sumidouro",
        name_base: "sumidouro",
    },
    FluidEntry {
        id: "FromItems",
        label: "Itens em fluido",
        name_base: "itens_em_fluido",
    },
    FluidEntry {
        id: "ToItems",
        label: "Fluido em itens",
        name_base: "fluido_em_itens",
    },
];

/// The entry with this id.
pub fn fluid_entry(id: &str) -> Option<&'static FluidEntry> {
    FLUID_PALETTE.iter().find(|e| e.id == id)
}

/// Id of the entry of `kind`.
pub fn fluid_kind_id(kind: &FluidKind) -> &'static str {
    match kind {
        FluidKind::Source { .. } => "Source",
        FluidKind::Tank { .. } => "Tank",
        FluidKind::Pipe { .. } => "Pipe",
        FluidKind::Sink { .. } => "Sink",
        FluidKind::FromItems { .. } => "FromItems",
        FluidKind::ToItems { .. } => "ToItems",
    }
}

/// A new node kind with sensible defaults; `object` is the discrete object of a
/// converter (empty when the model has none yet: validation then asks for one).
pub fn fluid_template(id: &str, object: &str) -> Option<FluidKind> {
    Some(match id {
        "Source" => FluidKind::Source { rate: 1.0 },
        "Tank" => FluidKind::Tank {
            capacity: 100.0,
            initial: 0.0,
            max_in: None,
            max_out: None,
        },
        "Pipe" => FluidKind::Pipe { max_rate: 1.0 },
        "Sink" => FluidKind::Sink { max_rate: None },
        "FromItems" => FluidKind::FromItems {
            object: object.to_string(),
            volume: 1.0,
        },
        "ToItems" => FluidKind::ToItems {
            object: object.to_string(),
            volume: 1.0,
            max_rate: None,
        },
        _ => return None,
    })
}

fn not_found(name: &str) -> String {
    format!("nó de fluido \"{name}\" não existe")
}

fn existing(model: &mut Model) -> Result<&mut FluidDef, String> {
    model
        .fluids
        .as_mut()
        .ok_or_else(|| "o modelo não tem rede de fluidos".to_string())
}

fn node_mut<'a>(def: &'a mut FluidDef, name: &str) -> Result<&'a mut FluidNode, String> {
    def.nodes
        .iter_mut()
        .find(|n| n.name == name)
        .ok_or_else(|| not_found(name))
}

/// Drops the network once it has nothing left.
fn tidy(model: &mut Model) {
    if model
        .fluids
        .as_ref()
        .is_some_and(|f| f.nodes.is_empty() && f.links.is_empty())
    {
        model.fluids = None;
    }
}

pub(crate) fn add_node(model: &mut Model, node: &FluidNode) -> Result<(), String> {
    if node.name.trim().is_empty() {
        return Err("o nó precisa de um nome".into());
    }
    let def = model.fluids.get_or_insert_with(|| FluidDef {
        name: "fluidos".into(),
        nodes: Vec::new(),
        links: Vec::new(),
    });
    if def.nodes.iter().any(|n| n.name == node.name) {
        return Err(format!(
            "já existe um nó de fluido chamado \"{}\"",
            node.name
        ));
    }
    def.nodes.push(node.clone());
    Ok(())
}

pub(crate) fn move_node(model: &mut Model, name: &str, position: [f64; 3]) -> Result<(), String> {
    node_mut(existing(model)?, name)?.position = position;
    Ok(())
}

pub(crate) fn rename_node(model: &mut Model, from: &str, to: &str) -> Result<(), String> {
    if to.trim().is_empty() {
        return Err("o nome não pode ficar vazio".into());
    }
    let def = existing(model)?;
    if from != to && def.nodes.iter().any(|n| n.name == to) {
        return Err(format!("já existe um nó de fluido chamado \"{to}\""));
    }
    node_mut(def, from)?.name = to.to_string();
    for (a, b) in &mut def.links {
        for end in [a, b] {
            if end == from {
                *end = to.to_string();
            }
        }
    }
    Ok(())
}

pub(crate) fn set_kind(model: &mut Model, name: &str, kind: &FluidKind) -> Result<(), String> {
    node_mut(existing(model)?, name)?.kind = kind.clone();
    Ok(())
}

pub(crate) fn remove_node(model: &mut Model, name: &str) -> Result<(), String> {
    let def = existing(model)?;
    let before = def.nodes.len();
    def.nodes.retain(|n| n.name != name);
    if def.nodes.len() == before {
        return Err(not_found(name));
    }
    def.links.retain(|(a, b)| a != name && b != name);
    tidy(model);
    Ok(())
}

pub(crate) fn add_link(model: &mut Model, from: &str, to: &str) -> Result<(), String> {
    let def = existing(model)?;
    for end in [from, to] {
        if !def.nodes.iter().any(|n| n.name == end) {
            return Err(not_found(end));
        }
    }
    if from == to {
        return Err("um nó não pode ligar em si mesmo".into());
    }
    if def.links.iter().any(|(a, b)| a == from && b == to) {
        return Err(format!("{from} já está ligado a {to}"));
    }
    def.links.push((from.to_string(), to.to_string()));
    Ok(())
}

pub(crate) fn remove_link(model: &mut Model, from: &str, to: &str) -> Result<(), String> {
    let def = existing(model)?;
    let before = def.links.len();
    def.links.retain(|(a, b)| !(a == from && b == to));
    if def.links.len() == before {
        return Err(format!("{from} não está ligado a {to}"));
    }
    Ok(())
}

/// Converters follow a renamed discrete object.
pub(crate) fn rename_object(model: &mut Model, from: &str, to: &str) {
    let Some(def) = model.fluids.as_mut() else {
        return;
    };
    for n in &mut def.nodes {
        if let FluidKind::FromItems { object, .. } | FluidKind::ToItems { object, .. } = &mut n.kind
            && object == from
        {
            *object = to.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, template};

    fn add(doc: &mut Document, id: &str, name: &str, x: f64) {
        let kind = fluid_template(id, "descarga").unwrap();
        doc.apply(Command::AddFluidNode(FluidNode {
            name: name.into(),
            kind,
            position: [x, 0.0, 0.0],
        }))
        .unwrap();
    }

    fn link(doc: &mut Document, a: &str, b: &str) -> Result<(), String> {
        doc.apply(Command::AddFluidLink {
            from: a.into(),
            to: b.into(),
        })
    }

    /// fonte -> tanque -> tubo -> sumidouro
    fn chain() -> Document {
        let mut doc = Document::new("fluidos");
        for (i, (id, name)) in [
            ("Source", "fonte"),
            ("Tank", "tanque"),
            ("Pipe", "tubo"),
            ("Sink", "sumidouro"),
        ]
        .into_iter()
        .enumerate()
        {
            add(&mut doc, id, name, 3.0 * i as f64);
        }
        for (a, b) in [
            ("fonte", "tanque"),
            ("tanque", "tubo"),
            ("tubo", "sumidouro"),
        ] {
            link(&mut doc, a, b).unwrap();
        }
        doc
    }

    fn def(doc: &Document) -> &FluidDef {
        doc.model().fluids.as_ref().unwrap()
    }

    #[test]
    fn palette_templates_and_ids_agree() {
        for e in FLUID_PALETTE {
            let kind = fluid_template(e.id, "x").unwrap();
            assert_eq!(fluid_kind_id(&kind), e.id);
            assert_eq!(fluid_entry(e.id), Some(&e));
        }
        assert!(fluid_template("Nada", "x").is_none());
    }

    #[test]
    fn a_built_chain_is_valid_runs_and_changes_the_level() {
        let mut doc = chain();
        assert_eq!(doc.issues(), Vec::new());
        doc.apply(Command::SetFluidKind {
            name: "tubo".into(),
            kind: FluidKind::Pipe { max_rate: 0.5 },
        })
        .unwrap();
        doc.apply(Command::Set {
            key: "run.stop_time".into(),
            value: "50.0".into(),
        })
        .unwrap();
        let replay = doc.replay(0).unwrap();
        assert!(replay.fluids.is_some());
        let text = doc.to_text();
        assert_eq!(&Model::parse(&text).unwrap(), doc.model());
        assert!(text.contains("max_rate: 0.5"));
    }

    #[test]
    fn links_need_existing_distinct_nodes_and_cycles_are_reported() {
        let mut doc = chain();
        let before = doc.model().clone();
        assert!(link(&mut doc, "fonte", "nada").is_err());
        assert!(link(&mut doc, "fonte", "fonte").is_err());
        assert!(link(&mut doc, "fonte", "tanque").is_err(), "duplicate");
        assert_eq!(doc.model(), &before);
        link(&mut doc, "sumidouro", "fonte").unwrap();
        let issues = doc.issues();
        assert!(
            issues.iter().any(|i| i.message.contains("ciclo")),
            "{issues:?}"
        );
        doc.apply(Command::RemoveFluidLink {
            from: "sumidouro".into(),
            to: "fonte".into(),
        })
        .unwrap();
        assert!(doc.issues().is_empty());
        assert!(
            doc.apply(Command::RemoveFluidLink {
                from: "sumidouro".into(),
                to: "fonte".into(),
            })
            .is_err()
        );
    }

    #[test]
    fn converters_need_their_object_and_one_to_items_per_object() {
        let mut doc = chain();
        add(&mut doc, "ToItems", "envase", 9.0);
        link(&mut doc, "tubo", "envase").unwrap();
        let issues = doc.issues();
        assert!(
            issues.iter().any(|i| i.message.contains("'descarga'")),
            "{issues:?}"
        );
        let q = template("Queue", "descarga", [0.0, 5.0, 0.0]).unwrap();
        doc.apply(Command::Add(Box::new(q))).unwrap();
        let k = template("Sink", "fim", [3.0, 5.0, 0.0]).unwrap();
        doc.apply(Command::Add(Box::new(k))).unwrap();
        doc.apply(Command::Connect {
            from: "descarga".into(),
            to: "fim".into(),
        })
        .unwrap();
        assert!(doc.issues().is_empty(), "{:?}", doc.issues());
        add(&mut doc, "ToItems", "envase_2", 12.0);
        let issues = doc.issues();
        assert!(
            issues.iter().any(|i| i.message.contains("ToItems")),
            "{issues:?}"
        );
        // Renaming the discrete object keeps the converters pointing at it.
        doc.apply(Command::RemoveFluidNode {
            name: "envase_2".into(),
        })
        .unwrap();
        doc.apply(Command::Rename {
            from: "descarga".into(),
            to: "garrafas".into(),
        })
        .unwrap();
        assert!(doc.issues().is_empty(), "{:?}", doc.issues());
        assert!(matches!(
            &def(&doc).nodes[4].kind,
            FluidKind::ToItems { object, .. } if object == "garrafas"
        ));
    }

    #[test]
    fn move_rename_remove_and_undo() {
        let mut doc = chain();
        doc.apply(Command::MoveFluidNode {
            name: "tanque".into(),
            position: [4.0, 2.0, 0.0],
        })
        .unwrap();
        assert_eq!(def(&doc).nodes[1].position, [4.0, 2.0, 0.0]);
        doc.apply(Command::RenameFluidNode {
            from: "tanque".into(),
            to: "reservatorio".into(),
        })
        .unwrap();
        assert!(def(&doc).links.iter().any(|(a, _)| a == "reservatorio"));
        assert!(
            doc.apply(Command::RenameFluidNode {
                from: "tubo".into(),
                to: "fonte".into(),
            })
            .is_err()
        );
        doc.apply(Command::RemoveFluidNode {
            name: "reservatorio".into(),
        })
        .unwrap();
        assert_eq!(def(&doc).links.len(), 1);
        doc.undo();
        doc.undo();
        assert_eq!(def(&doc).links.len(), 3);
        assert_eq!(def(&doc).nodes[1].name, "tanque");
        assert_eq!(doc.unique_fluid_name("tanque"), "tanque_2");
        assert_eq!(doc.unique_fluid_name("bomba"), "bomba");
        // Removing every node removes the network.
        for n in ["fonte", "tanque", "tubo", "sumidouro"] {
            doc.apply(Command::RemoveFluidNode { name: n.into() })
                .unwrap();
        }
        assert!(doc.model().fluids.is_none());
    }

    #[test]
    fn invalid_parameters_are_reported_not_refused() {
        let mut doc = chain();
        doc.apply(Command::SetFluidKind {
            name: "tanque".into(),
            kind: FluidKind::Tank {
                capacity: 10.0,
                initial: 20.0,
                max_in: None,
                max_out: Some(2.0),
            },
        })
        .unwrap();
        let issues = doc.issues();
        assert!(
            issues.iter().any(|i| i.message.contains("fluidos.tanque")),
            "{issues:?}"
        );
    }
}
