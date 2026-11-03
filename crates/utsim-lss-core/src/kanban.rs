//! The action board (`docs/plano_lean_seis_sigma.md`, 1.7): cards per column, in order, and
//! the edits that create and move cards. The order within a column is a fractional index
//! (`ordem`), so moving one card writes one field and two people moving different cards do
//! not collide.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::domain::{Action, Column, Origin, enum_value, fields, of_project};
use crate::id::Id;
use crate::op::{Edit, Kind};
use crate::state::State;

/// Cards of a project's board, per column, sorted by `ordem` (then id).
pub fn board(state: &State, project: Id) -> BTreeMap<Column, Vec<Action>> {
    let mut b: BTreeMap<Column, Vec<Action>> =
        Column::ALL.iter().map(|c| (*c, Vec::new())).collect();
    for a in of_project(state, Kind::Action, project, Action::read) {
        b.entry(a.column).or_default().push(a);
    }
    for cards in b.values_mut() {
        cards.sort_by(|x, y| x.order.total_cmp(&y.order).then(x.id.cmp(&y.id)));
    }
    b
}

/// Order value for a card placed at `index` among `cards` (which must not contain it).
fn order_at(cards: &[&Action], index: usize) -> f64 {
    let index = index.min(cards.len());
    let before = index.checked_sub(1).map(|i| cards[i].order);
    let after = cards.get(index).map(|c| c.order);
    match (before, after) {
        (None, None) => 1.0,
        (Some(b), None) => b + 1.0,
        (None, Some(a)) => a - 1.0,
        (Some(b), Some(a)) => {
            let mid = (b + a) / 2.0;
            // Equal neighbours (concurrent moves): fall just after `before`.
            if mid > b && mid < a {
                mid
            } else {
                b + f64::EPSILON * b.abs().max(1.0)
            }
        }
    }
}

/// Edits that create a card at the end of `column`.
pub fn new_card(state: &State, project: Id, id: Id, title: &str, column: Column) -> Vec<Edit> {
    use fields::action as f;
    let b = board(state, project);
    let cards: Vec<&Action> = b
        .get(&column)
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    let order = order_at(&cards, cards.len());
    let r = Action::entity_ref(id);
    vec![
        Edit::new(r, f::PROJECT, project.to_string()),
        Edit::new(r, f::TITLE, title),
        Edit::new(r, f::COLUMN, enum_value(column)),
        Edit::new(r, f::ORDER, order),
        Edit::new(r, f::ORIGIN, enum_value(Origin::Manual)),
    ]
}

/// Edits that move a card to `column` at position `index` (0 = top). Returns nothing if
/// the card does not exist or would not move.
pub fn move_card(state: &State, project: Id, card: Id, column: Column, index: usize) -> Vec<Edit> {
    use fields::action as f;
    let b = board(state, project);
    let Some(current) = b.values().flatten().find(|a| a.id == card) else {
        return Vec::new();
    };
    let cards: Vec<&Action> = b
        .get(&column)
        .map(|v| v.iter().filter(|a| a.id != card).collect())
        .unwrap_or_default();
    let index = index.min(cards.len());
    let same_place =
        current.column == column && b[&column].iter().position(|a| a.id == card) == Some(index);
    if same_place {
        return Vec::new();
    }
    let r = Action::entity_ref(card);
    let mut edits = Vec::new();
    if current.column != column {
        edits.push(Edit::new(r, f::COLUMN, enum_value(column)));
    }
    edits.push(Edit::new(r, f::ORDER, Value::from(order_at(&cards, index))));
    edits
}

/// Edits that change one text field of a card (title, 5W2H fields, due date).
pub fn set_text(card: Id, field: &str, text: &str) -> Edit {
    Edit::new(Action::entity_ref(card), field, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Approach, Project};

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

    fn titles(s: &State, p: Id, c: Column) -> Vec<String> {
        board(s, p)[&c].iter().map(|a| a.title.clone()).collect()
    }

    #[test]
    fn create_and_move_cards() {
        let (mut s, me, p, mut ids) = setup();
        for t in ["A", "B", "C"] {
            let id = ids();
            let e = new_card(&s, p, id, t, Column::Todo);
            commit(&mut s, me, e, &mut ids);
        }
        assert_eq!(titles(&s, p, Column::Todo), ["A", "B", "C"]);
        let c = board(&s, p)[&Column::Todo][2].id;
        let e = move_card(&s, p, c, Column::Todo, 0);
        assert_eq!(e.len(), 1, "same column: only the order changes");
        commit(&mut s, me, e, &mut ids);
        assert_eq!(titles(&s, p, Column::Todo), ["C", "A", "B"]);
        let a = board(&s, p)[&Column::Todo][1].id;
        let e = move_card(&s, p, a, Column::Doing, 5);
        commit(&mut s, me, e, &mut ids);
        assert_eq!(titles(&s, p, Column::Todo), ["C", "B"]);
        assert_eq!(titles(&s, p, Column::Doing), ["A"]);
        // Between two cards.
        let e = move_card(&s, p, a, Column::Todo, 1);
        commit(&mut s, me, e, &mut ids);
        assert_eq!(titles(&s, p, Column::Todo), ["C", "A", "B"]);
        assert!(
            move_card(&s, p, a, Column::Todo, 1).is_empty(),
            "no-op move"
        );
        assert!(
            move_card(&s, p, ids(), Column::Done, 0).is_empty(),
            "unknown card"
        );
    }

    #[test]
    fn order_between_equal_neighbours_stays_after() {
        let mut n = 0u64;
        let mut ids = || {
            n += 1;
            Id::v7(n, [0; 10])
        };
        let s = State::new();
        let mk = |id: Id, o: f64| {
            let r = Action::entity_ref(id);
            vec![Edit::new(r, fields::action::ORDER, o)]
        };
        let (a, b) = (ids(), ids());
        let mut edits = mk(a, 2.0);
        edits.extend(mk(b, 2.0));
        let s = State::replay(&s.prepare(ids(), 0, edits, &mut ids));
        let ea = Action::read(a, s.entity(a).unwrap());
        let eb = Action::read(b, s.entity(b).unwrap());
        let o = order_at(&[&ea, &eb], 1);
        assert!(o > 2.0);
    }
}
