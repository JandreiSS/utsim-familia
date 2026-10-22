//! The editable model behind the editor (phase 3), independent of any GUI.
//!
//! Every change goes through a [`Command`]; the [`Document`] keeps the undo/redo
//! history, reports validation issues per object while the model is still
//! incomplete, and saves it as stable, diff-friendly RON text.

use std::path::{Path, PathBuf};

use utsim_core::Dist;
use utsim_model::experiment::patch;
use utsim_model::replay::Replay;
use utsim_model::{Model, ModelError, ObjectDef, ObjectKind};
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
    /// Several commands undone as one step.
    Batch(Vec<Command>),
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
            "Model(name: {name:?}, run: (stop_time: 10000.0), objects: [], connections: [])"
        ))
        .expect("empty model parses");
        Document::from_model(model, None, String::new())
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, ModelError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        let model = Model::parse(&text)?;
        Ok(Document::from_model(
            model,
            Some(path.to_path_buf()),
            leading_comments(&text),
        ))
    }

    fn from_model(model: Model, path: Option<PathBuf>, header: String) -> Self {
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
        self.model
            .objects
            .iter()
            .map(|o| o.name.as_str())
            .filter(|name| {
                message.starts_with(&format!("{name}:"))
                    || message.starts_with(&format!("{name}."))
                    || message.contains(&format!("'{name}'"))
                    || message.contains(&format!("({name} ->"))
            })
            .max_by_key(|name| name.len())
            .map(str::to_string)
    }

    /// `base`, or `base_2`, `base_3`... whichever is free.
    pub fn unique_name(&self, base: &str) -> String {
        let taken = |n: &str| self.model.objects.iter().any(|o| o.name == n);
        if !taken(base) {
            return base.to_string();
        }
        (2..)
            .map(|i| format!("{base}_{i}"))
            .find(|n| !taken(n))
            .expect("some suffix is free")
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
        Command::Batch(commands) => {
            for c in commands {
                execute(model, c)?;
            }
        }
    }
    Ok(())
}

/// Object types offered by the editor palette, with Portuguese labels.
pub const LIBRARY: [(&str, &str); 8] = [
    ("Source", "Chegada"),
    ("Queue", "Fila"),
    ("Processor", "Processo"),
    ("Combiner", "Montagem"),
    ("Separator", "Desmontagem"),
    ("Sink", "Saída"),
    ("Transporter", "Transportador"),
    ("Dispatcher", "Despachante"),
];

/// A new object of `kind` (one of [`LIBRARY`]) with sensible defaults.
pub fn template(kind: &str, name: &str, position: [f64; 3]) -> Option<ObjectDef> {
    let kind = match kind {
        "Source" => ObjectKind::Source {
            interarrival: Dist::Exp { mean: 1.0 },
            first_at_zero: false,
            max_arrivals: None,
            labels: Default::default(),
            on_blocked: Default::default(),
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
    fn saving_is_stable_and_round_trips() {
        let mut doc = line();
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
            if path.extension().is_some_and(|e| e == "ron") && !stem.starts_with("experimento") {
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
}
