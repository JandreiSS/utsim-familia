//! Presence (`docs/plano_lean_seis_sigma.md`, 4.5): each person writes only their own
//! `presenca/<pessoa>.json` with a heartbeat; the others see "Ana está editando a ação X".
//! It is a warning, never a lock.

use serde::{Deserialize, Serialize};

use crate::id::Id;
use crate::op::EntityRef;

/// Heartbeat interval the app uses, milliseconds.
pub const HEARTBEAT_MS: i64 = 20_000;

/// A presence older than this is ignored (the person closed the app or lost the sync).
pub const STALE_MS: i64 = 3 * 60_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    #[serde(rename = "pessoa")]
    pub person: Id,
    #[serde(rename = "nome")]
    pub name: String,
    /// Screen key (`acoes`, `termo`...).
    #[serde(rename = "tela")]
    pub screen: String,
    /// What the person is editing, if anything.
    #[serde(rename = "entidade", default, skip_serializing_if = "Option::is_none")]
    pub editing: Option<EntityRef>,
    #[serde(rename = "instante")]
    pub at_ms: i64,
}

impl Presence {
    pub fn is_fresh(&self, now_ms: i64) -> bool {
        now_ms - self.at_ms <= STALE_MS
    }
}

/// The fresh presences of other people, one per person (the latest), sorted by name.
pub fn others(all: &[Presence], me: Id, now_ms: i64) -> Vec<Presence> {
    let mut latest: std::collections::BTreeMap<Id, &Presence> = Default::default();
    for p in all.iter().filter(|p| p.person != me && p.is_fresh(now_ms)) {
        let slot = latest.entry(p.person).or_insert(p);
        if p.at_ms > slot.at_ms {
            *slot = p;
        }
    }
    let mut v: Vec<Presence> = latest.into_values().cloned().collect();
    v.sort_by(|a, b| a.name.cmp(&b.name).then(a.person.cmp(&b.person)));
    v
}

/// Others editing the same entity.
pub fn editing(others: &[Presence], entity: EntityRef) -> impl Iterator<Item = &Presence> {
    others.iter().filter(move |p| p.editing == Some(entity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::Kind;

    fn p(n: u8, name: &str, at: i64, editing: Option<EntityRef>) -> Presence {
        Presence {
            person: Id::v7(1, [n; 10]),
            name: name.into(),
            screen: "acoes".into(),
            editing,
            at_ms: at,
        }
    }

    #[test]
    fn filters_me_stale_and_duplicates() {
        let card = EntityRef::new(Kind::Action, Id::v7(9, [9; 10]));
        let now = 1_000_000;
        let all = vec![
            p(1, "Eu", now, None),
            p(2, "Bia", now - 10_000, None),
            p(2, "Bia", now - 5_000, Some(card)),
            p(3, "Caio", now - STALE_MS - 1, Some(card)),
            p(4, "Ana", now, None),
        ];
        let me = Id::v7(1, [1; 10]);
        let o = others(&all, me, now);
        let names: Vec<_> = o.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Ana", "Bia"]);
        let ed: Vec<_> = editing(&o, card).map(|p| p.name.as_str()).collect();
        assert_eq!(ed, ["Bia"]);
        let json = serde_json::to_string(&all[0]).unwrap();
        assert!(json.contains("\"tela\"") && !json.contains("entidade"));
    }
}
