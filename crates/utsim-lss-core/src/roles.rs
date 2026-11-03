//! Roles of a project and the RACI matrix of its deliveries
//! (`docs/plano_lean_seis_sigma.md`, 1.2). The matrix is generated from the roles (who is
//! responsible, accountable, consulted and informed by default) and every cell can be
//! changed; a changed cell is an entity of its own, so two people editing different cells
//! never collide.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::{Role, RoleKind, enum_value, fields as dfields, of_project, phases};
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::State;

pub mod fields {
    pub const PROJECT: &str = "projeto";
    /// Row key of a RACI cell (see [`super::Row`]).
    pub const ROW: &str = "linha";
    pub const ROLE: &str = "papel";
    pub const LETTER: &str = "letra";
}

/// Every role, in the order of section 1.2.
pub const ALL_ROLES: [RoleKind; 9] = [
    RoleKind::Sponsor,
    RoleKind::Champion,
    RoleKind::ProcessOwner,
    RoleKind::MasterBlackBelt,
    RoleKind::BlackBelt,
    RoleKind::GreenBelt,
    RoleKind::YellowBelt,
    RoleKind::Team,
    RoleKind::Controller,
];

/// Black or green belt: the one who conducts the project.
pub fn is_belt(k: RoleKind) -> bool {
    matches!(k, RoleKind::BlackBelt | RoleKind::GreenBelt)
}

/// Roles of a project.
pub fn roles(state: &State, project: Id) -> Vec<Role> {
    let mut v = of_project(state, Kind::Role, project, Role::read);
    v.sort_by_key(|r| r.id);
    v
}

/// People holding `kind` in the project.
pub fn holders(state: &State, project: Id, kind: RoleKind) -> Vec<Id> {
    let mut v: Vec<Id> = roles(state, project)
        .into_iter()
        .filter(|r| r.role == kind)
        .filter_map(|r| r.person)
        .collect();
    v.sort();
    v.dedup();
    v
}

pub fn has_role(state: &State, project: Id, person: Id, kind: RoleKind) -> bool {
    holders(state, project, kind).contains(&person)
}

/// Edits that give `person` the role `kind` (nothing if they already have it).
pub fn add_role(state: &State, project: Id, id: Id, person: Id, kind: RoleKind) -> Vec<Edit> {
    if has_role(state, project, person, kind) {
        return Vec::new();
    }
    let r = EntityRef::new(Kind::Role, id);
    vec![
        Edit::new(r, dfields::role::PROJECT, project.to_string()),
        Edit::new(r, dfields::role::PERSON, person.to_string()),
        Edit::new(r, dfields::role::ROLE, enum_value(kind)),
    ]
}

/// One letter of the RACI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Letter {
    #[serde(rename = "R")]
    Responsible,
    #[serde(rename = "A")]
    Accountable,
    #[serde(rename = "C")]
    Consulted,
    #[serde(rename = "I")]
    Informed,
}

impl Letter {
    pub const ALL: [Letter; 4] = [
        Letter::Responsible,
        Letter::Accountable,
        Letter::Consulted,
        Letter::Informed,
    ];

    pub fn symbol(self) -> &'static str {
        match self {
            Letter::Responsible => "R",
            Letter::Accountable => "A",
            Letter::Consulted => "C",
            Letter::Informed => "I",
        }
    }
}

/// A row of the matrix: a delivery of the project.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Row {
    Charter,
    /// The tollgate of a phase.
    Tollgate(Id),
    Schedule,
    Risks,
    Savings,
    ControlPlan,
    Closure,
}

impl Row {
    /// Stable key written in a cell (`termo`, `tollgate:<id>`...).
    pub fn key(&self) -> String {
        match self {
            Row::Charter => "termo".into(),
            Row::Tollgate(p) => format!("tollgate:{p}"),
            Row::Schedule => "cronograma".into(),
            Row::Risks => "riscos".into(),
            Row::Savings => "ganhos".into(),
            Row::ControlPlan => "plano_de_controle".into(),
            Row::Closure => "encerramento".into(),
        }
    }

    pub fn from_key(s: &str) -> Option<Row> {
        Some(match s {
            "termo" => Row::Charter,
            "cronograma" => Row::Schedule,
            "riscos" => Row::Risks,
            "ganhos" => Row::Savings,
            "plano_de_controle" => Row::ControlPlan,
            "encerramento" => Row::Closure,
            _ => Row::Tollgate(s.strip_prefix("tollgate:")?.parse().ok()?),
        })
    }
}

/// The default letter of a role in a row, from the responsibilities of section 1.2.
pub fn default_letter(row: &Row, role: RoleKind, last_phase: bool) -> Option<Letter> {
    use Letter::*;
    use RoleKind as R;
    let belt = if is_belt(role) {
        Some(Responsible)
    } else {
        None
    };
    match row {
        Row::Charter => match role {
            R::Sponsor => Some(Accountable),
            R::Champion | R::MasterBlackBelt | R::ProcessOwner => Some(Consulted),
            R::YellowBelt | R::Team | R::Controller => Some(Informed),
            _ => belt,
        },
        Row::Tollgate(_) => match role {
            R::Sponsor => Some(Accountable),
            R::Champion | R::MasterBlackBelt => Some(Consulted),
            R::ProcessOwner if last_phase => Some(Consulted),
            R::ProcessOwner | R::YellowBelt | R::Team | R::Controller => Some(Informed),
            _ => belt,
        },
        Row::Schedule | Row::Risks => match role {
            R::Sponsor | R::Champion | R::Controller | R::ProcessOwner => Some(Informed),
            R::MasterBlackBelt | R::YellowBelt | R::Team => Some(Consulted),
            _ => belt.map(|_| Accountable),
        },
        Row::Savings => match role {
            R::Controller => Some(Accountable),
            R::Sponsor | R::Champion => Some(Informed),
            R::ProcessOwner | R::MasterBlackBelt => Some(Consulted),
            R::YellowBelt | R::Team => None,
            _ => belt,
        },
        Row::ControlPlan => match role {
            R::ProcessOwner => Some(Accountable),
            R::Sponsor | R::Champion | R::Controller => Some(Informed),
            R::MasterBlackBelt | R::YellowBelt | R::Team => Some(Consulted),
            _ => belt,
        },
        Row::Closure => match role {
            R::Sponsor => Some(Accountable),
            R::ProcessOwner | R::Controller | R::Champion | R::MasterBlackBelt => Some(Consulted),
            R::YellowBelt | R::Team => Some(Informed),
            _ => belt,
        },
    }
}

/// The RACI of a project: rows, the role columns in use and the letters.
#[derive(Debug, Clone, PartialEq)]
pub struct Raci {
    pub rows: Vec<Row>,
    pub roles: Vec<RoleKind>,
    pub cells: BTreeMap<(Row, RoleKind), Letter>,
    /// Cells changed by hand (the others are the default).
    pub edited: BTreeMap<(Row, RoleKind), Id>,
}

impl Raci {
    pub fn get(&self, row: &Row, role: RoleKind) -> Option<Letter> {
        self.cells.get(&(row.clone(), role)).copied()
    }

    /// Rows without exactly one role marked A (a usual RACI rule).
    pub fn rows_without_single_a(&self) -> Vec<Row> {
        self.rows
            .iter()
            .filter(|r| {
                self.roles
                    .iter()
                    .filter(|k| self.get(r, **k) == Some(Letter::Accountable))
                    .count()
                    != 1
            })
            .cloned()
            .collect()
    }
}

/// Edited cells: `(row, role) → (cell id, letter or none)`. Two people creating the same
/// cell at once give two entities; the one changed last wins.
fn edited_cells(state: &State, project: Id) -> BTreeMap<(Row, RoleKind), (Id, Option<Letter>)> {
    let mut found: BTreeMap<(Row, RoleKind), (crate::op::OrderKey, Id, Option<Letter>)> =
        BTreeMap::new();
    for (id, e) in state.of_kind(Kind::RaciCell) {
        if e.id(fields::PROJECT) != Some(project) {
            continue;
        }
        let (Some(row), Some(role)) = (
            Row::from_key(&e.text(fields::ROW)),
            e.typed::<RoleKind>(fields::ROLE),
        ) else {
            continue;
        };
        let letter: Option<Letter> = e.typed(fields::LETTER);
        let Some(last) = e.last_change() else {
            continue;
        };
        let key = crate::op::OrderKey {
            lamport: last.lamport,
            author: last.author,
            id: last.op,
        };
        let slot = (row, role);
        if found.get(&slot).is_none_or(|(k, _, _)| key > *k) {
            found.insert(slot, (key, id, letter));
        }
    }
    found
        .into_iter()
        .map(|(k, (_, id, l))| (k, (id, l)))
        .collect()
}

/// The matrix of a project: rows (charter, one tollgate per phase, schedule, risks,
/// savings, control plan, closure), the roles held by someone, defaults and edits.
pub fn raci(state: &State, project: Id) -> Raci {
    let ph = phases(state, project);
    let mut rows = vec![Row::Charter];
    rows.extend(ph.iter().map(|p| Row::Tollgate(p.id)));
    rows.extend([
        Row::Schedule,
        Row::Risks,
        Row::Savings,
        Row::ControlPlan,
        Row::Closure,
    ]);
    let held: Vec<RoleKind> = roles(state, project).iter().map(|r| r.role).collect();
    let cols: Vec<RoleKind> = ALL_ROLES
        .iter()
        .copied()
        .filter(|k| held.contains(k))
        .collect();
    let last = ph.last().map(|p| p.id);
    let mut cells = BTreeMap::new();
    for row in &rows {
        let is_last = matches!(row, Row::Tollgate(p) if Some(*p) == last);
        for role in &cols {
            if let Some(l) = default_letter(row, *role, is_last) {
                cells.insert((row.clone(), *role), l);
            }
        }
    }
    let mut edited = BTreeMap::new();
    for ((row, role), (id, letter)) in edited_cells(state, project) {
        edited.insert((row.clone(), role), id);
        match letter {
            Some(l) => cells.insert((row, role), l),
            None => cells.remove(&(row, role)),
        };
    }
    Raci {
        rows,
        roles: cols,
        cells,
        edited,
    }
}

/// Edits that set one cell (`None`: empty). `new_id` is used only when the cell was never
/// edited.
pub fn set_cell(
    state: &State,
    project: Id,
    row: &Row,
    role: RoleKind,
    letter: Option<Letter>,
    new_id: Id,
) -> Vec<Edit> {
    let value = letter.map_or(serde_json::Value::Null, enum_value);
    let existing = edited_cells(state, project)
        .get(&(row.clone(), role))
        .map(|(id, _)| *id);
    match existing {
        Some(id) => vec![Edit::new(
            EntityRef::new(Kind::RaciCell, id),
            fields::LETTER,
            value,
        )],
        None => {
            let r = EntityRef::new(Kind::RaciCell, new_id);
            vec![
                Edit::new(r, fields::PROJECT, project.to_string()),
                Edit::new(r, fields::ROW, row.key()),
                Edit::new(r, fields::ROLE, enum_value(role)),
                Edit::new(r, fields::LETTER, value),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Approach, Project};

    #[test]
    fn raci_defaults_edits_and_rule() {
        let mut n = 0u64;
        let mut ids = || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        };
        let (me, sponsor, p) = (ids(), ids(), ids());
        let mut edits = Project::create(p, "P", Approach::Dmaic, &mut ids);
        let s0 = State::new();
        let mut s = State::replay(&s0.prepare(me, 0, std::mem::take(&mut edits), &mut ids));
        let commit = |s: &mut State, e: Vec<Edit>, ids: &mut dyn FnMut() -> Id| {
            for op in s.prepare(me, 0, e, ids) {
                s.apply(&op).unwrap();
            }
        };
        let e = add_role(&s, p, ids(), me, RoleKind::GreenBelt);
        commit(&mut s, e, &mut ids);
        let e = add_role(&s, p, ids(), sponsor, RoleKind::Sponsor);
        commit(&mut s, e, &mut ids);
        assert!(add_role(&s, p, ids(), me, RoleKind::GreenBelt).is_empty());
        assert_eq!(holders(&s, p, RoleKind::Sponsor), vec![sponsor]);

        let m = raci(&s, p);
        assert_eq!(m.rows.len(), 1 + 5 + 5);
        assert_eq!(m.roles, vec![RoleKind::Sponsor, RoleKind::GreenBelt]);
        assert_eq!(
            m.get(&Row::Charter, RoleKind::Sponsor),
            Some(Letter::Accountable)
        );
        assert_eq!(
            m.get(&Row::Charter, RoleKind::GreenBelt),
            Some(Letter::Responsible)
        );
        // Savings has no A without a controller.
        assert_eq!(
            m.rows_without_single_a(),
            vec![Row::Savings, Row::ControlPlan]
        );

        let e = set_cell(
            &s,
            p,
            &Row::Savings,
            RoleKind::Sponsor,
            Some(Letter::Accountable),
            ids(),
        );
        assert_eq!(e.len(), 4);
        commit(&mut s, e, &mut ids);
        let e = set_cell(
            &s,
            p,
            &Row::Savings,
            RoleKind::Sponsor,
            Some(Letter::Accountable),
            ids(),
        );
        assert_eq!(e.len(), 1, "an edited cell is updated in place");
        let m = raci(&s, p);
        assert_eq!(
            m.get(&Row::Savings, RoleKind::Sponsor),
            Some(Letter::Accountable)
        );
        assert_eq!(m.edited.len(), 1);
        let e = set_cell(&s, p, &Row::Charter, RoleKind::GreenBelt, None, ids());
        commit(&mut s, e, &mut ids);
        assert_eq!(raci(&s, p).get(&Row::Charter, RoleKind::GreenBelt), None);

        let tg = Row::Tollgate(p);
        assert_eq!(Row::from_key(&tg.key()), Some(tg));
        assert_eq!(Row::from_key("ganhos"), Some(Row::Savings));
        assert_eq!(Row::from_key("x"), None);
    }
}
