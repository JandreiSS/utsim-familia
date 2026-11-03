//! The qualitative tools of the phases (`docs/plano_lean_seis_sigma.md`, 2.1 to 2.5, etapa
//! L2). Each tool is a deliverable (`entregavel`) of a phase; its rows and nodes are
//! [`Kind::ToolItem`] entities pointing at the deliverable. Every cell is one field of one
//! entity, so team editing, the history and the conflicts work as on the action board.
//!
//! - [`voc`]: voice of the customer and the CTQ tree;
//! - [`sipoc`]: suppliers, inputs, process, outputs, customers;
//! - [`ishikawa`]: cause and effect diagram (6M);
//! - [`whys`]: five whys chains, linked to Ishikawa causes;
//! - [`cause_effect`]: cause and effect matrix (C&E);
//! - [`pareto`]: Pareto analysis;
//! - [`fmea`]: process FMEA with RPN and action priority; [`ap`]: the AP tables;
//! - [`plans`]: data collection, control and response plans;
//! - [`a3`]: the one page report composed of the other deliverables;
//! - [`tree`]: forests of items (parent links) robust to cycles.

pub mod a3;
pub mod ap;
pub mod cause_effect;
pub mod fmea;
pub mod ishikawa;
pub mod pareto;
pub mod plans;
pub mod sipoc;
pub mod tree;
pub mod voc;
pub mod whys;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{enum_value, of_project, phases};
use crate::id::Id;
use crate::op::{Edit, EntityRef, FIELD_DELETED, Kind};
use crate::state::{Entity, State};

/// Field names of deliverables and items.
pub mod fields {
    pub use crate::domain::fields::deliverable;
    pub mod item {
        pub const DELIVERABLE: &str = "entregavel";
        pub const PROJECT: &str = "projeto";
        /// What the item is inside its tool (`causa`, `linha_fmea`, `ctq`...).
        pub const ITEM_KIND: &str = "tipo_item";
        pub const PARENT: &str = "pai";
        pub const ORDER: &str = "ordem";
        pub const TEXT: &str = "texto";
    }
}

/// The tools of etapa L2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tool {
    #[serde(rename = "voc_ctq")]
    Voc,
    #[serde(rename = "sipoc")]
    Sipoc,
    #[serde(rename = "pareto")]
    Pareto,
    #[serde(rename = "plano_coleta")]
    DataCollection,
    #[serde(rename = "ishikawa")]
    Ishikawa,
    #[serde(rename = "cinco_porques")]
    FiveWhys,
    #[serde(rename = "matriz_ce")]
    CauseEffect,
    #[serde(rename = "fmea")]
    Fmea,
    #[serde(rename = "plano_controle")]
    ControlPlan,
    #[serde(rename = "plano_resposta")]
    ResponsePlan,
    #[serde(rename = "a3")]
    A3,
}

impl Tool {
    pub const ALL: [Tool; 11] = [
        Tool::Voc,
        Tool::Sipoc,
        Tool::Pareto,
        Tool::DataCollection,
        Tool::Ishikawa,
        Tool::FiveWhys,
        Tool::CauseEffect,
        Tool::Fmea,
        Tool::ControlPlan,
        Tool::ResponsePlan,
        Tool::A3,
    ];

    /// Index of the DMAIC phase where the tool usually lives (Definir 0 ... Controlar 4).
    pub fn usual_phase(self) -> usize {
        match self {
            Tool::Voc | Tool::Sipoc | Tool::Pareto | Tool::A3 => 0,
            Tool::DataCollection => 1,
            Tool::Ishikawa | Tool::FiveWhys | Tool::CauseEffect | Tool::Fmea => 2,
            Tool::ControlPlan | Tool::ResponsePlan => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeliverableStatus {
    #[default]
    #[serde(rename = "em_elaboracao")]
    Draft,
    #[serde(rename = "concluido")]
    Done,
}

/// A deliverable of a phase.
#[derive(Debug, Clone, PartialEq)]
pub struct Deliverable {
    pub id: Id,
    pub project: Option<Id>,
    pub phase: Option<Id>,
    pub name: String,
    pub tool: Option<Tool>,
    pub status: DeliverableStatus,
}

impl Deliverable {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields::deliverable as f;
        Self {
            id,
            project: e.id(f::PROJECT),
            phase: e.id(f::PHASE),
            name: e.text(f::NAME),
            tool: e.typed(f::TOOL),
            status: e.typed(f::STATUS).unwrap_or_default(),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Deliverable, id)
    }

    /// The edits that create a deliverable of `tool` in `phase`.
    pub fn create(project: Id, phase: Option<Id>, tool: Tool, id: Id, name: &str) -> Vec<Edit> {
        use fields::deliverable as f;
        let r = Self::entity_ref(id);
        let mut edits = vec![
            Edit::new(r, f::PROJECT, project.to_string()),
            Edit::new(r, f::NAME, name),
            Edit::new(r, f::TOOL, enum_value(tool)),
            Edit::new(r, f::STATUS, enum_value(DeliverableStatus::Draft)),
        ];
        if let Some(p) = phase {
            edits.push(Edit::new(r, f::PHASE, p.to_string()));
        }
        edits
    }
}

/// The phase where a new deliverable of `tool` goes: the usual DMAIC phase, clamped to the
/// project's phases (an A3 or a kaizen has fewer).
pub fn default_phase(state: &State, project: Id, tool: Tool) -> Option<Id> {
    let ph = phases(state, project);
    if ph.is_empty() {
        return None;
    }
    Some(ph[tool.usual_phase().min(ph.len() - 1)].id)
}

/// Deliverables of a project in phase order, then creation order.
pub fn deliverables(state: &State, project: Id) -> Vec<Deliverable> {
    let order: Vec<Id> = phases(state, project).iter().map(|p| p.id).collect();
    let mut v = of_project(state, Kind::Deliverable, project, Deliverable::read);
    v.sort_by_key(|d| {
        let pos = d
            .phase
            .and_then(|p| order.iter().position(|x| *x == p))
            .unwrap_or(usize::MAX);
        (pos, d.id)
    });
    v
}

/// Deliverables of one tool.
pub fn of_tool(state: &State, project: Id, tool: Tool) -> Vec<Deliverable> {
    deliverables(state, project)
        .into_iter()
        .filter(|d| d.tool == Some(tool))
        .collect()
}

/// A row or node of a deliverable, with its raw entity for the tool views.
#[derive(Debug, Clone, Copy)]
pub struct ItemRef<'a> {
    pub id: Id,
    pub entity: &'a Entity,
}

impl ItemRef<'_> {
    pub fn parent(&self) -> Option<Id> {
        self.entity.id(fields::item::PARENT)
    }
    pub fn order(&self) -> f64 {
        self.entity.number(fields::item::ORDER).unwrap_or(0.0)
    }
    pub fn text(&self, field: &str) -> String {
        self.entity.text(field)
    }
}

pub fn item_ref(id: Id) -> EntityRef {
    EntityRef::new(Kind::ToolItem, id)
}

/// Items of `item_kind` in a deliverable, sorted by `ordem` (then id).
pub fn items<'a>(state: &'a State, deliverable: Id, item_kind: &str) -> Vec<ItemRef<'a>> {
    let key = deliverable.to_string();
    let mut v: Vec<ItemRef<'a>> = state
        .of_kind(Kind::ToolItem)
        .filter(|(_, e)| {
            e.get(fields::item::DELIVERABLE).and_then(Value::as_str) == Some(key.as_str())
                && e.get(fields::item::ITEM_KIND).and_then(Value::as_str) == Some(item_kind)
        })
        .map(|(id, entity)| ItemRef { id, entity })
        .collect();
    v.sort_by(|a, b| a.order().total_cmp(&b.order()).then(a.id.cmp(&b.id)));
    v
}

/// Every item of a deliverable, of any kind.
pub fn all_items(state: &State, deliverable: Id) -> Vec<ItemRef<'_>> {
    let key = deliverable.to_string();
    state
        .of_kind(Kind::ToolItem)
        .filter(|(_, e)| {
            e.get(fields::item::DELIVERABLE).and_then(Value::as_str) == Some(key.as_str())
        })
        .map(|(id, entity)| ItemRef { id, entity })
        .collect()
}

/// A new item of `item_kind` at the end of its siblings (same kind and parent), with the
/// given extra fields.
pub fn new_item(
    state: &State,
    project: Id,
    deliverable: Id,
    item_kind: &str,
    id: Id,
    parent: Option<Id>,
    extra: Vec<(&str, Value)>,
) -> Vec<Edit> {
    use fields::item as f;
    let last = items(state, deliverable, item_kind)
        .iter()
        .filter(|i| i.parent() == parent)
        .map(|i| i.order())
        .fold(None, |m: Option<f64>, o| Some(m.map_or(o, |m| m.max(o))));
    let r = item_ref(id);
    let mut edits = vec![
        Edit::new(r, f::PROJECT, project.to_string()),
        Edit::new(r, f::DELIVERABLE, deliverable.to_string()),
        Edit::new(r, f::ITEM_KIND, item_kind),
        Edit::new(r, f::ORDER, last.map_or(1.0, |o| o + 1.0)),
    ];
    if let Some(p) = parent {
        edits.push(Edit::new(r, f::PARENT, p.to_string()));
    }
    for (field, value) in extra {
        edits.push(Edit::new(r, field, value));
    }
    edits
}

/// Moves an item one place up (`-1`) or down (`+1`) among its siblings: swaps the `ordem`
/// of the two items (two fields written).
pub fn nudge_item(state: &State, item: Id, step: i32) -> Vec<Edit> {
    let Some(e) = state.entity(item).filter(|e| e.kind == Kind::ToolItem) else {
        return Vec::new();
    };
    let (Some(del), kind) = (
        e.id(fields::item::DELIVERABLE),
        e.text(fields::item::ITEM_KIND),
    ) else {
        return Vec::new();
    };
    let parent = e.id(fields::item::PARENT);
    let sibs: Vec<ItemRef> = items(state, del, &kind)
        .into_iter()
        .filter(|i| i.parent() == parent)
        .collect();
    let Some(pos) = sibs.iter().position(|i| i.id == item) else {
        return Vec::new();
    };
    let target = pos as i64 + i64::from(step);
    let Some(other) = usize::try_from(target).ok().and_then(|t| sibs.get(t)) else {
        return Vec::new();
    };
    let (a, b) = (sibs[pos].order(), other.order());
    let (a, b) = if a == b {
        // Equal orders (two people added at once): spread them by the step.
        (b + f64::from(step), a)
    } else {
        (b, a)
    };
    vec![
        Edit::new(item_ref(item), fields::item::ORDER, a),
        Edit::new(item_ref(other.id), fields::item::ORDER, b),
    ]
}

/// Edits that delete an item and every item below it (children by `pai`).
pub fn delete_subtree(state: &State, item: Id) -> Vec<Edit> {
    let Some(e) = state.entity(item) else {
        return Vec::new();
    };
    let Some(del) = e.id(fields::item::DELIVERABLE) else {
        return vec![Edit::new(item_ref(item), FIELD_DELETED, true)];
    };
    let all = all_items(state, del);
    let mut out = vec![item];
    let mut i = 0;
    while i < out.len() {
        let p = out[i];
        for c in &all {
            if c.parent() == Some(p) && !out.contains(&c.id) {
                out.push(c.id);
            }
        }
        i += 1;
    }
    out.into_iter()
        .map(|id| Edit::new(item_ref(id), FIELD_DELETED, true))
        .collect()
}

/// A rating 1 to 10 (S, O, D, weights); anything else is absent.
pub fn rating(e: &Entity, field: &str) -> Option<u8> {
    e.integer(field)
        .filter(|v| (1..=10).contains(v))
        .map(|v| v as u8)
}

/// A number typed by a person: `1,5`, `1.5`, ` 2 `; `None` when empty or not a number.
pub fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let t = if t.contains(',') && !t.contains('.') {
        t.replace(',', ".")
    } else {
        t.replace(',', "")
    };
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::domain::{Approach, Project};

    /// A state with one DMAIC project, an author and an id generator.
    pub struct Fixture {
        pub state: State,
        pub me: Id,
        pub project: Id,
        n: u64,
    }

    impl Fixture {
        pub fn new() -> Self {
            let mut f = Fixture {
                state: State::new(),
                me: Id::v7(1, [1; 10]),
                project: Id::v7(2, [2; 10]),
                n: 100,
            };
            let p = f.project;
            let edits = Project::create(p, "P", Approach::Dmaic, || f.id_raw());
            f.commit(edits);
            f
        }

        fn id_raw(&mut self) -> Id {
            self.n += 1;
            let mut r = [0u8; 10];
            r[2..].copy_from_slice(&self.n.to_be_bytes());
            Id::v7(self.n, r)
        }

        pub fn id(&mut self) -> Id {
            self.id_raw()
        }

        pub fn commit(&mut self, edits: Vec<Edit>) {
            let mut n = self.n;
            let ops = self.state.prepare(self.me, 0, edits, || {
                n += 1;
                let mut r = [0u8; 10];
                r[2..].copy_from_slice(&n.to_be_bytes());
                Id::v7(n, r)
            });
            self.n = n;
            for op in ops {
                self.state.apply(&op).unwrap();
            }
        }

        pub fn deliverable(&mut self, tool: Tool) -> Id {
            let id = self.id();
            let phase = default_phase(&self.state, self.project, tool);
            self.commit(Deliverable::create(self.project, phase, tool, id, "x"));
            id
        }

        pub fn item(
            &mut self,
            del: Id,
            kind: &str,
            parent: Option<Id>,
            extra: Vec<(&str, Value)>,
        ) -> Id {
            let id = self.id();
            let e = new_item(&self.state, self.project, del, kind, id, parent, extra);
            self.commit(e);
            id
        }

        pub fn set(&mut self, item: Id, field: &str, v: impl Into<Value>) {
            let r = self
                .state
                .entity(item)
                .map(|e| EntityRef::new(e.kind, item))
                .unwrap_or(item_ref(item));
            self.commit(vec![Edit::new(r, field, v)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Fixture;
    use super::*;

    #[test]
    fn deliverables_go_to_their_usual_phase() {
        let mut f = Fixture::new();
        let ph = phases(&f.state, f.project);
        let fmea = f.deliverable(Tool::Fmea);
        let voc = f.deliverable(Tool::Voc);
        let cp = f.deliverable(Tool::ControlPlan);
        let ds = deliverables(&f.state, f.project);
        let order: Vec<Id> = ds.iter().map(|d| d.id).collect();
        assert_eq!(order, [voc, fmea, cp], "phase order");
        assert_eq!(ds[1].phase, Some(ph[2].id));
        assert_eq!(ds[1].tool, Some(Tool::Fmea));
        assert_eq!(of_tool(&f.state, f.project, Tool::Fmea).len(), 1);
        for t in Tool::ALL {
            assert!(t.usual_phase() < 5);
            assert_eq!(
                serde_json::from_value::<Tool>(enum_value(t)).unwrap(),
                t,
                "keys round trip"
            );
        }
    }

    #[test]
    fn items_order_nudge_and_delete_subtree() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Ishikawa);
        let a = f.item(d, "causa", None, vec![("texto", "a".into())]);
        let b = f.item(d, "causa", None, vec![("texto", "b".into())]);
        let a1 = f.item(d, "causa", Some(a), vec![]);
        let a11 = f.item(d, "causa", Some(a1), vec![]);
        let texts = |f: &Fixture| -> Vec<Id> {
            items(&f.state, d, "causa")
                .iter()
                .filter(|i| i.parent().is_none())
                .map(|i| i.id)
                .collect()
        };
        assert_eq!(texts(&f), [a, b]);
        let e = nudge_item(&f.state, b, -1);
        f.commit(e);
        assert_eq!(texts(&f), [b, a]);
        assert!(nudge_item(&f.state, b, -1).is_empty(), "already first");
        let e = delete_subtree(&f.state, a);
        assert_eq!(e.len(), 3);
        f.commit(e);
        assert_eq!(items(&f.state, d, "causa").len(), 1);
        assert!(f.state.entity(a11).unwrap().is_deleted());
    }

    #[test]
    fn numbers_typed_by_people() {
        assert_eq!(parse_number("1,5"), Some(1.5));
        assert_eq!(parse_number(" 2 "), Some(2.0));
        assert_eq!(parse_number("1,234.5"), Some(1234.5));
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number("inf"), None);
    }
}
