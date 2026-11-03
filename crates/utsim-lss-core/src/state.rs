//! Reconstruction of the project state from operations.
//!
//! Operations are applied in [`OrderKey`] order. Each field keeps the last value written
//! (last writer wins by the total order) and the id of the operation that wrote it (its
//! *version*). Optimistic locking (`docs/plano_lean_seis_sigma.md`, 4.5): an operation whose
//! `base` is not the current version of the field was written without seeing the current
//! value; the later operation (higher Lamport clock) wins and the pair goes to the list of
//! conflicts to review, with both values, until someone dismisses it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::Id;
use crate::op::{EntityRef, FIELD_DELETED, FIELD_DISMISS_CONFLICT, Kind, Op, OrderKey};

/// The value of a field and who wrote it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldValue {
    #[serde(rename = "valor")]
    pub value: Value,
    /// The operation that wrote it: the field's version.
    pub op: Id,
    #[serde(rename = "autor")]
    pub author: Id,
    #[serde(rename = "relogio_lamport")]
    pub lamport: u64,
    #[serde(rename = "instante")]
    pub at_ms: i64,
}

/// One entity: its kind and its fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    #[serde(rename = "tipo")]
    pub kind: Kind,
    #[serde(rename = "campos", default)]
    pub fields: BTreeMap<String, FieldValue>,
}

impl Entity {
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.fields.get(field).map(|f| &f.value)
    }

    /// The version (id of the last operation) of a field, `None` if never written. This is
    /// what an edit uses as its `base`.
    pub fn version(&self, field: &str) -> Option<Id> {
        self.fields.get(field).map(|f| f.op)
    }

    pub fn is_deleted(&self) -> bool {
        self.get(FIELD_DELETED).and_then(Value::as_bool) == Some(true)
    }

    /// Text field; empty when absent or not a string.
    pub fn text(&self, field: &str) -> String {
        self.get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    pub fn opt_text(&self, field: &str) -> Option<String> {
        self.get(field)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from)
    }

    pub fn number(&self, field: &str) -> Option<f64> {
        self.get(field)
            .and_then(Value::as_f64)
            .filter(|x| x.is_finite())
    }

    pub fn integer(&self, field: &str) -> Option<i64> {
        self.get(field).and_then(Value::as_i64)
    }

    pub fn boolean(&self, field: &str) -> bool {
        self.get(field).and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn id(&self, field: &str) -> Option<Id> {
        self.get(field)
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
    }

    /// A field holding a serde enum or struct; `None` when absent or not understood.
    pub fn typed<T: serde::de::DeserializeOwned>(&self, field: &str) -> Option<T> {
        self.get(field)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Instant of the last change to any field.
    pub fn last_change(&self) -> Option<&FieldValue> {
        self.fields
            .values()
            .max_by_key(|f| (f.lamport, f.author, f.op))
    }
}

/// A write that replaced a value its author had not seen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    #[serde(rename = "entidade")]
    pub entity: EntityRef,
    #[serde(rename = "campo")]
    pub field: String,
    /// The value kept (the later operation).
    #[serde(rename = "mantido")]
    pub kept: FieldValue,
    /// The value replaced, which the author of `kept` had not seen.
    #[serde(rename = "substituido")]
    pub replaced: FieldValue,
}

/// The reconstructed state of a project.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(rename = "entidades", default)]
    pub entities: BTreeMap<Id, Entity>,
    /// Conflicts not yet dismissed, in the order they happened.
    #[serde(rename = "conflitos", default)]
    pub conflicts: Vec<Conflict>,
    /// Operations ignored because they name an existing entity with another kind.
    #[serde(rename = "rejeitadas", default)]
    pub rejected: Vec<Id>,
    /// Key of the last operation applied.
    #[serde(rename = "ultima", default)]
    pub last: Option<OrderKey>,
    /// Number of operations applied.
    #[serde(rename = "operacoes", default)]
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("operação fora de ordem: a reconstrução precisa recomeçar")]
pub struct OutOfOrder;

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds the state from any collection of operations, in any order (duplicates by id
    /// are applied once). The result depends only on the set.
    pub fn replay<'a>(ops: impl IntoIterator<Item = &'a Op>) -> State {
        let mut sorted: BTreeMap<OrderKey, &Op> = BTreeMap::new();
        for op in ops {
            sorted.insert(op.key(), op);
        }
        let mut state = State::new();
        for op in sorted.values() {
            state.apply_in_order(op);
        }
        state
    }

    /// Highest Lamport clock seen; a new operation takes this plus one.
    pub fn max_lamport(&self) -> u64 {
        self.last.map_or(0, |k| k.lamport)
    }

    /// Applies an operation that comes after every one already applied.
    pub fn apply(&mut self, op: &Op) -> Result<(), OutOfOrder> {
        if self.last.is_some_and(|last| op.key() <= last) {
            return Err(OutOfOrder);
        }
        self.apply_in_order(op);
        Ok(())
    }

    fn apply_in_order(&mut self, op: &Op) {
        self.last = Some(op.key());
        self.count += 1;
        if op.field == FIELD_DISMISS_CONFLICT {
            if let Some(replaced) = op.value.as_str().and_then(|s| s.parse::<Id>().ok()) {
                self.conflicts
                    .retain(|c| !(c.entity.id == op.entity.id && c.field_matches_op(replaced)));
            }
            return;
        }
        if op.is_meta() {
            // Unknown meta operation (newer version of the app): counted, no effect.
            return;
        }
        let entity = self.entities.entry(op.entity.id).or_insert_with(|| Entity {
            kind: op.entity.kind,
            fields: BTreeMap::new(),
        });
        if entity.kind != op.entity.kind {
            self.rejected.push(op.id);
            return;
        }
        let new = FieldValue {
            value: op.value.clone(),
            op: op.id,
            author: op.author,
            lamport: op.lamport,
            at_ms: op.at_ms,
        };
        let previous = entity.fields.insert(op.field.clone(), new.clone());
        if let Some(previous) = previous {
            if op.base != Some(previous.op) && previous.value != op.value {
                self.conflicts.push(Conflict {
                    entity: op.entity,
                    field: op.field.clone(),
                    kept: new,
                    replaced: previous,
                });
            }
        }
    }

    pub fn entity(&self, id: Id) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// Entities of one kind that are not deleted, in id order (creation order).
    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = (Id, &Entity)> {
        self.entities
            .iter()
            .filter(move |(_, e)| e.kind == kind && !e.is_deleted())
            .map(|(id, e)| (*id, e))
    }

    /// Turns edits into operations by `author`: consecutive Lamport clocks after the highest
    /// seen and, as base, the version the person saw ([`crate::op::Edit::seen`]) or else the
    /// current version of each field (or the operation of the same batch that wrote it just
    /// before). `next_id` supplies new ids (UUID v7).
    pub fn prepare(
        &self,
        author: Id,
        at_ms: i64,
        edits: impl IntoIterator<Item = crate::op::Edit>,
        mut next_id: impl FnMut() -> Id,
    ) -> Vec<Op> {
        let mut written: BTreeMap<(Id, String), Id> = BTreeMap::new();
        let mut ops = Vec::new();
        for (lamport, edit) in (self.max_lamport() + 1..).zip(edits) {
            let id = next_id();
            let slot = (edit.entity.id, edit.field.clone());
            let base = match edit.seen {
                Some(seen) => seen,
                None => written.get(&slot).copied().or_else(|| {
                    self.entity(edit.entity.id)
                        .and_then(|e| e.version(&edit.field))
                }),
            };
            written.insert(slot, id);
            ops.push(Op {
                id,
                author,
                at_ms,
                lamport,
                entity: edit.entity,
                field: edit.field,
                value: edit.value,
                base,
            });
        }
        ops
    }

    /// Conflicts on one entity.
    pub fn conflicts_of(&self, id: Id) -> impl Iterator<Item = &Conflict> {
        self.conflicts.iter().filter(move |c| c.entity.id == id)
    }
}

impl Conflict {
    /// Identifier used to dismiss the conflict: the replaced operation.
    pub fn key(&self) -> Id {
        self.replaced.op
    }

    fn field_matches_op(&self, replaced: Id) -> bool {
        self.replaced.op == replaced
    }

    /// The edit that dismisses this conflict (keeping the current value).
    pub fn dismiss(&self) -> crate::op::Edit {
        crate::op::Edit::new(
            self.entity,
            FIELD_DISMISS_CONFLICT,
            Value::from(self.replaced.op.to_string()),
        )
    }

    /// The edits that bring back the replaced value and dismiss the conflict.
    pub fn restore(&self) -> Vec<crate::op::Edit> {
        vec![
            crate::op::Edit::new(self.entity, self.field.clone(), self.replaced.value.clone()),
            self.dismiss(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::Edit;

    pub(crate) fn ids() -> impl FnMut() -> Id {
        let mut n = 0u64;
        move || {
            n += 1;
            Id::v7(1_700_000_000_000 + n, [(n % 251) as u8; 10])
        }
    }

    fn person(n: u8) -> Id {
        Id::v7(1, [n; 10])
    }

    #[test]
    fn last_writer_wins_and_versions() {
        let mut next_id = ids();
        let action = EntityRef::new(Kind::Action, next_id());
        let ana = person(1);
        let mut s = State::new();
        let ops = s.prepare(
            ana,
            0,
            [
                Edit::new(action, "titulo", "A"),
                Edit::new(action, "titulo", "B"),
            ],
            &mut next_id,
        );
        assert_eq!(ops[0].base, None);
        assert_eq!(ops[1].base, Some(ops[0].id), "base inside the batch");
        assert_eq!((ops[0].lamport, ops[1].lamport), (1, 2));
        for op in &ops {
            s.apply(op).unwrap();
        }
        let e = s.entity(action.id).unwrap();
        assert_eq!(e.text("titulo"), "B");
        assert_eq!(e.version("titulo"), Some(ops[1].id));
        assert!(s.conflicts.is_empty());
        assert_eq!(s.apply(&ops[0]), Err(OutOfOrder));
    }

    #[test]
    fn concurrent_writes_make_a_conflict_kept_by_higher_clock() {
        let mut next_id = ids();
        let action = EntityRef::new(Kind::Action, next_id());
        let (ana, bia) = (person(1), person(2));
        let mut s = State::new();
        let first = s.prepare(ana, 0, [Edit::new(action, "titulo", "v0")], &mut next_id);
        s.apply(&first[0]).unwrap();
        // Both saw v0 and wrote at the same time.
        let a = s.prepare(ana, 1, [Edit::new(action, "titulo", "Ana")], &mut next_id);
        let b = s.prepare(bia, 1, [Edit::new(action, "titulo", "Bia")], &mut next_id);
        assert_eq!(a[0].lamport, b[0].lamport);
        let all = [first[0].clone(), a[0].clone(), b[0].clone()];
        let s = State::replay(&all);
        let winner = if a[0].key() > b[0].key() {
            "Ana"
        } else {
            "Bia"
        };
        assert_eq!(s.entity(action.id).unwrap().text("titulo"), winner);
        assert_eq!(s.conflicts.len(), 1);
        let c = &s.conflicts[0];
        assert_eq!(c.kept.value, Value::from(winner));
        assert_ne!(c.replaced.value, c.kept.value);

        // Restore the other value: one more op, conflict gone.
        let mut s2 = s.clone();
        let fix = s2.prepare(ana, 2, c.restore(), &mut next_id);
        for op in &fix {
            s2.apply(op).unwrap();
        }
        assert!(s2.conflicts.is_empty());
        assert_eq!(
            s2.entity(action.id).unwrap().get("titulo"),
            Some(&c.replaced.value)
        );
        // Dismiss: value stays.
        let mut s3 = s.clone();
        let fix = s3.prepare(bia, 2, [c.dismiss()], &mut next_id);
        s3.apply(&fix[0]).unwrap();
        assert!(s3.conflicts.is_empty());
        assert_eq!(s3.entity(action.id).unwrap().text("titulo"), winner);
    }

    #[test]
    fn seen_version_reveals_a_change_made_while_typing() {
        let mut next_id = ids();
        let e = EntityRef::new(Kind::Project, next_id());
        let mut s = State::new();
        let v0 = s.prepare(person(1), 0, [Edit::new(e, "meta", "10 %")], &mut next_id);
        s.apply(&v0[0]).unwrap();
        // Ana starts typing (sees v0); Bia's change arrives meanwhile.
        let seen = s.entity(e.id).unwrap().version("meta");
        let bia = s.prepare(person(2), 0, [Edit::new(e, "meta", "8 %")], &mut next_id);
        s.apply(&bia[0]).unwrap();
        let ana = s.prepare(
            person(1),
            0,
            [Edit::new(e, "meta", "5 %").seen(seen)],
            &mut next_id,
        );
        assert_eq!(ana[0].base, Some(v0[0].id));
        s.apply(&ana[0]).unwrap();
        assert_eq!(s.conflicts.len(), 1);
        assert_eq!(s.conflicts[0].replaced.value, Value::from("8 %"));
    }

    #[test]
    fn same_value_is_not_a_conflict() {
        let mut next_id = ids();
        let e = EntityRef::new(Kind::Risk, next_id());
        let s = State::new();
        let a = s.prepare(person(1), 0, [Edit::new(e, "impacto", 3)], &mut next_id);
        let b = s.prepare(person(2), 0, [Edit::new(e, "impacto", 3)], &mut next_id);
        let s = State::replay(a.iter().chain(&b));
        assert!(s.conflicts.is_empty());
    }

    #[test]
    fn kind_mismatch_is_rejected_and_delete_restores() {
        let mut next_id = ids();
        let id = next_id();
        let s = State::new();
        let mut ops = s.prepare(
            person(1),
            0,
            [Edit::new(EntityRef::new(Kind::Risk, id), "descricao", "x")],
            &mut next_id,
        );
        let mut s = State::replay(&ops);
        let bad = s.prepare(
            person(1),
            0,
            [Edit::new(EntityRef::new(Kind::Action, id), "titulo", "y")],
            &mut next_id,
        );
        s.apply(&bad[0]).unwrap();
        assert_eq!(s.rejected, vec![bad[0].id]);
        assert_eq!(s.of_kind(Kind::Risk).count(), 1);
        let del = s.prepare(
            person(1),
            0,
            [Edit::new(
                EntityRef::new(Kind::Risk, id),
                FIELD_DELETED,
                true,
            )],
            &mut next_id,
        );
        s.apply(&del[0]).unwrap();
        assert_eq!(s.of_kind(Kind::Risk).count(), 0);
        ops.extend(bad);
        ops.extend(del);
        assert_eq!(State::replay(ops.iter().rev()), s);
    }

    #[test]
    fn state_round_trips_through_json() {
        let mut next_id = ids();
        let e = EntityRef::new(Kind::Project, next_id());
        let s = State::new();
        let ops = s.prepare(
            person(1),
            5,
            [
                Edit::new(e, "titulo", "Linha 2"),
                Edit::new(e, "meta", 12.5),
            ],
            &mut next_id,
        );
        let s = State::replay(&ops);
        let text = serde_json::to_string(&s).unwrap();
        let back: State = serde_json::from_str(&text).unwrap();
        assert_eq!(back, s);
    }
}
