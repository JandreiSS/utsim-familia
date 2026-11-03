//! The editable model behind the editor (phase 3), independent of any GUI.
//!
//! Every change goes through a [`Command`]; the [`Document`] keeps the undo/redo
//! history, reports validation issues per object while the model is still
//! incomplete, and saves it as stable, diff-friendly RON text.
//!
//! [`arrange`] holds the pure helpers of the editor for several objects at once:
//! clipboard (copy, cut, paste, duplicate), moving and deleting a selection, align and
//! distribute, and fanning a property edit out to every selected object. Each one
//! returns a single [`Command::Batch`], so it is one undo step.
//!
//! [`network`] and [`fluids`] execute the commands on the path network and on the
//! fluid network, and hold the helpers the editor draws them with.

pub mod aisles;
pub mod arrange;
pub mod fluids;
pub mod network;
pub mod ron_text;

use std::path::{Path, PathBuf};

use utsim_core::Dist;
pub use utsim_fluid::{FluidDef, FluidKind, FluidNode};
use utsim_model::dados::{self, DataInput};
use utsim_model::experiment::patch;
use utsim_model::replay::Replay;
use utsim_model::{
    Aisle, FloorMarking, Model, ModelError, Movement, ObjectDef, ObjectKind, VisualDef,
};
pub use utsim_objects::network::{EdgeDef, NetworkDef, NodeDef};
use utsim_objects::{CombineMode, SeparateMode};

/// One user action. Commands are data, so the GUI, tests and scripts drive the
/// document the same way.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Adds an object; the name must be unique (see [`Document::unique_name`]).
    Add(Box<ObjectDef>),
    /// Removes an object and every connection touching it.
    Remove {
        name: String,
    },
    /// Renames an object, updating the connections.
    Rename {
        from: String,
        to: String,
    },
    Move {
        name: String,
        position: [f64; 3],
    },
    /// Sets a field with RON text, addressed as `<object>.<field>` or `run.<field>`
    /// (the same keys as scenario files).
    Set {
        key: String,
        value: String,
    },
    Connect {
        from: String,
        to: String,
    },
    Disconnect {
        from: String,
        to: String,
    },
    /// Swaps the places of two existing connections in the connection list. The order of
    /// the list gives the output ports of the source and the pull priority of the
    /// destination, so two neighbours of one object trade ports.
    SwapConnections {
        first: (String, String),
        second: (String, String),
    },
    /// Replaces the look of an object (`visual`, presentation only). `None`, or a
    /// `VisualDef` with every field empty, restores the default look of its kind.
    SetVisual {
        name: String,
        visual: Option<VisualDef>,
    },
    /// Adds a node to the path network (created on first use).
    AddNode(NodeDef),
    MoveNode {
        name: String,
        position: [f64; 3],
    },
    /// Renames a node, updating its edges.
    RenameNode {
        from: String,
        to: String,
    },
    /// Removes a node and every edge touching it (the network goes when it is empty).
    RemoveNode {
        name: String,
    },
    /// Adds an edge between two existing, different nodes.
    AddEdge(EdgeDef),
    /// Replaces the edge stored as `from -> to` (direction, ends or capacity).
    SetEdge {
        from: String,
        to: String,
        edge: EdgeDef,
    },
    RemoveEdge {
        from: String,
        to: String,
    },
    /// Adds a circulation aisle (a new, unique name).
    AddAisle(Box<Aisle>),
    /// Replaces the aisle `name`: vertices, width, kind, direction, profile or its name.
    SetAisle {
        name: String,
        aisle: Box<Aisle>,
    },
    RemoveAisle {
        name: String,
    },
    /// Sets (`None`: removes) the access point of an object, in its own coordinates.
    SetAccess {
        name: String,
        access: Option<[f64; 2]>,
    },
    /// Own movement mode of a mobile resource (`None`: the model's).
    SetMovement {
        name: String,
        movement: Option<Movement>,
    },
    /// Own clearance of a mobile resource (`None`: the one of its role).
    SetClearance {
        name: String,
        clearance: Option<f64>,
    },
    /// The movement mode of the model, `run.movement` (`None`: `Free`).
    SetRunMovement(Option<Movement>),
    /// The floor marking of the model (`None`: the default, Brasil).
    SetFloorMarking(Option<Box<FloorMarking>>),
    /// Adds a node to the fluid network (created on first use).
    AddFluidNode(FluidNode),
    MoveFluidNode {
        name: String,
        position: [f64; 3],
    },
    /// Renames a fluid node, updating its links.
    RenameFluidNode {
        from: String,
        to: String,
    },
    /// Replaces the kind and parameters of a fluid node.
    SetFluidKind {
        name: String,
        kind: FluidKind,
    },
    /// Removes a fluid node and its links (the network goes when it is empty).
    RemoveFluidNode {
        name: String,
    },
    AddFluidLink {
        from: String,
        to: String,
    },
    RemoveFluidLink {
        from: String,
        to: String,
    },
    /// Replaces the model's name and description (the header of the file).
    SetInfo {
        name: String,
        description: String,
    },
    /// Switches a distribution field (`<object>.<field>`, see
    /// [`utsim_model::dados::dist_fields`]) between a fixed distribution and real data:
    /// with [`FieldSource::Data`] the values are fitted at once (the command fails, and
    /// nothing changes, when they cannot be read or fitted); with [`FieldSource::Dist`]
    /// the data entry goes and the field keeps the given distribution.
    SetFieldSource {
        field: String,
        source: FieldSource,
    },
    /// Several commands undone as one step.
    Batch(Vec<Command>),
}

/// Where a distribution field gets its distribution from.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldSource {
    /// A distribution written in the model, as before.
    Dist(Dist),
    /// Observed values, fitted automatically (the entry's `field` is replaced).
    Data(Box<DataInput>),
}

/// A validation problem, attached to an object when it concerns one.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub object: Option<String>,
    pub message: String,
}

pub struct Document {
    model: Model,
    path: Option<PathBuf>,
    /// Comment lines found before `Model(` in the file, kept on save.
    header: String,
    undo: Vec<(Model, Option<String>)>,
    redo: Vec<Model>,
    revision: u64,
    saved_revision: u64,
}

impl Document {
    pub fn new(name: &str) -> Self {
        let model = Model::parse(&format!(
            "Model(format: {}, name: {name:?}, run: (stop_time: 10000.0), objects: [], connections: [])",
            utsim_model::FORMAT_VERSION
        ))
        .expect("empty model parses");
        Document::from_model(model, None, String::new())
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, ModelError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        let model = Model::parse_in(&text, path.parent())?;
        Ok(Document::from_model(
            model,
            Some(path.to_path_buf()),
            leading_comments(&text),
        ))
    }

    fn from_model(mut model: Model, path: Option<PathBuf>, header: String) -> Self {
        // Saving always writes the current format version.
        model.format = Some(utsim_model::FORMAT_VERSION);
        Document {
            model,
            path,
            header,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            saved_revision: 0,
        }
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Counts every change of the model (apply, undo, redo): equal revisions of the same
    /// document mean the same model, so derived views can be cached on it.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether there are changes since the last save (or open).
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Applies a command as one undo step. On error nothing changes.
    pub fn apply(&mut self, command: Command) -> Result<(), String> {
        self.apply_coalescing(command, None)
    }

    /// Like [`Document::apply`], but consecutive commands with the same `key`
    /// (e.g. `"mover:torno"` while dragging) merge into a single undo step.
    pub fn apply_coalescing(&mut self, command: Command, key: Option<&str>) -> Result<(), String> {
        let mut next = self.model.clone();
        execute(&mut next, &command)?;
        // Data entries added or changed by the command are fitted now.
        dados::resolve_pending(&mut next, self.path.as_deref().and_then(Path::parent))
            .map_err(|e| e.to_string())?;
        let merge = key.is_some() && self.undo.last().is_some_and(|(_, k)| k.as_deref() == key);
        if merge {
            self.model = next;
        } else {
            let previous = std::mem::replace(&mut self.model, next);
            self.undo.push((previous, key.map(str::to_string)));
        }
        self.redo.clear();
        self.revision += 1;
        Ok(())
    }

    /// Ends the current coalescing gesture, so the next command starts a new step.
    pub fn end_gesture(&mut self) {
        if let Some((_, key)) = self.undo.last_mut() {
            *key = None;
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some((previous, _)) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.model, previous));
        self.revision += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo
            .push((std::mem::replace(&mut self.model, next), None));
        self.revision += 1;
        true
    }

    /// Current validation issues; an empty list means the model can run.
    pub fn issues(&self) -> Vec<Issue> {
        let Err(ModelError::Invalid(messages)) = self.model.validate() else {
            return Vec::new();
        };
        messages
            .into_iter()
            .map(|message| Issue {
                object: self.object_of(&message),
                message,
            })
            .collect()
    }

    fn object_of(&self, message: &str) -> Option<String> {
        // `name:` or `name.` at the start, `'name'` or `(name ->` anywhere. Models have
        // hundreds of objects and messages, so no allocation per pair.
        let quoted = |name: &str| {
            message.match_indices('\'').any(|(i, _)| {
                message[i + 1..]
                    .strip_prefix(name)
                    .is_some_and(|r| r.starts_with('\''))
            })
        };
        let arrow = |name: &str| {
            message.match_indices('(').any(|(i, _)| {
                message[i + 1..]
                    .strip_prefix(name)
                    .is_some_and(|r| r.starts_with(" ->"))
            })
        };
        self.model
            .objects
            .iter()
            .map(|o| o.name.as_str())
            .filter(|name| {
                message
                    .strip_prefix(name)
                    .is_some_and(|r| r.starts_with(':') || r.starts_with('.'))
                    || quoted(name)
                    || arrow(name)
            })
            .max_by_key(|name| name.len())
            .map(str::to_string)
    }

    /// Warnings about a model that runs but is probably not what the author meant
    /// ([`Model::warnings`], plus mobile resources that serve nobody). Only computed
    /// when there are no [`Document::issues`], as `Model::warnings` expects.
    pub fn warnings(&self) -> Vec<Issue> {
        if self.model.validate().is_err() {
            return Vec::new();
        }
        self.model
            .warnings()
            .into_iter()
            .chain(network::idle_resources(&self.model))
            .map(|message| Issue {
                object: self.object_of(&message),
                message,
            })
            .collect()
    }

    /// Full analysis of the data of `field` (ranking, choice, warnings and the HTML/CSV
    /// report), for the interface. `Err` when the field has no data or they cannot be read.
    pub fn data_analysis(&self, field: &str) -> Result<dados::auto::AutoFit, String> {
        let input = self
            .model
            .data
            .iter()
            .find(|i| i.field == field)
            .ok_or_else(|| format!("'{field}' não usa dados reais"))?;
        input.analyze(self.path.as_deref().and_then(Path::parent))
    }

    /// `base`, or `base_2`, `base_3`... whichever is free.
    pub fn unique_name(&self, base: &str) -> String {
        free_name(base, |n| self.model.objects.iter().any(|o| o.name == n))
    }

    /// A free name for a new node of the path network.
    pub fn unique_node_name(&self, base: &str) -> String {
        let net = self.model.network.as_ref();
        free_name(base, |n| {
            net.is_some_and(|x| x.nodes.iter().any(|o| o.name == n))
        })
    }

    /// A free name for a new aisle (`corredor_1`, `corredor_2`...).
    pub fn unique_aisle_name(&self) -> String {
        (1..)
            .map(|k| format!("{}_{k}", aisles::AISLE_BASE))
            .find(|n| !self.model.aisles.iter().any(|a| &a.name == n))
            .expect("an unbounded range has a free name")
    }

    /// A free name for a new node of the fluid network.
    pub fn unique_fluid_name(&self, base: &str) -> String {
        let def = self.model.fluids.as_ref();
        free_name(base, |n| {
            def.is_some_and(|x| x.nodes.iter().any(|o| o.name == n))
        })
    }

    /// Stable RON text: same model, same bytes. Comments inside the model are not
    /// kept (only the header comments before `Model(`).
    pub fn to_text(&self) -> String {
        let config = ron::ser::PrettyConfig::new()
            .struct_names(true)
            .indentor("    ".to_string())
            .extensions(ron::extensions::Extensions::IMPLICIT_SOME);
        let body = ron::ser::to_string_pretty(&self.model, config).expect("model serializes");
        format!("{}{body}\n", self.header)
    }

    pub fn save(&mut self) -> Result<(), ModelError> {
        let path = self.path.clone().ok_or_else(|| {
            ModelError::Invalid(vec![
                "o documento ainda não tem arquivo: use salvar como".into(),
            ])
        })?;
        self.save_as(path)
    }

    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), ModelError> {
        std::fs::write(path.as_ref(), self.to_text())?;
        self.path = Some(path.as_ref().to_path_buf());
        self.saved_revision = self.revision;
        Ok(())
    }

    /// Runs one replication for the 3D view. Fails with the issues if the model is invalid.
    pub fn replay(&self, replication: u32) -> Result<Replay, ModelError> {
        self.model.replay(replication)
    }
}

fn free_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base}_{i}"))
        .find(|n| !taken(n))
        .expect("some suffix is free")
}

fn not_found(name: &str) -> String {
    format!("objeto \"{name}\" não existe")
}

fn execute(model: &mut Model, command: &Command) -> Result<(), String> {
    let exists = |model: &Model, name: &str| model.objects.iter().any(|o| o.name == name);
    match command {
        Command::Add(object) => {
            if object.name.trim().is_empty() {
                return Err("o objeto precisa de um nome".into());
            }
            if exists(model, &object.name) {
                return Err(format!("já existe um objeto chamado \"{}\"", object.name));
            }
            model.objects.push((**object).clone());
        }
        Command::Remove { name } => {
            let before = model.objects.len();
            model.objects.retain(|o| &o.name != name);
            if model.objects.len() == before {
                return Err(not_found(name));
            }
            model.connections.retain(|(a, b)| a != name && b != name);
            dados::remove_object(model, name);
            // A time measurement needs both of its ends.
            model.spans.retain(|m| &m.from != name && &m.to != name);
        }
        Command::Rename { from, to } => {
            if to.trim().is_empty() {
                return Err("o nome não pode ficar vazio".into());
            }
            if from != to && exists(model, to) {
                return Err(format!("já existe um objeto chamado \"{to}\""));
            }
            let object = model
                .objects
                .iter_mut()
                .find(|o| &o.name == from)
                .ok_or_else(|| not_found(from))?;
            object.name = to.clone();
            for (a, b) in &mut model.connections {
                for end in [a, b] {
                    if end == from {
                        *end = to.clone();
                    }
                }
            }
            fluids::rename_object(model, from, to);
            dados::rename_object(model, from, to);
            for span in &mut model.spans {
                for end in [&mut span.from, &mut span.to] {
                    if end == from {
                        *end = to.clone();
                    }
                }
            }
        }
        Command::Move { name, position } => {
            let object = model
                .objects
                .iter_mut()
                .find(|o| &o.name == name)
                .ok_or_else(|| not_found(name))?;
            object.position = *position;
        }
        Command::Set { key, value } => patch(model, key, value).map_err(|e| e.to_string())?,
        Command::Connect { from, to } => {
            for end in [from, to] {
                if !exists(model, end) {
                    return Err(not_found(end));
                }
            }
            if from == to {
                return Err("um objeto não pode ligar em si mesmo".into());
            }
            if model.connections.iter().any(|(a, b)| a == from && b == to) {
                return Err(format!("{from} já está ligado a {to}"));
            }
            model.connections.push((from.clone(), to.clone()));
        }
        Command::Disconnect { from, to } => {
            let before = model.connections.len();
            model.connections.retain(|(a, b)| !(a == from && b == to));
            if model.connections.len() == before {
                return Err(format!("{from} não está ligado a {to}"));
            }
        }
        Command::SwapConnections { first, second } => {
            let find = |model: &Model, c: &(String, String)| {
                model
                    .connections
                    .iter()
                    .position(|(a, b)| *a == c.0 && *b == c.1)
                    .ok_or_else(|| format!("{} não está ligado a {}", c.0, c.1))
            };
            let (i, j) = (find(model, first)?, find(model, second)?);
            model.connections.swap(i, j);
        }
        Command::SetVisual { name, visual } => {
            let object = model
                .objects
                .iter_mut()
                .find(|o| &o.name == name)
                .ok_or_else(|| not_found(name))?;
            object.visual = visual.clone().filter(|v| *v != VisualDef::default());
        }
        Command::AddNode(node) => network::add_node(model, node)?,
        Command::MoveNode { name, position } => network::move_node(model, name, *position)?,
        Command::RenameNode { from, to } => network::rename_node(model, from, to)?,
        Command::RemoveNode { name } => network::remove_node(model, name)?,
        Command::AddEdge(edge) => network::add_edge(model, edge)?,
        Command::SetEdge { from, to, edge } => network::set_edge(model, from, to, edge)?,
        Command::RemoveEdge { from, to } => network::remove_edge(model, from, to)?,
        Command::AddAisle(aisle) => aisles::add_aisle(model, aisle)?,
        Command::SetAisle { name, aisle } => aisles::set_aisle(model, name, aisle)?,
        Command::RemoveAisle { name } => aisles::remove_aisle(model, name)?,
        Command::SetAccess { name, access } => aisles::set_access(model, name, *access)?,
        Command::SetMovement { name, movement } => aisles::set_movement(model, name, *movement)?,
        Command::SetClearance { name, clearance } => {
            aisles::set_clearance(model, name, *clearance)?;
        }
        Command::SetRunMovement(movement) => aisles::set_run_movement(model, *movement),
        Command::SetFloorMarking(marking) => aisles::set_floor_marking(model, marking)?,
        Command::AddFluidNode(node) => fluids::add_node(model, node)?,
        Command::MoveFluidNode { name, position } => fluids::move_node(model, name, *position)?,
        Command::RenameFluidNode { from, to } => fluids::rename_node(model, from, to)?,
        Command::SetFluidKind { name, kind } => fluids::set_kind(model, name, kind)?,
        Command::RemoveFluidNode { name } => fluids::remove_node(model, name)?,
        Command::AddFluidLink { from, to } => fluids::add_link(model, from, to)?,
        Command::RemoveFluidLink { from, to } => fluids::remove_link(model, from, to)?,
        Command::SetInfo { name, description } => {
            let name = name.trim();
            if name.is_empty() {
                return Err("o modelo precisa de um nome".into());
            }
            model.name = name.to_string();
            model.description = description.clone();
        }
        Command::SetFieldSource { field, source } => match source {
            FieldSource::Dist(dist) => {
                dados::set_dist(model, field, dist)?;
                model.data.retain(|i| &i.field != field);
            }
            FieldSource::Data(input) => {
                // Checks that the field exists and holds a distribution.
                dados::get_dist(model, field)?;
                let mut input = (**input).clone();
                input.field = field.clone();
                input.status = dados::DataStatus::Pending;
                match model.data.iter_mut().find(|i| &i.field == field) {
                    Some(existing) => *existing = input,
                    None => model.data.push(input),
                }
            }
        },
        Command::Batch(commands) => {
            for c in commands {
                execute(model, c)?;
            }
        }
    }
    Ok(())
}

/// Object types offered by the editor palette, with Portuguese labels.
pub const LIBRARY: [(&str, &str); 10] = [
    ("Source", "Chegada"),
    ("Queue", "Fila"),
    ("Processor", "Processo"),
    ("Combiner", "Montagem"),
    ("Separator", "Desmontagem"),
    ("Sink", "Saída"),
    ("Conveyor", "Esteira"),
    ("Rack", "Porta-paletes"),
    ("Transporter", "Transportador"),
    ("Dispatcher", "Despachante"),
];

/// One entry of the editor's object library: a kind, possibly preset (an operator is a
/// `Transporter` drawn as a person).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteEntry {
    /// Identifier of the entry (the kind itself for the plain kinds).
    pub id: &'static str,
    /// Object kind created.
    pub kind: &'static str,
    /// Portuguese label.
    pub label: &'static str,
    /// Portuguese category the palette groups it under.
    pub category: &'static str,
    /// Base of the names of new objects (`Document::unique_name` adds `_2`, `_3`...).
    pub name_base: &'static str,
    /// Asset of `visual` (presets only).
    pub asset: Option<&'static str>,
}

const fn entry(
    id: &'static str,
    kind: &'static str,
    label: &'static str,
    category: &'static str,
    name_base: &'static str,
    asset: Option<&'static str>,
) -> PaletteEntry {
    PaletteEntry {
        id,
        kind,
        label,
        category,
        name_base,
        asset,
    }
}

/// Palette categories, in display order.
pub const CATEGORIES: [&str; 4] = [
    "Fluxo de itens",
    "Montagem",
    "Manuseio de materiais",
    "Recursos móveis",
];

/// Every object of the editor's library, grouped by [`CATEGORIES`].
pub const PALETTE: [PaletteEntry; 13] = [
    entry(
        "Source",
        "Source",
        "Chegada",
        CATEGORIES[0],
        "chegada",
        None,
    ),
    entry("Queue", "Queue", "Fila", CATEGORIES[0], "fila", None),
    entry(
        "Processor",
        "Processor",
        "Processo",
        CATEGORIES[0],
        "processo",
        None,
    ),
    entry("Sink", "Sink", "Saída", CATEGORIES[0], "saida", None),
    entry(
        "Combiner",
        "Combiner",
        "Montagem",
        CATEGORIES[1],
        "montagem",
        None,
    ),
    entry(
        "Separator",
        "Separator",
        "Desmontagem",
        CATEGORIES[1],
        "desmontagem",
        None,
    ),
    entry(
        "Conveyor",
        "Conveyor",
        "Esteira",
        CATEGORIES[2],
        "esteira",
        None,
    ),
    entry(
        "Rack",
        "Rack",
        "Porta-paletes",
        CATEGORIES[2],
        "porta_paletes",
        None,
    ),
    entry(
        "Transporter",
        "Transporter",
        "Transportador",
        CATEGORIES[3],
        "transportador",
        None,
    ),
    entry(
        "Operator",
        "Transporter",
        "Operador",
        CATEGORIES[3],
        "operador",
        Some("operador"),
    ),
    entry(
        "Forklift",
        "Transporter",
        "Empilhadeira",
        CATEGORIES[3],
        "empilhadeira",
        Some("empilhadeira"),
    ),
    entry(
        "Agv",
        "Transporter",
        "AGV",
        CATEGORIES[3],
        "agv",
        Some("agv"),
    ),
    entry(
        "Dispatcher",
        "Dispatcher",
        "Despachante",
        CATEGORIES[3],
        "despachante",
        None,
    ),
];

/// The palette entry with this id.
pub fn palette_entry(id: &str) -> Option<&'static PaletteEntry> {
    PALETTE.iter().find(|e| e.id == id)
}

/// A new object from palette entry `id` with sensible defaults (a preset sets its
/// `visual` and the speeds of its role: 1.2 m/s walking, 2 m/s forklift, 1.5 m/s AGV;
/// the AGV also accelerates at 0.5 m/s², docks in 0.5 s, follows the traffic rules of
/// the network and returns to its base when idle).
pub fn from_palette(id: &str, name: &str, position: [f64; 3]) -> Option<ObjectDef> {
    let e = palette_entry(id)?;
    let mut object = template(e.kind, name, position)?;
    if let Some(asset) = e.asset {
        object.visual = Some(VisualDef {
            asset: Some(asset.to_string()),
            ..VisualDef::default()
        });
    }
    if let ObjectKind::Transporter {
        speed,
        acceleration,
        deceleration,
        agv,
        home,
        load_time,
        unload_time,
        ..
    } = &mut object.kind
    {
        match id {
            "Operator" => *speed = 1.2,
            "Forklift" => {
                *speed = 2.0;
                *load_time = Dist::Const(0.5);
                *unload_time = Dist::Const(0.5);
            }
            "Agv" => {
                // Gentle starts and stops, a short docking at each end, and back to its
                // base (a node of the network) when idle.
                *speed = 1.5;
                *acceleration = Some(0.5);
                *deceleration = Some(0.5);
                *load_time = Dist::Const(0.5);
                *unload_time = Dist::Const(0.5);
                *home = true;
                *agv = true;
            }
            _ => {}
        }
    }
    Some(object)
}

/// A new object of `kind` (one of [`LIBRARY`]) with sensible defaults.
pub fn template(kind: &str, name: &str, position: [f64; 3]) -> Option<ObjectDef> {
    let kind = match kind {
        "Source" => ObjectKind::Source {
            interarrival: Dist::Exp { mean: 1.0 },
            first_at_zero: false,
            max_arrivals: None,
            labels: Default::default(),
            on_blocked: Default::default(),
            schedule: None,
        },
        "Queue" => ObjectKind::Queue {
            capacity: None,
            discipline: Default::default(),
            batch: None,
        },
        "Processor" => ObjectKind::Processor {
            capacity: 1,
            setup: None,
            process: Dist::Const(1.0),
            operators: None,
        },
        "Combiner" => ObjectKind::Combiner {
            components: vec![1],
            mode: CombineMode::default(),
            setup: None,
            process: Dist::Const(1.0),
        },
        "Separator" => ObjectKind::Separator {
            mode: SeparateMode::default(),
            process: Dist::Const(1.0),
        },
        "Sink" => ObjectKind::Sink,
        "Transporter" => ObjectKind::Transporter {
            speed: 1.0,
            acceleration: None,
            deceleration: None,
            capacity: 1,
            load_time: Dist::Const(0.0),
            unload_time: Dist::Const(0.0),
            home: false,
            agv: false,
            movement: None,
            clearance: None,
        },
        "Conveyor" => ObjectKind::Conveyor {
            length: 5.0,
            speed: 1.0,
            item_length: 1.0,
            accumulating: true,
        },
        "Rack" => ObjectKind::Rack {
            bays: 4,
            levels: 3,
            dwell: None,
            placement: Default::default(),
            bay_width: 1.0,
            level_height: 1.0,
        },
        "Dispatcher" => ObjectKind::Dispatcher {
            team: Vec::new(),
            rule: Default::default(),
        },
        _ => return None,
    };
    Some(ObjectDef {
        name: name.to_string(),
        kind,
        output: Default::default(),
        position,
        downtime: None,
        shifts: None,
        transport: None,
        transfer: None,
        on_entry: None,
        visual: None,
        access: None,
    })
}

fn leading_comments(text: &str) -> String {
    let mut header = String::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("//") || t.is_empty() {
            header.push_str(line);
            header.push('\n');
        } else {
            break;
        }
    }
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Document {
        let mut doc = Document::new("teste");
        let parts = [
            ("Source", "chegada"),
            ("Queue", "fila"),
            ("Processor", "maquina"),
            ("Sink", "saida"),
        ];
        for (i, (kind, name)) in parts.into_iter().enumerate() {
            let object = template(kind, name, [3.0 * i as f64, 0.0, 0.0]).unwrap();
            doc.apply(Command::Add(Box::new(object))).unwrap();
        }
        for (a, b) in [
            ("chegada", "fila"),
            ("fila", "maquina"),
            ("maquina", "saida"),
        ] {
            doc.apply(Command::Connect {
                from: a.into(),
                to: b.into(),
            })
            .unwrap();
        }
        doc
    }

    #[test]
    fn built_model_is_valid_and_runs() {
        let mut doc = line();
        doc.apply(Command::Set {
            key: "run.stop_time".into(),
            value: "200.0".into(),
        })
        .unwrap();
        assert_eq!(doc.issues(), Vec::new());
        assert!(doc.replay(0).unwrap().report.block("saida").unwrap().input > 0);
    }

    #[test]
    fn warnings_are_kept_apart_from_issues() {
        let mut doc = line();
        assert!(doc.warnings().is_empty());
        let emp = from_palette("Forklift", "emp", [0.0, 5.0, 0.0]).unwrap();
        doc.apply(Command::Add(Box::new(emp))).unwrap();
        assert!(doc.issues().is_empty());
        let warnings = doc.warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].object.as_deref(), Some("emp"));
        // A long instantaneous connection is a model warning.
        doc.apply(Command::Move {
            name: "saida".into(),
            position: [60.0, 0.0, 0.0],
        })
        .unwrap();
        assert!(
            doc.warnings()
                .iter()
                .any(|w| w.message.contains("instantânea"))
        );
        // No warnings while the model has issues.
        doc.apply(Command::Disconnect {
            from: "maquina".into(),
            to: "saida".into(),
        })
        .unwrap();
        assert!(!doc.issues().is_empty());
        assert!(doc.warnings().is_empty());
    }

    #[test]
    fn empty_model_is_not_ready_to_run() {
        let issues = Document::new("vazio").issues();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("vazio"));
    }

    #[test]
    fn incomplete_model_reports_issues_per_object() {
        let mut doc = Document::new("t");
        let object = template("Processor", "maquina", [0.0; 3]).unwrap();
        doc.apply(Command::Add(Box::new(object))).unwrap();
        let issues = doc.issues();
        assert!(
            issues
                .iter()
                .any(|i| i.object.as_deref() == Some("maquina")),
            "{issues:?}"
        );
    }

    #[test]
    fn the_model_name_and_description_are_one_undoable_step() {
        let mut doc = Document::new("a");
        doc.apply(Command::SetInfo {
            name: " Tutorial 01 ".into(),
            description: "Uma fila".into(),
        })
        .unwrap();
        assert_eq!(
            (doc.model().name.as_str(), doc.model().description.as_str()),
            ("Tutorial 01", "Uma fila")
        );
        assert!(
            doc.apply(Command::SetInfo {
                name: "  ".into(),
                description: String::new(),
            })
            .is_err()
        );
        assert!(doc.undo());
        assert_eq!(doc.model().name, "a");
        assert!(doc.model().description.is_empty());
    }

    #[test]
    fn undo_redo_and_failed_commands_leave_no_trace() {
        let mut doc = line();
        let before = doc.model().clone();
        doc.apply(Command::Rename {
            from: "fila".into(),
            to: "buffer".into(),
        })
        .unwrap();
        assert!(doc.model().connections.iter().any(|(a, _)| a == "buffer"));
        let bad = Command::Connect {
            from: "buffer".into(),
            to: "nada".into(),
        };
        assert!(doc.apply(bad).is_err());
        doc.undo();
        assert_eq!(doc.model(), &before);
        doc.redo();
        assert!(doc.model().objects.iter().any(|o| o.name == "buffer"));
        doc.apply(Command::Remove {
            name: "buffer".into(),
        })
        .unwrap();
        assert_eq!(doc.model().connections.len(), 1);
    }

    #[test]
    fn swapping_connections_reorders_ports_and_undoes() {
        let mut doc = line();
        doc.apply(Command::Connect {
            from: "chegada".into(),
            to: "maquina".into(),
        })
        .unwrap();
        let before = doc.model().connections.clone();
        let first = before[0].clone();
        let last = before[before.len() - 1].clone();
        doc.apply(Command::SwapConnections {
            first: first.clone(),
            second: last.clone(),
        })
        .unwrap();
        assert_eq!(doc.model().connections[0], last);
        assert_eq!(doc.model().connections[before.len() - 1], first);
        let missing = Command::SwapConnections {
            first,
            second: ("nada".into(), "maquina".into()),
        };
        assert!(doc.apply(missing).is_err());
        doc.undo();
        assert_eq!(doc.model().connections, before);
    }

    #[test]
    fn dragging_coalesces_into_one_undo_step() {
        let mut doc = line();
        let start = doc.model().clone();
        for x in 1..=10 {
            let cmd = Command::Move {
                name: "maquina".into(),
                position: [x as f64, 0.0, 0.0],
            };
            doc.apply_coalescing(cmd, Some("mover:maquina")).unwrap();
        }
        doc.end_gesture();
        let cmd = Command::Move {
            name: "maquina".into(),
            position: [20.0, 0.0, 0.0],
        };
        doc.apply_coalescing(cmd, Some("mover:maquina")).unwrap();
        assert!(doc.undo());
        assert_eq!(doc.model().objects[2].position, [10.0, 0.0, 0.0]);
        assert!(doc.undo());
        assert_eq!(doc.model(), &start);
    }

    #[test]
    fn new_models_move_freely_and_old_ones_keep_legacy() {
        let mut doc = Document::new("novo");
        assert_eq!(doc.model().run.movement, None);
        doc.apply(Command::Add(Box::new(
            template("Transporter", "pessoa", [0.0; 3]).unwrap(),
        )))
        .unwrap();
        let m = doc.model();
        assert_eq!(
            m.movement_of(&m.objects[0]),
            Some(utsim_model::Movement::Free)
        );
        let reopened = Model::parse(&doc.to_text()).unwrap();
        assert_eq!(reopened.run.movement, None);
        // A format-1 model opened and saved keeps the old behaviour explicitly.
        let dir = std::env::temp_dir().join(format!("utsim-edit-v1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("antigo.ron");
        std::fs::write(
            &path,
            "Model(name: \"a\", run: (stop_time: 5.0), objects: [(name: \"p\", kind: Transporter(speed: 1.0))])",
        )
        .unwrap();
        let old = Document::open(&path).unwrap();
        let text = old.to_text();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            text.contains("format: 2") && text.contains("movement: Legacy"),
            "{text}"
        );
    }

    #[test]
    fn saving_is_stable_and_round_trips() {
        let mut doc = line();
        assert!(doc.to_text().contains("format: 2"));
        doc.apply(Command::Set {
            key: "fila.capacity".into(),
            value: "5".into(),
        })
        .unwrap();
        let text = doc.to_text();
        assert_eq!(text, doc.to_text());
        assert_eq!(&Model::parse(&text).unwrap(), doc.model());
    }

    #[test]
    fn opening_keeps_header_comments_and_examples_round_trip() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let doc = Document::open(dir.join("mm1.ron")).unwrap();
        assert!(doc.to_text().starts_with("// Fila M/M/1"));
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            if path.extension().is_some_and(|e| e == "ron")
                && !stem.starts_with("experimento")
                && !stem.starts_with("otimizacao")
            {
                let doc = Document::open(&path).unwrap();
                let reparsed = Model::parse(&doc.to_text()).unwrap();
                assert_eq!(&reparsed, doc.model(), "{stem}");
                assert!(!doc.is_dirty());
            }
        }
    }

    #[test]
    fn unique_names_and_templates() {
        let doc = line();
        assert_eq!(doc.unique_name("fila"), "fila_2");
        assert_eq!(doc.unique_name("nova"), "nova");
        for (kind, _) in LIBRARY {
            assert!(template(kind, "x", [0.0; 3]).is_some(), "{kind}");
        }
    }

    #[test]
    fn palette_covers_every_kind_and_presets_set_their_look() {
        for (kind, _) in LIBRARY {
            assert!(PALETTE.iter().any(|e| e.id == kind), "{kind}");
        }
        for e in PALETTE {
            let o = from_palette(e.id, "x", [1.0, 2.0, 0.0]).unwrap();
            assert_eq!(utsim_model::replay::kind_name(&o.kind), e.kind);
            assert!(CATEGORIES.contains(&e.category), "{}", e.id);
            assert_eq!(o.visual.as_ref().and_then(|v| v.asset.as_deref()), e.asset);
            assert_eq!(o.position, [1.0, 2.0, 0.0]);
        }
        let agv = from_palette("Agv", "a", [0.0; 3]).unwrap();
        assert!(matches!(
            agv.kind,
            ObjectKind::Transporter {
                agv: true,
                home: true,
                acceleration: Some(_),
                deceleration: Some(_),
                ..
            }
        ));
        assert!(from_palette("Nada", "x", [0.0; 3]).is_none());
    }

    #[test]
    fn set_visual_replaces_and_restores_the_default_look() {
        let mut doc = line();
        let look = VisualDef {
            asset: Some("maquina_kenney".into()),
            scale: Some(1.5),
            ..VisualDef::default()
        };
        doc.apply(Command::SetVisual {
            name: "maquina".into(),
            visual: Some(look.clone()),
        })
        .unwrap();
        assert_eq!(doc.model().objects[2].visual.as_ref(), Some(&look));
        assert!(doc.to_text().contains("maquina_kenney"));
        doc.apply(Command::SetVisual {
            name: "maquina".into(),
            visual: Some(VisualDef::default()),
        })
        .unwrap();
        assert_eq!(doc.model().objects[2].visual, None);
        doc.undo();
        assert_eq!(doc.model().objects[2].visual.as_ref(), Some(&look));
        let missing = Command::SetVisual {
            name: "nada".into(),
            visual: None,
        };
        assert!(doc.apply(missing).is_err());
    }
}
