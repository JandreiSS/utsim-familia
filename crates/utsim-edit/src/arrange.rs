//! Editing several objects at once: clipboard, moving and deleting a selection, align and
//! distribute, and fanning one property edit out to every selected object.
//!
//! Every function only reads the model and returns a [`Command::Batch`] (or `None` when
//! there is nothing to do), so the GUI applies it through the [`Document`](crate::Document)
//! as one undo step and a failure leaves no trace.

use std::collections::{HashMap, HashSet};

use utsim_model::replay::kind_name;
use utsim_model::{Model, ObjectDef, ObjectKind};

use crate::Command;

/// Objects copied with [`Clipboard::copy`], plus the connections among them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clipboard {
    /// The copied objects, in model order.
    pub objects: Vec<ObjectDef>,
    /// Connections whose both ends were copied (connections to other objects are not).
    pub connections: Vec<(String, String)>,
}

/// `name` without a trailing `_<digits>` (`maquina_2` -> `maquina`), the base of the
/// names given to pasted copies.
pub fn name_root(name: &str) -> &str {
    match name.rsplit_once('_') {
        Some((root, digits))
            if !root.is_empty()
                && !digits.is_empty()
                && digits.chars().all(|c| c.is_ascii_digit()) =>
        {
            root
        }
        _ => name,
    }
}

/// A name for a copy of `name` that `taken` rejects: `name` itself when free (pasting
/// after a cut keeps the names), else `root_2`, `root_3`... (`maquina` -> `maquina_2`,
/// `maquina_2` -> `maquina_3`).
pub fn copy_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let root = name_root(name);
    (2..)
        .map(|i| format!("{root}_{i}"))
        .find(|n| !taken(n))
        .expect("some suffix is free")
}

impl Clipboard {
    /// Copies the objects called `names` (unknown names are skipped) and the
    /// connections among them.
    pub fn copy(model: &Model, names: &[String]) -> Clipboard {
        let chosen: HashSet<&str> = names.iter().map(String::as_str).collect();
        Clipboard {
            objects: model
                .objects
                .iter()
                .filter(|o| chosen.contains(o.name.as_str()))
                .cloned()
                .collect(),
            connections: model
                .connections
                .iter()
                .filter(|(a, b)| chosen.contains(a.as_str()) && chosen.contains(b.as_str()))
                .cloned()
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// The command that adds a copy of the clipboard to `model`, moved by `offset`:
    /// unique names ([`copy_name`]), the copied connections between the copies, and the
    /// references among copied objects (`transport`, `operators.team`, a dispatcher's
    /// team) pointing at the copies. `transport` and `operators.team` naming objects that
    /// were not copied are kept; a copied dispatcher drops the members that were not
    /// copied (a transporter belongs to one team). Returns the command and the new
    /// names, in clipboard order.
    pub fn paste(&self, model: &Model, offset: [f64; 3]) -> Option<(Command, Vec<String>)> {
        if self.is_empty() {
            return None;
        }
        let existing: HashSet<&str> = model.objects.iter().map(|o| o.name.as_str()).collect();
        let mut renames: HashMap<String, String> = HashMap::new();
        let mut assigned: HashSet<String> = HashSet::new();
        let mut names = Vec::new();
        for o in &self.objects {
            let taken = |n: &str| existing.contains(n) || assigned.contains(n);
            let new = copy_name(&o.name, &taken);
            assigned.insert(new.clone());
            renames.insert(o.name.clone(), new.clone());
            names.push(new);
        }
        let map = |n: &str| renames.get(n).cloned().unwrap_or_else(|| n.to_string());
        let mut commands = Vec::new();
        for (o, new) in self.objects.iter().zip(&names) {
            let mut copy = o.clone();
            copy.name = new.clone();
            for (p, d) in copy.position.iter_mut().zip(offset) {
                *p += d;
            }
            if let Some(t) = &mut copy.transport {
                *t = map(t);
            }
            match &mut copy.kind {
                ObjectKind::Processor {
                    operators: Some(op),
                    ..
                } => op.team = map(&op.team),
                ObjectKind::Dispatcher { team, .. } => {
                    // A transporter belongs to one team only: a copied dispatcher keeps
                    // the copies of its copied members and drops the others.
                    *team = team
                        .iter()
                        .filter_map(|m| renames.get(m).cloned())
                        .collect();
                }
                _ => {}
            }
            commands.push(Command::Add(Box::new(copy)));
        }
        for (a, b) in &self.connections {
            commands.push(Command::Connect {
                from: map(a),
                to: map(b),
            });
        }
        Some((Command::Batch(commands), names))
    }
}

/// Removes every object of `names` (and their connections) in one step.
pub fn delete_command(names: &[String]) -> Option<Command> {
    (!names.is_empty()).then(|| {
        Command::Batch(
            names
                .iter()
                .map(|n| Command::Remove { name: n.clone() })
                .collect(),
        )
    })
}

/// Current positions of `names` (unknown names are skipped).
pub fn positions(model: &Model, names: &[String]) -> Vec<(String, [f64; 3])> {
    names
        .iter()
        .filter_map(|n| {
            let o = model.objects.iter().find(|o| &o.name == n)?;
            Some((n.clone(), o.position))
        })
        .collect()
}

/// Moves objects from their `starts` by `delta` on the ground (`x`, `y`; the height is
/// kept). Dragging a selection calls it with the positions taken when the drag started.
pub fn moved_from(starts: &[(String, [f64; 3])], delta: [f64; 2]) -> Command {
    Command::Batch(
        starts
            .iter()
            .map(|(name, p)| Command::Move {
                name: name.clone(),
                position: [p[0] + delta[0], p[1] + delta[1], p[2]],
            })
            .collect(),
    )
}

/// Moves `names` together by `delta` on the ground.
pub fn move_command(model: &Model, names: &[String], delta: [f64; 2]) -> Option<Command> {
    let starts = positions(model, names);
    (!starts.is_empty()).then(|| moved_from(&starts, delta))
}

/// How [`align_command`] lines objects up (model plan coordinates: `x` to the right,
/// `y` to the top of the plan).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}

/// Lines `names` up by their positions (the objects' centres): the smallest `x` for
/// `Left`, the largest for `Right`, the middle of the extremes for `CenterX`; the same
/// in `y` for `Bottom`, `Top` and `CenterY`. Needs two objects or more.
pub fn align_command(model: &Model, names: &[String], how: Align) -> Option<Command> {
    let starts = positions(model, names);
    if starts.len() < 2 {
        return None;
    }
    let axis = match how {
        Align::Left | Align::CenterX | Align::Right => 0,
        Align::Top | Align::CenterY | Align::Bottom => 1,
    };
    let values = starts.iter().map(|(_, p)| p[axis]);
    let min = values.clone().fold(f64::INFINITY, f64::min);
    let max = values.fold(f64::NEG_INFINITY, f64::max);
    let target = match how {
        Align::Left | Align::Bottom => min,
        Align::Right | Align::Top => max,
        Align::CenterX | Align::CenterY => (min + max) / 2.0,
    };
    Some(Command::Batch(
        starts
            .into_iter()
            .map(|(name, mut position)| {
                position[axis] = target;
                Command::Move { name, position }
            })
            .collect(),
    ))
}

/// Axis of [`distribute_command`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// Spaces `names` evenly along `axis`: the two extreme objects stay, the others are put
/// at equal steps between them, keeping their order. Needs three objects or more.
pub fn distribute_command(model: &Model, names: &[String], axis: Axis) -> Option<Command> {
    let k = match axis {
        Axis::X => 0,
        Axis::Y => 1,
    };
    let mut starts = positions(model, names);
    if starts.len() < 3 {
        return None;
    }
    starts.sort_by(|a, b| a.1[k].total_cmp(&b.1[k]).then_with(|| a.0.cmp(&b.0)));
    let first = starts[0].1[k];
    let step = (starts[starts.len() - 1].1[k] - first) / (starts.len() - 1) as f64;
    Some(Command::Batch(
        starts
            .into_iter()
            .enumerate()
            .map(|(i, (name, mut position))| {
                position[k] = first + step * i as f64;
                Command::Move { name, position }
            })
            .collect(),
    ))
}

/// Whether `names` are two or more objects of one kind (so a property edit can be
/// applied to all of them).
pub fn same_kind(model: &Model, names: &[String]) -> bool {
    let kinds: Vec<&str> = names
        .iter()
        .filter_map(|n| model.objects.iter().find(|o| &o.name == n))
        .map(|o| kind_name(&o.kind))
        .collect();
    kinds.len() >= 2 && kinds.len() == names.len() && kinds.iter().all(|k| *k == kinds[0])
}

/// The property edits made on `primary` repeated on each of `others`, as one batch:
/// field changes (`Set` of `primary.<campo>`) and the look (`SetVisual`). Renames and
/// other commands are not repeated. `None` when nothing was edited.
pub fn fan_out(commands: &[Command], primary: &str, others: &[String]) -> Option<Command> {
    if commands.is_empty() {
        return None;
    }
    let prefix = format!("{primary}.");
    let mut all = commands.to_vec();
    for c in commands {
        for other in others.iter().filter(|o| o.as_str() != primary) {
            match c {
                Command::Set { key, value } => {
                    if let Some(field) = key.strip_prefix(&prefix) {
                        all.push(Command::Set {
                            key: format!("{other}.{field}"),
                            value: value.clone(),
                        });
                    }
                }
                Command::SetVisual { name, visual } if name == primary => {
                    all.push(Command::SetVisual {
                        name: other.clone(),
                        visual: visual.clone(),
                    });
                }
                _ => {}
            }
        }
    }
    Some(Command::Batch(all))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, template};
    use utsim_model::VisualDef;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// chegada -> fila -> maquina -> saida, three metres apart along x.
    fn line() -> Document {
        let mut doc = Document::new("t");
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
    fn copy_names_get_a_numeric_suffix() {
        assert_eq!(name_root("maquina_2"), "maquina");
        assert_eq!(name_root("maquina"), "maquina");
        assert_eq!(name_root("porta_paletes"), "porta_paletes");
        assert_eq!(name_root("_7"), "_7");
        assert_eq!(name_root("linha_"), "linha_");
        let taken = |n: &str| ["maquina", "maquina_2"].contains(&n);
        assert_eq!(copy_name("maquina", &taken), "maquina_3");
        assert_eq!(copy_name("maquina_2", &taken), "maquina_3");
        assert_eq!(copy_name("fila", &taken), "fila");
    }

    #[test]
    fn paste_gives_unique_names_offset_and_internal_connections() {
        let mut doc = line();
        let clip = Clipboard::copy(doc.model(), &names(&["fila", "maquina"]));
        assert_eq!(clip.objects.len(), 2);
        // Only the connection between the two copied objects.
        assert_eq!(clip.connections, vec![("fila".into(), "maquina".into())]);
        let (cmd, new) = clip.paste(doc.model(), [0.0, -4.0, 0.0]).unwrap();
        assert_eq!(new, names(&["fila_2", "maquina_2"]));
        doc.apply(cmd).unwrap();
        let m = doc.model();
        assert_eq!(m.objects.len(), 6);
        let pos = |n: &str| m.objects.iter().find(|o| o.name == n).unwrap().position;
        assert_eq!(pos("fila_2"), [3.0, -4.0, 0.0]);
        assert_eq!(pos("maquina_2"), [6.0, -4.0, 0.0]);
        let has = |a: &str, b: &str| m.connections.iter().any(|(x, y)| x == a && y == b);
        assert!(has("fila_2", "maquina_2"));
        assert!(!has("chegada", "fila_2") && !has("maquina_2", "saida"));
        assert_eq!(m.connections.len(), 4);
        // Pasting again keeps numbering.
        let (cmd, new) = clip.paste(doc.model(), [0.0, -8.0, 0.0]).unwrap();
        assert_eq!(new, names(&["fila_3", "maquina_3"]));
        doc.apply(cmd).unwrap();
        // The whole paste is one undo step.
        doc.undo();
        doc.undo();
        assert_eq!(doc.model(), line().model());
        assert!(Clipboard::default().paste(doc.model(), [0.0; 3]).is_none());
    }

    #[test]
    fn cut_then_paste_restores_names_and_references_follow_the_copies() {
        let text = r#"Model(
            name: "m", run: (stop_time: 10.0),
            objects: [
                (name: "q", kind: Queue(), transport: "equipe"),
                (name: "p", kind: Processor(process: Const(1.0),
                    operators: (team: "equipe"))),
                (name: "v", kind: Transporter(speed: 1.0)),
                (name: "w", kind: Transporter(speed: 1.0)),
                (name: "equipe", kind: Dispatcher(team: ["v", "w"])),
                (name: "k", kind: Sink),
            ],
            connections: [("q", "p"), ("p", "k")],
        )"#;
        let model = Model::parse(text).unwrap();
        let mut doc = Document::new("m");
        for o in &model.objects {
            doc.apply(Command::Add(Box::new(o.clone()))).unwrap();
        }
        for (a, b) in &model.connections {
            doc.apply(Command::Connect {
                from: a.clone(),
                to: b.clone(),
            })
            .unwrap();
        }
        // Copy q, p, v and the dispatcher (not w): references among copies follow them.
        let clip = Clipboard::copy(doc.model(), &names(&["q", "p", "v", "equipe"]));
        let (cmd, new) = clip.paste(doc.model(), [5.0, 0.0, 0.0]).unwrap();
        doc.apply(cmd).unwrap();
        assert_eq!(new, names(&["q_2", "p_2", "v_2", "equipe_2"]));
        let get = |n: &str| doc.model().objects.iter().find(|o| o.name == n).unwrap();
        assert_eq!(get("q_2").transport.as_deref(), Some("equipe_2"));
        let ObjectKind::Processor {
            operators: Some(op),
            ..
        } = &get("p_2").kind
        else {
            panic!()
        };
        assert_eq!(op.team, "equipe_2");
        let ObjectKind::Dispatcher { team, .. } = &get("equipe_2").kind else {
            panic!()
        };
        // A transporter belongs to one team: members that were not copied are dropped.
        assert_eq!(team, &names(&["v_2"]));
        assert!(
            doc.issues().iter().all(|i| !i.message.contains("equipes")),
            "{:?}",
            doc.issues()
        );

        // Cut (delete) and paste: the original names come back.
        let clip = Clipboard::copy(doc.model(), &names(&["q_2", "p_2"]));
        doc.apply(delete_command(&names(&["q_2", "p_2"])).unwrap())
            .unwrap();
        let (cmd, new) = clip.paste(doc.model(), [0.0; 3]).unwrap();
        assert_eq!(new, names(&["q_2", "p_2"]));
        doc.apply(cmd).unwrap();
    }

    #[test]
    fn moving_and_deleting_a_selection_are_single_undo_steps() {
        let mut doc = line();
        let start = doc.model().clone();
        let sel = names(&["fila", "maquina"]);
        doc.apply(move_command(doc.model(), &sel, [1.0, 2.0]).unwrap())
            .unwrap();
        let pos = |d: &Document, n: &str| {
            d.model()
                .objects
                .iter()
                .find(|o| o.name == n)
                .unwrap()
                .position
        };
        assert_eq!(pos(&doc, "fila"), [4.0, 2.0, 0.0]);
        assert_eq!(pos(&doc, "maquina"), [7.0, 2.0, 0.0]);
        assert_eq!(pos(&doc, "chegada"), [0.0, 0.0, 0.0]);
        assert!(doc.undo());
        assert_eq!(doc.model(), &start);

        doc.apply(delete_command(&sel).unwrap()).unwrap();
        assert_eq!(doc.model().objects.len(), 2);
        assert!(doc.model().connections.is_empty());
        assert!(doc.undo());
        assert_eq!(doc.model(), &start);
        assert!(delete_command(&[]).is_none());
        assert!(move_command(doc.model(), &names(&["nada"]), [1.0, 0.0]).is_none());

        // A drag: every step starts from the same positions and coalesces into one.
        let starts = positions(doc.model(), &sel);
        for k in 1..=5 {
            let cmd = moved_from(&starts, [k as f64, 0.0]);
            doc.apply_coalescing(cmd, Some("mover:selecao")).unwrap();
        }
        doc.end_gesture();
        assert_eq!(pos(&doc, "fila"), [8.0, 0.0, 0.0]);
        assert!(doc.undo());
        assert_eq!(doc.model(), &start);
    }

    #[test]
    fn align_and_distribute() {
        let mut doc = line();
        doc.apply(Command::Move {
            name: "fila".into(),
            position: [3.0, 4.0, 0.0],
        })
        .unwrap();
        doc.apply(Command::Move {
            name: "maquina".into(),
            position: [7.5, -2.0, 0.0],
        })
        .unwrap();
        let all = names(&["chegada", "fila", "maquina", "saida"]);
        let y = |d: &Document| {
            d.model()
                .objects
                .iter()
                .map(|o| o.position[1])
                .collect::<Vec<_>>()
        };
        let x = |d: &Document| {
            d.model()
                .objects
                .iter()
                .map(|o| o.position[0])
                .collect::<Vec<_>>()
        };

        let mut d = Document::new("x");
        d.apply(Command::Batch(
            doc.model()
                .objects
                .iter()
                .map(|o| Command::Add(Box::new(o.clone())))
                .collect(),
        ))
        .unwrap();
        let cases = [
            (Align::Top, 1, 4.0),
            (Align::Bottom, 1, -2.0),
            (Align::CenterY, 1, 1.0),
            (Align::Left, 0, 0.0),
            (Align::Right, 0, 9.0),
            (Align::CenterX, 0, 4.5),
        ];
        for (how, axis, want) in cases {
            d.apply(align_command(d.model(), &all, how).unwrap())
                .unwrap();
            let got = if axis == 0 { x(&d) } else { y(&d) };
            assert!(got.iter().all(|v| *v == want), "{how:?}: {got:?}");
            assert!(d.undo());
        }
        // Aligning moves along one axis only.
        d.apply(align_command(d.model(), &all, Align::Top).unwrap())
            .unwrap();
        assert_eq!(x(&d), vec![0.0, 3.0, 7.5, 9.0]);
        d.undo();

        // Distribute along x: the ends stay, the middle ones get equal steps.
        doc.apply(distribute_command(doc.model(), &all, Axis::X).unwrap())
            .unwrap();
        assert_eq!(x(&doc), vec![0.0, 3.0, 6.0, 9.0]);
        assert_eq!(y(&doc), vec![0.0, 4.0, -2.0, 0.0]);
        // Along y, ordered by y: maquina (-2) and fila (4) are the ends.
        doc.apply(distribute_command(doc.model(), &all, Axis::Y).unwrap())
            .unwrap();
        assert_eq!(y(&doc), vec![0.0, 4.0, -2.0, 2.0]);
        assert!(align_command(doc.model(), &names(&["fila"]), Align::Left).is_none());
        assert!(distribute_command(doc.model(), &names(&["fila", "saida"]), Axis::X).is_none());
    }

    #[test]
    fn a_property_edit_fans_out_to_objects_of_the_same_kind() {
        let mut doc = line();
        let extra = template("Processor", "maquina_2", [0.0, 3.0, 0.0]).unwrap();
        doc.apply(Command::Add(Box::new(extra))).unwrap();
        let sel = names(&["maquina", "maquina_2"]);
        assert!(same_kind(doc.model(), &sel));
        assert!(!same_kind(doc.model(), &names(&["maquina", "fila"])));
        assert!(!same_kind(doc.model(), &names(&["maquina"])));
        let edits = vec![
            Command::Set {
                key: "maquina.capacity".into(),
                value: "3".into(),
            },
            Command::SetVisual {
                name: "maquina".into(),
                visual: Some(VisualDef {
                    color: Some("#ff8800".into()),
                    ..VisualDef::default()
                }),
            },
            Command::Rename {
                from: "maquina".into(),
                to: "torno".into(),
            },
        ];
        let before = doc.model().clone();
        doc.apply(fan_out(&edits, "maquina", &sel).unwrap())
            .unwrap();
        for o in doc.model().objects.iter().filter(|o| o.name != "fila") {
            if let ObjectKind::Processor { capacity, .. } = o.kind {
                assert_eq!(capacity, 3, "{}", o.name);
                assert_eq!(
                    o.visual.as_ref().and_then(|v| v.color.as_deref()),
                    Some("#ff8800")
                );
            }
        }
        // The rename is applied to the primary only.
        assert!(doc.model().objects.iter().any(|o| o.name == "torno"));
        assert!(doc.model().objects.iter().any(|o| o.name == "maquina_2"));
        assert!(doc.undo());
        assert_eq!(doc.model(), &before);
        assert!(fan_out(&[], "maquina", &sel).is_none());
    }
}
