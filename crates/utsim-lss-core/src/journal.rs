//! The journal: every operation known, in order, and the state they build, fed one
//! operation at a time in whatever order they arrive (the sync client delivers files in
//! any order and with delays).
//!
//! It can start from a [`Snapshot`] (the state up to some operations, written by anyone):
//! the state is then the snapshot plus the operations it does not include. Snapshots are
//! only a speed-up: when an operation arrives that the snapshot should have included (its
//! key is not after the snapshot's last one), the journal answers
//! [`Insert::NeedsFullReplay`] and the caller rebuilds from every operation. The result is
//! always the one of [`State::replay`] over the whole set.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::id::Id;
use crate::op::{Op, OrderKey};
use crate::state::State;

/// The full state up to a set of operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Ids of the operations included, sorted.
    #[serde(rename = "incluidas")]
    pub included: Vec<Id>,
    #[serde(rename = "estado")]
    pub state: State,
}

impl Snapshot {
    /// Key of the last operation included.
    pub fn upto(&self) -> Option<OrderKey> {
        self.state.last
    }

    pub fn len(&self) -> usize {
        self.included.len()
    }

    pub fn is_empty(&self) -> bool {
        self.included.is_empty()
    }

    pub fn includes(&self, id: Id) -> bool {
        self.included.binary_search(&id).is_ok()
    }
}

/// What happened to an inserted operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insert {
    /// After every known operation: applied directly.
    Applied,
    /// Older than some known operation: the state was rebuilt (from the snapshot, if any).
    Refolded,
    /// Already known (same id): ignored.
    Duplicate,
    /// The snapshot in use should have included it: rebuild from all operations.
    NeedsFullReplay,
}

#[derive(Debug, Clone, Default)]
pub struct Journal {
    base: State,
    base_ids: BTreeSet<Id>,
    tail: BTreeMap<OrderKey, Op>,
    tail_ids: BTreeSet<Id>,
    state: State,
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    /// A journal over a whole set of operations (no snapshot).
    pub fn from_ops(ops: impl IntoIterator<Item = Op>) -> Self {
        let mut j = Self::new();
        for op in ops {
            j.push_tail(op);
        }
        j.refold();
        j
    }

    /// Starts from a snapshot and the operations it does not include. Fails (returning the
    /// operations back) if one of them sorts before the snapshot's last operation: then the
    /// snapshot does not describe this set and the caller must replay everything.
    pub fn from_snapshot(snapshot: Snapshot, ops: Vec<Op>) -> Result<Self, Vec<Op>> {
        let upto = snapshot.upto();
        if ops
            .iter()
            .any(|op| !snapshot.includes(op.id) && upto.is_some_and(|u| op.key() <= u))
        {
            return Err(ops);
        }
        let mut j = Self {
            base_ids: snapshot.included.iter().copied().collect(),
            base: snapshot.state,
            ..Self::default()
        };
        for op in ops {
            if !j.base_ids.contains(&op.id) {
                j.push_tail(op);
            }
        }
        j.refold();
        Ok(j)
    }

    fn push_tail(&mut self, op: Op) {
        if self.tail_ids.insert(op.id) {
            self.tail.insert(op.key(), op);
        }
    }

    fn refold(&mut self) {
        let mut state = self.base.clone();
        for op in self.tail.values() {
            // The tail is sorted and after the base, so this cannot fail.
            let _ = state.apply(op);
        }
        self.state = state;
    }

    pub fn contains(&self, id: Id) -> bool {
        self.base_ids.contains(&id) || self.tail_ids.contains(&id)
    }

    /// Adds an operation received from anywhere.
    pub fn insert(&mut self, op: Op) -> Insert {
        if self.contains(op.id) {
            return Insert::Duplicate;
        }
        let key = op.key();
        if self.base.last.is_some_and(|u| key <= u) {
            return Insert::NeedsFullReplay;
        }
        let after_all = self.state.last.is_none_or(|last| key > last);
        if after_all {
            let _ = self.state.apply(&op);
            self.push_tail(op);
            Insert::Applied
        } else {
            self.push_tail(op);
            self.refold();
            Insert::Refolded
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// Number of operations represented (snapshot plus tail).
    pub fn len(&self) -> usize {
        self.base_ids.len() + self.tail_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Operations held individually (those after the snapshot), in order.
    pub fn tail(&self) -> impl Iterator<Item = &Op> {
        self.tail.values()
    }

    pub fn tail_len(&self) -> usize {
        self.tail.len()
    }

    /// A snapshot of the current state (everything known).
    pub fn snapshot(&self) -> Snapshot {
        let mut included: Vec<Id> = self
            .base_ids
            .iter()
            .chain(self.tail_ids.iter())
            .copied()
            .collect();
        included.sort();
        Snapshot {
            included,
            state: self.state.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::{Edit, EntityRef, Kind};

    fn ops() -> Vec<Op> {
        let mut n = 0u64;
        let mut next_id = move || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        };
        let e = EntityRef::new(Kind::Action, next_id());
        let ana = Id::v7(1, [1; 10]);
        let bia = Id::v7(1, [2; 10]);
        let mut s = State::new();
        let mut all = Vec::new();
        for (who, v) in [(ana, "a"), (bia, "b"), (ana, "c")] {
            let o = s.prepare(who, 0, [Edit::new(e, "titulo", v)], &mut next_id);
            s.apply(&o[0]).unwrap();
            all.extend(o);
        }
        // A late operation from Bia based on the first value, with an old clock.
        let mut late = State::replay(&all[..1]).prepare(
            bia,
            0,
            [Edit::new(e, "coluna", "fazendo")],
            &mut next_id,
        );
        all.append(&mut late);
        all
    }

    #[test]
    fn out_of_order_insert_refolds_to_the_replay() {
        let all = ops();
        let mut j = Journal::new();
        let mut outcomes = Vec::new();
        for op in all.iter().rev() {
            outcomes.push(j.insert(op.clone()));
        }
        assert!(outcomes.contains(&Insert::Refolded));
        assert_eq!(j.insert(all[0].clone()), Insert::Duplicate);
        assert_eq!(j.state(), &State::replay(&all));
        assert_eq!(j.len(), all.len());
    }

    #[test]
    fn snapshot_plus_tail_equals_replay() {
        let all = ops();
        let mut sorted = all.clone();
        sorted.sort_by_key(Op::key);
        let snap = Journal::from_ops(sorted[..2].to_vec()).snapshot();
        assert_eq!(snap.len(), 2);
        let j = Journal::from_snapshot(snap.clone(), sorted.clone()).unwrap();
        assert_eq!(j.state(), &State::replay(&all));
        assert_eq!(j.tail_len(), sorted.len() - 2);
        // An operation the snapshot should have included makes it unusable.
        let partial = Journal::from_ops([sorted[0].clone(), sorted[2].clone()]).snapshot();
        assert!(Journal::from_snapshot(partial.clone(), vec![sorted[1].clone()]).is_err());
        let mut j = Journal::from_snapshot(partial, vec![]).unwrap();
        assert_eq!(j.insert(sorted[1].clone()), Insert::NeedsFullReplay);
        assert_eq!(j.insert(sorted[3].clone()), Insert::Applied);
    }

    #[test]
    fn snapshot_serializes() {
        let snap = Journal::from_ops(ops()).snapshot();
        let text = serde_json::to_string(&snap).unwrap();
        assert!(text.contains("\"incluidas\""));
        let back: Snapshot = serde_json::from_str(&text).unwrap();
        assert_eq!(back, snap);
    }
}
