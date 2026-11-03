//! Rules of the action board added in stage L1 (`docs/plano_lean_seis_sigma.md`, 1.7):
//! work-in-progress limits per column, the effectiveness check before "feito", and cards
//! linked to the entity they came from (a tollgate reservation, a risk, a meeting).
//! The cards themselves are those of [`crate::kanban`].

use std::collections::BTreeMap;

use serde_json::Value;

use crate::date::Date;
use crate::domain::{Action, Column, Origin, Project, enum_value, fields as dfields, of_project};
use crate::id::Id;
use crate::kanban;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::State;

/// Field names.
pub mod fields {
    /// On the project: `limite_wip_<coluna>` (see [`super::wip_field`]).
    pub const WIP_PREFIX: &str = "limite_wip_";
    /// On a card: id of the entity it came from and the kind of that entity.
    pub const LINK: &str = "vinculo";
    pub const LINK_KIND: &str = "vinculo_tipo";
    /// Effectiveness check: criterion, date and whether it was confirmed.
    pub const CHECK_CRITERION: &str = "criterio_eficacia";
    pub const CHECK_DATE: &str = "data_eficacia";
    pub const CHECKED: &str = "eficacia_ok";
}

/// The project field holding the WIP limit of a column (`limite_wip_fazendo`).
pub fn wip_field(c: Column) -> String {
    let key = enum_value(c);
    format!("{}{}", fields::WIP_PREFIX, key.as_str().unwrap_or("x"))
}

/// WIP limit of every column (`None`: no limit). A limit of 0 means no limit too.
pub fn wip_limits(state: &State, project: Id) -> BTreeMap<Column, Option<u32>> {
    let e = state.entity(project);
    Column::ALL
        .iter()
        .map(|c| {
            let lim = e
                .and_then(|e| e.integer(&wip_field(*c)))
                .filter(|n| *n > 0)
                .map(|n| n.min(u32::MAX as i64) as u32);
            (*c, lim)
        })
        .collect()
}

/// Edit that sets (or clears, with `None` or 0) the WIP limit of a column.
pub fn set_wip_limit(project: Id, column: Column, limit: Option<u32>) -> Edit {
    let v = limit.filter(|n| *n > 0).map_or(Value::Null, Value::from);
    Edit::new(Project::entity_ref(project), wip_field(column), v)
}

/// How full a column is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnLoad {
    pub column: Column,
    pub cards: usize,
    pub limit: Option<u32>,
}

impl ColumnLoad {
    /// More cards than the limit (possible after concurrent moves).
    pub fn over(&self) -> bool {
        self.limit.is_some_and(|l| self.cards > l as usize)
    }

    /// Whether one more card fits.
    pub fn has_room(&self) -> bool {
        self.limit.is_none_or(|l| self.cards < l as usize)
    }
}

pub fn loads(state: &State, project: Id) -> Vec<ColumnLoad> {
    let board = kanban::board(state, project);
    let limits = wip_limits(state, project);
    Column::ALL
        .iter()
        .map(|c| ColumnLoad {
            column: *c,
            cards: board.get(c).map_or(0, Vec::len),
            limit: limits.get(c).copied().flatten(),
        })
        .collect()
}

/// Why a card cannot move.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoveBlock {
    #[error("a coluna já tem {cards} cartões e o limite é {limit}")]
    WipFull { limit: u32, cards: usize },
    #[error("o cartão pede a verificação de eficácia antes de ir para \"feito\"")]
    NeedsEffectiveness,
}

/// Whether `card` may enter `to`: the column has room (a card already there may always
/// move inside it) and, for "feito", the effectiveness check is done when one is asked.
pub fn check_move(state: &State, project: Id, card: Id, to: Column) -> Result<(), MoveBlock> {
    let Some(e) = state.entity(card) else {
        return Ok(());
    };
    let current: Column = e.typed(dfields::action::COLUMN).unwrap_or_default();
    if current != to {
        let load = loads(state, project)
            .into_iter()
            .find(|l| l.column == to)
            .expect("every column");
        if !load.has_room() {
            return Err(MoveBlock::WipFull {
                limit: load.limit.unwrap_or(0),
                cards: load.cards,
            });
        }
    }
    if to == Column::Done && needs_effectiveness(state, card) {
        return Err(MoveBlock::NeedsEffectiveness);
    }
    Ok(())
}

/// The card asks for an effectiveness check that was not confirmed.
pub fn needs_effectiveness(state: &State, card: Id) -> bool {
    state.entity(card).is_some_and(|e| {
        !e.text(fields::CHECK_CRITERION).trim().is_empty() && !e.boolean(fields::CHECKED)
    })
}

/// Effectiveness checks due by `today` and not confirmed.
pub fn effectiveness_due(state: &State, project: Id, today: Date) -> Vec<Action> {
    of_project(state, Kind::Action, project, |id, e| (id, e.clone()))
        .into_iter()
        .filter(|(id, e)| {
            needs_effectiveness(state, *id)
                && e.opt_text(fields::CHECK_DATE)
                    .and_then(|d| d.parse::<Date>().ok())
                    .is_some_and(|d| d <= today)
        })
        .map(|(id, e)| Action::read(id, &e))
        .collect()
}

/// What a new linked card carries.
#[derive(Debug, Clone, Default)]
pub struct CardSpec {
    pub title: String,
    pub why: String,
    pub who: Option<Id>,
    pub due: Option<Date>,
    pub phase: Option<Id>,
}

/// Edits that create a card at the end of "a fazer", with its origin and the entity it came
/// from. The WIP limit of "a fazer" is not enforced here: cards born from a decision must
/// not be lost.
pub fn linked_card(
    state: &State,
    project: Id,
    id: Id,
    spec: &CardSpec,
    origin: Origin,
    link: EntityRef,
) -> Vec<Edit> {
    use dfields::action as f;
    let r = Action::entity_ref(id);
    let mut edits = kanban::new_card(state, project, id, spec.title.trim(), Column::Todo);
    edits.retain(|e| e.field != f::ORIGIN);
    edits.push(Edit::new(r, f::ORIGIN, enum_value(origin)));
    edits.push(Edit::new(r, fields::LINK, link.id.to_string()));
    edits.push(Edit::new(r, fields::LINK_KIND, link.kind.key()));
    if !spec.why.trim().is_empty() {
        edits.push(Edit::new(r, f::WHY, spec.why.trim()));
    }
    if let Some(w) = spec.who {
        edits.push(Edit::new(r, f::WHO, w.to_string()));
    }
    if let Some(d) = spec.due {
        edits.push(Edit::new(r, f::DUE, d.to_string()));
    }
    if let Some(p) = spec.phase {
        edits.push(Edit::new(r, f::PHASE, p.to_string()));
    }
    edits
}

/// Cards linked to an entity.
pub fn cards_linked_to(state: &State, project: Id, entity: Id) -> Vec<Action> {
    let key = entity.to_string();
    of_project(state, Kind::Action, project, |id, e| (id, e.clone()))
        .into_iter()
        .filter(|(_, e)| e.text(fields::LINK) == key)
        .map(|(id, e)| Action::read(id, &e))
        .collect()
}

/// The entity a card came from.
pub fn link_of(state: &State, card: Id) -> Option<EntityRef> {
    let e = state.entity(card)?;
    let id = e.id(fields::LINK)?;
    let kind = state.entity(id).map(|x| x.kind)?;
    Some(EntityRef::new(kind, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Approach;

    fn setup() -> (State, Id, Id, impl FnMut() -> Id) {
        let mut n = 0u64;
        let mut ids = move || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        };
        let me = ids();
        let p = ids();
        let s = State::new();
        let ops = s.prepare(
            me,
            0,
            Project::create(p, "P", Approach::Dmaic, &mut ids),
            &mut ids,
        );
        (State::replay(&ops), me, p, ids)
    }

    fn commit(s: &mut State, me: Id, edits: Vec<Edit>, ids: &mut impl FnMut() -> Id) {
        for op in s.prepare(me, 0, edits, ids) {
            s.apply(&op).unwrap();
        }
    }

    #[test]
    fn wip_limit_blocks_entering_a_full_column() {
        let (mut s, me, p, mut ids) = setup();
        commit(
            &mut s,
            me,
            vec![set_wip_limit(p, Column::Doing, Some(1))],
            &mut ids,
        );
        assert_eq!(wip_limits(&s, p)[&Column::Doing], Some(1));
        let (a, b) = (ids(), ids());
        let e = kanban::new_card(&s, p, a, "A", Column::Todo);
        commit(&mut s, me, e, &mut ids);
        let e = kanban::new_card(&s, p, b, "B", Column::Todo);
        commit(&mut s, me, e, &mut ids);
        assert!(check_move(&s, p, a, Column::Doing).is_ok());
        let e = kanban::move_card(&s, p, a, Column::Doing, 0);
        commit(&mut s, me, e, &mut ids);
        assert_eq!(
            check_move(&s, p, b, Column::Doing),
            Err(MoveBlock::WipFull { limit: 1, cards: 1 })
        );
        // Moving inside the full column is fine.
        assert!(check_move(&s, p, a, Column::Doing).is_ok());
        let l = loads(&s, p);
        assert!(!l[1].has_room() && !l[1].over());
        // Clearing the limit.
        commit(
            &mut s,
            me,
            vec![set_wip_limit(p, Column::Doing, Some(0))],
            &mut ids,
        );
        assert!(check_move(&s, p, b, Column::Doing).is_ok());
        assert_eq!(wip_field(Column::Doing), "limite_wip_fazendo");
    }

    #[test]
    fn effectiveness_check_before_done() {
        let (mut s, me, p, mut ids) = setup();
        let a = ids();
        let mut e = kanban::new_card(&s, p, a, "A", Column::Todo);
        let r = Action::entity_ref(a);
        e.push(Edit::new(
            r,
            fields::CHECK_CRITERION,
            "refugo < 2 % por 4 semanas",
        ));
        e.push(Edit::new(r, fields::CHECK_DATE, "2026-12-01"));
        commit(&mut s, me, e, &mut ids);
        assert_eq!(
            check_move(&s, p, a, Column::Done),
            Err(MoveBlock::NeedsEffectiveness)
        );
        let today = Date::new(2026, 12, 2).unwrap();
        assert_eq!(effectiveness_due(&s, p, today).len(), 1);
        commit(
            &mut s,
            me,
            vec![Edit::new(r, fields::CHECKED, true)],
            &mut ids,
        );
        assert!(check_move(&s, p, a, Column::Done).is_ok());
        assert!(effectiveness_due(&s, p, today).is_empty());
    }

    #[test]
    fn linked_cards_remember_their_origin() {
        let (mut s, me, p, mut ids) = setup();
        let risk = EntityRef::new(Kind::Risk, ids());
        commit(
            &mut s,
            me,
            vec![
                Edit::new(risk, "projeto", p.to_string()),
                Edit::new(risk, "descricao", "Fornecedor atrasa"),
            ],
            &mut ids,
        );
        let c = ids();
        let spec = CardSpec {
            title: " Qualificar segundo fornecedor ".into(),
            who: Some(me),
            due: Date::new(2026, 12, 1),
            ..CardSpec::default()
        };
        let e = linked_card(&s, p, c, &spec, Origin::Risk, risk);
        commit(&mut s, me, e, &mut ids);
        let cards = cards_linked_to(&s, p, risk.id);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, "Qualificar segundo fornecedor");
        assert_eq!(cards[0].origin, Origin::Risk);
        assert_eq!(cards[0].who, Some(me));
        assert_eq!(link_of(&s, c), Some(risk));
    }
}
