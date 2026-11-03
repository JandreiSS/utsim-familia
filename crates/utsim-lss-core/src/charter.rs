//! The versioned project charter (`docs/plano_lean_seis_sigma.md`, 1.3).
//!
//! The charter fields live on the project entity and are edited freely. A *version* is a
//! frozen copy of them (`versao_termo`), numbered; the sponsor approves a version. Once a
//! version is approved, changing the goal or the scope makes the charter "needs a new
//! approval" until a newer version is approved. Approving a version also freezes the
//! schedule baseline (section 1.6).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::digest::{self, Change};
use crate::domain::{Project, RoleKind, enum_value, fields as dfields};
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::roles;
use crate::state::State;

pub mod fields {
    /// New charter fields of stage L1, on the project.
    pub const Y: &str = "y_projeto";
    pub const ASSUMPTIONS: &str = "premissas";
    pub const CONSTRAINTS: &str = "restricoes";
    pub const INITIAL_RISKS: &str = "riscos_iniciais";
    /// Charter version entity.
    pub const PROJECT: &str = "projeto";
    pub const NUMBER: &str = "numero";
    pub const CONTENT: &str = "conteudo";
    pub const STATUS: &str = "estado";
    pub const REASON: &str = "motivo";
    pub const APPROVED_BY: &str = "aprovada_por";
    pub const APPROVED_ON: &str = "aprovada_em";
}

/// The charter fields, in screen order.
pub const CHARTER_FIELDS: [&str; 15] = [
    dfields::project::TITLE,
    dfields::project::PROCESS,
    dfields::project::AREA,
    dfields::project::PROBLEM,
    fields::Y,
    dfields::project::GOAL,
    dfields::project::BASELINE,
    dfields::project::DEADLINE,
    dfields::project::SCOPE_IN,
    dfields::project::SCOPE_OUT,
    dfields::project::BUSINESS_CASE,
    fields::ASSUMPTIONS,
    fields::CONSTRAINTS,
    fields::INITIAL_RISKS,
    crate::schedule::fields::START,
];

/// Fields whose change after approval needs a new approval (goal and scope, with the
/// deadline that belongs to the SMART goal).
pub const MATERIAL_FIELDS: [&str; 4] = [
    dfields::project::GOAL,
    dfields::project::DEADLINE,
    dfields::project::SCOPE_IN,
    dfields::project::SCOPE_OUT,
];

/// Fields the tollgate check "termo completo" asks for.
pub const REQUIRED_FIELDS: [&str; 6] = [
    dfields::project::PROBLEM,
    dfields::project::GOAL,
    dfields::project::BASELINE,
    dfields::project::DEADLINE,
    dfields::project::SCOPE_IN,
    dfields::project::BUSINESS_CASE,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VersionStatus {
    #[default]
    #[serde(rename = "registrada")]
    Recorded,
    #[serde(rename = "aprovada")]
    Approved,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharterVersion {
    pub id: Id,
    pub number: i64,
    pub content: BTreeMap<String, Value>,
    pub status: VersionStatus,
    pub reason: String,
    pub approved_by: Option<Id>,
    pub approved_on: Option<Date>,
    pub author: Option<Id>,
    pub at_ms: i64,
}

impl CharterVersion {
    fn read(id: Id, e: &crate::state::Entity) -> Self {
        let created = e.fields.get(fields::NUMBER);
        Self {
            id,
            number: e.integer(fields::NUMBER).unwrap_or(0),
            content: digest::as_map(e.get(fields::CONTENT)),
            status: e.typed(fields::STATUS).unwrap_or_default(),
            reason: e.text(fields::REASON),
            approved_by: e.id(fields::APPROVED_BY),
            approved_on: e.opt_text(fields::APPROVED_ON).and_then(|s| s.parse().ok()),
            author: created.map(|f| f.author),
            at_ms: created.map_or(0, |f| f.at_ms),
        }
    }
}

/// The charter as it is now: field → text (empty fields left out).
pub fn current(state: &State, project: Id) -> BTreeMap<String, Value> {
    let Some(e) = state.entity(project) else {
        return BTreeMap::new();
    };
    CHARTER_FIELDS
        .iter()
        .filter_map(|f| {
            let v = e.text(f);
            (!v.trim().is_empty()).then(|| (f.to_string(), Value::from(v)))
        })
        .collect()
}

/// Versions, oldest first (by number, then id: two versions numbered at once both stay).
pub fn versions(state: &State, project: Id) -> Vec<CharterVersion> {
    let mut v =
        crate::domain::of_project(state, Kind::CharterVersion, project, CharterVersion::read);
    v.sort_by_key(|x| (x.number, x.id));
    v
}

pub fn latest_approved(state: &State, project: Id) -> Option<CharterVersion> {
    versions(state, project)
        .into_iter()
        .rfind(|v| v.status == VersionStatus::Approved)
}

/// Differences between two versions (or a version and the current charter).
pub fn diff(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Vec<Change> {
    digest::diff(before, after)
}

/// Where the charter stands.
#[derive(Debug, Clone, PartialEq)]
pub enum CharterStatus {
    /// No version recorded yet.
    NoVersion,
    /// Versions recorded, none approved.
    NotApproved,
    /// The current charter is the approved one (or differs only in non-material fields:
    /// the list).
    Approved { minor_changes: Vec<Change> },
    /// Goal, deadline or scope changed after the approval: a new version must be approved.
    NeedsReapproval { changes: Vec<Change> },
}

pub fn status(state: &State, project: Id) -> CharterStatus {
    let all = versions(state, project);
    if all.is_empty() {
        return CharterStatus::NoVersion;
    }
    let Some(approved) = latest_approved(state, project) else {
        return CharterStatus::NotApproved;
    };
    let changes = diff(&approved.content, &current(state, project));
    let (material, minor): (Vec<Change>, Vec<Change>) = changes
        .into_iter()
        .partition(|c| MATERIAL_FIELDS.contains(&c.key.as_str()));
    if material.is_empty() {
        CharterStatus::Approved {
            minor_changes: minor,
        }
    } else {
        CharterStatus::NeedsReapproval { changes: material }
    }
}

/// Edits that record a new version with the current charter.
pub fn new_version(state: &State, project: Id, id: Id, reason: &str) -> Vec<Edit> {
    let number = versions(state, project)
        .iter()
        .map(|v| v.number)
        .max()
        .unwrap_or(0)
        + 1;
    let r = EntityRef::new(Kind::CharterVersion, id);
    vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::NUMBER, number),
        Edit::new(
            r,
            fields::CONTENT,
            digest::to_value(&current(state, project)),
        ),
        Edit::new(r, fields::REASON, reason.trim()),
        Edit::new(r, fields::STATUS, enum_value(VersionStatus::Recorded)),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CharterError {
    #[error("versão do termo não encontrada")]
    NoVersion,
    #[error("só o patrocinador do projeto aprova o termo; defina o patrocinador em Papéis")]
    NotSponsor,
    #[error("esta versão já está aprovada")]
    AlreadyApproved,
    #[error("há uma versão mais nova do termo; aprove a mais nova")]
    NotLatest,
}

/// Edits that approve a version. Only a sponsor of the project approves, and only the
/// latest version. The schedule baseline is frozen with the approval.
pub fn approve(
    state: &State,
    project: Id,
    version: Id,
    by: Id,
    today: Date,
) -> Result<Vec<Edit>, CharterError> {
    let all = versions(state, project);
    let v = all
        .iter()
        .find(|v| v.id == version)
        .ok_or(CharterError::NoVersion)?;
    if v.status == VersionStatus::Approved {
        return Err(CharterError::AlreadyApproved);
    }
    if all.last().map(|l| l.id) != Some(version) {
        return Err(CharterError::NotLatest);
    }
    if !roles::has_role(state, project, by, RoleKind::Sponsor) {
        return Err(CharterError::NotSponsor);
    }
    let r = EntityRef::new(Kind::CharterVersion, version);
    let mut edits = vec![
        Edit::new(r, fields::STATUS, enum_value(VersionStatus::Approved)),
        Edit::new(r, fields::APPROVED_BY, by.to_string()),
        Edit::new(r, fields::APPROVED_ON, today.to_string()),
    ];
    edits.extend(crate::schedule::freeze_baseline(state, project, today));
    Ok(edits)
}

/// Whether the required fields are filled (tollgate check "termo completo"): the missing ones.
pub fn missing_fields(state: &State, project: Id) -> Vec<&'static str> {
    let cur = current(state, project);
    REQUIRED_FIELDS
        .iter()
        .copied()
        .filter(|f| !cur.contains_key(*f))
        .collect()
}

/// The project ref, for the screens.
pub fn project_ref(project: Id) -> EntityRef {
    Project::entity_ref(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Approach;

    struct T {
        s: State,
        n: u64,
        me: Id,
        p: Id,
    }

    impl T {
        fn new() -> Self {
            let mut t = T {
                s: State::new(),
                n: 0,
                me: Id::NIL,
                p: Id::NIL,
            };
            t.me = t.id();
            t.p = t.id();
            let e = Project::create(t.p, "Refugo", Approach::Dmaic, || {
                t.n += 1;
                Id::v7(t.n, [t.n as u8; 10])
            });
            t.commit(e);
            t
        }
        fn id(&mut self) -> Id {
            self.n += 1;
            Id::v7(self.n, [self.n as u8; 10])
        }
        fn commit(&mut self, e: Vec<Edit>) {
            let mut n = self.n;
            let ops = self.s.prepare(self.me, 0, e, || {
                n += 1;
                Id::v7(n, [n as u8; 10])
            });
            self.n = n;
            for op in ops {
                self.s.apply(&op).unwrap();
            }
        }
        fn set(&mut self, f: &str, v: &str) {
            let p = Project::entity_ref(self.p);
            self.commit(vec![Edit::new(p, f, v)]);
        }
    }

    #[test]
    fn versions_approval_and_reapproval() {
        let mut t = T::new();
        let today = Date::new(2026, 11, 3).unwrap();
        assert_eq!(status(&t.s, t.p), CharterStatus::NoVersion);
        assert_eq!(missing_fields(&t.s, t.p).len(), 6);
        t.set(dfields::project::GOAL, "refugo de 6 % para 2 %");
        t.set(dfields::project::SCOPE_IN, "linha 2");
        let v1 = t.id();
        let e = new_version(&t.s, t.p, v1, "primeira");
        t.commit(e);
        assert_eq!(status(&t.s, t.p), CharterStatus::NotApproved);
        // Only the sponsor approves.
        assert_eq!(
            approve(&t.s, t.p, v1, t.me, today),
            Err(CharterError::NotSponsor)
        );
        let rid = t.id();
        let e = roles::add_role(&t.s, t.p, rid, t.me, RoleKind::Sponsor);
        t.commit(e);
        let e = approve(&t.s, t.p, v1, t.me, today).unwrap();
        t.commit(e);
        assert_eq!(
            approve(&t.s, t.p, v1, t.me, today),
            Err(CharterError::AlreadyApproved)
        );
        assert!(
            matches!(status(&t.s, t.p), CharterStatus::Approved { ref minor_changes } if minor_changes.is_empty())
        );
        // A minor change keeps it approved; a goal change asks for a new approval.
        t.set(dfields::project::AREA, "Estamparia");
        assert!(
            matches!(status(&t.s, t.p), CharterStatus::Approved { ref minor_changes } if minor_changes.len() == 1)
        );
        t.set(dfields::project::GOAL, "refugo de 6 % para 3 %");
        let CharterStatus::NeedsReapproval { changes } = status(&t.s, t.p) else {
            panic!("needs reapproval");
        };
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].key, dfields::project::GOAL);
        assert_eq!(
            changes[0].before,
            Some(Value::from("refugo de 6 % para 2 %"))
        );
        let v2 = t.id();
        let e = new_version(&t.s, t.p, v2, "meta revista");
        t.commit(e);
        assert_eq!(
            approve(&t.s, t.p, v1, t.me, today),
            Err(CharterError::AlreadyApproved)
        );
        let vs = versions(&t.s, t.p);
        assert_eq!(vs.iter().map(|v| v.number).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(diff(&vs[0].content, &vs[1].content).len(), 2);
        let e = approve(&t.s, t.p, v2, t.me, today).unwrap();
        t.commit(e);
        assert!(matches!(status(&t.s, t.p), CharterStatus::Approved { .. }));
        assert_eq!(latest_approved(&t.s, t.p).unwrap().number, 2);
        assert_eq!(latest_approved(&t.s, t.p).unwrap().approved_on, Some(today));
    }
}
