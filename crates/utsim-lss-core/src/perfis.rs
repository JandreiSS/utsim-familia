//! Profiles and permissions without a server (`docs/plano_lean_seis_sigma.md`, 8.8;
//! decision MB2).
//!
//! Eight profiles: MBB, champion, sponsor, belt, controller, instructor, leadership and
//! student. The program's role list (`perfil` entities of the program journal) is kept by
//! the MBB; the creator of the program folder is an MBB from the start. Two layers:
//!
//! 1. **In the app**: an action outside the person's profiles is refused with a clear
//!    message ([`check`], [`check_edits`]); and the reconstruction validates every
//!    operation against its author's profiles **at that instant** of the journal
//!    ([`replay`]): an operation of an author without the profile has no effect and is
//!    listed as "não autorizada" (LSS-RNF-13). A role given later does not make earlier
//!    operations valid; a role removed stops the later ones.
//! 2. **On the Drive**: what is confidential lives in folders of its own shared only with
//!    who may read them (the coaching log of each belt, the financial values before
//!    validation...). That is the real barrier; the app does not use the Drive's API, so
//!    the manual explains how to set the sharing. The signed role list (ed25519, P7) will
//!    detect a forged role; until then a person who edits the files by hand can write an
//!    operation, but the app ignores it unless its author holds the profile.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{enum_value, fields as dfields};
use crate::id::Id;
use crate::mentoria;
use crate::op::{Edit, EntityRef, Kind, Op, OrderKey};
use crate::programa::{self, fields as pf};
use crate::state::State;

pub mod fields {
    pub const PROGRAM: &str = "programa";
    pub const PERSON: &str = "pessoa";
    pub const PROFILE: &str = "perfil";
    /// Scope of a champion or sponsor: one unit (absent: every unit).
    pub const UNIT: &str = "unidade";
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Profile {
    #[serde(rename = "mbb")]
    Mbb,
    #[serde(rename = "champion")]
    Champion,
    #[serde(rename = "patrocinador")]
    Sponsor,
    #[default]
    #[serde(rename = "belt")]
    Belt,
    #[serde(rename = "controladoria")]
    Controller,
    #[serde(rename = "instrutor")]
    Instructor,
    #[serde(rename = "lideranca")]
    Leadership,
    #[serde(rename = "aluno")]
    Student,
}

impl Profile {
    pub const ALL: [Profile; 8] = [
        Profile::Mbb,
        Profile::Champion,
        Profile::Sponsor,
        Profile::Belt,
        Profile::Controller,
        Profile::Instructor,
        Profile::Leadership,
        Profile::Student,
    ];

    /// The name in the messages of the rules (the screens use the catalogue).
    pub fn label(self) -> &'static str {
        match self {
            Profile::Mbb => "MBB",
            Profile::Champion => "champion",
            Profile::Sponsor => "patrocinador",
            Profile::Belt => "belt",
            Profile::Controller => "controladoria",
            Profile::Instructor => "instrutor",
            Profile::Leadership => "liderança",
            Profile::Student => "aluno",
        }
    }
}

/// A profile given to a person.
#[derive(Debug, Clone, PartialEq)]
pub struct Grant {
    pub id: Id,
    pub person: Option<Id>,
    pub profile: Profile,
    pub unit: Option<Id>,
}

pub fn grants(state: &State) -> Vec<Grant> {
    let mut v: Vec<Grant> = state
        .of_kind(Kind::ProgramRole)
        .map(|(id, e)| Grant {
            id,
            person: e.id(fields::PERSON),
            profile: e.typed(fields::PROFILE).unwrap_or_default(),
            unit: e.id(fields::UNIT),
        })
        .collect();
    v.sort_by_key(|g| g.id);
    v
}

/// Edits that give a profile (the profile first: the permission check reads it).
pub fn grant(program: Id, id: Id, person: Id, profile: Profile, unit: Option<Id>) -> Vec<Edit> {
    let r = EntityRef::new(Kind::ProgramRole, id);
    let mut e = vec![
        Edit::new(r, fields::PROFILE, enum_value(profile)),
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::PERSON, person.to_string()),
    ];
    if let Some(u) = unit {
        e.push(Edit::new(r, fields::UNIT, u.to_string()));
    }
    e
}

/// The profiles of a person, each with its unit scope. The program's creator (`founder`)
/// is always an MBB.
pub fn profiles(state: &State, founder: Id, person: Id) -> Vec<(Profile, Option<Id>)> {
    let mut v: Vec<(Profile, Option<Id>)> = grants(state)
        .into_iter()
        .filter(|g| g.person == Some(person))
        .map(|g| (g.profile, g.unit))
        .collect();
    if person == founder && !v.contains(&(Profile::Mbb, None)) {
        v.insert(0, (Profile::Mbb, None));
    }
    v.sort();
    v.dedup();
    v
}

pub fn is_mbb(state: &State, founder: Id, person: Id) -> bool {
    profiles(state, founder, person)
        .iter()
        .any(|(p, _)| *p == Profile::Mbb)
}

/// The MBBs of the program (for the coaching logs).
pub fn mbbs(state: &State, founder: Id) -> Vec<Id> {
    let mut v: Vec<Id> = grants(state)
        .into_iter()
        .filter(|g| g.profile == Profile::Mbb)
        .filter_map(|g| g.person)
        .chain(std::iter::once(founder))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// What a person does in the program (the "Faz" column of 8.8) and what they see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// Program, units, objectives, standard review questions.
    ManageProgram,
    ManageRoles,
    SetTargets,
    /// Capacity, level and data of the program's people.
    ManagePeople,
    /// Screening: send an idea of the unit to the committee.
    SendToCommittee,
    /// The committee's (or a review's) meeting and its agenda.
    CommitteeAgenda,
    CommitteeDecide,
    Assign,
    ReviewTollgate,
    ConfirmCompetence,
    /// `estudou` and `praticou` (modules and exercises).
    RecordLearning,
    RecordCoaching,
    Anonymise,
    ValidateSavings,
    ApproveTollgate,
    ViewMbbDashboard,
    ViewLeadership,
    ViewScoreboard,
    ViewReviewQueue,
    Comment,
    // Stage L7 (section 8.5).
    /// Waves, classes, curriculum, nomination criteria, groups, instructors and the
    /// certification requirements.
    ManageTraining,
    /// Selecting trainees and the waiting list (and the MBB's override of a manager).
    SelectTrainees,
    /// Sessions, attendance, exams, grading.
    Teach,
    /// Issuing, renewing and revoking certifications, the MBB's opinion.
    Certify,
}

impl Action {
    /// Actions a champion or sponsor holds only in the unit of their grant.
    pub fn scoped(self) -> bool {
        matches!(self, Action::SendToCommittee | Action::ViewScoreboard)
    }
}

/// The matrix of section 8.8.
pub fn allowed(p: Profile, a: Action) -> bool {
    use Action::*;
    match p {
        // Everything of the program but validating savings (the controller's).
        Profile::Mbb => a != ValidateSavings,
        Profile::Champion => matches!(
            a,
            SendToCommittee
                | CommitteeAgenda
                | ApproveTollgate
                | ViewLeadership
                | ViewScoreboard
                | Comment
        ),
        Profile::Sponsor => matches!(
            a,
            ApproveTollgate | ViewLeadership | ViewScoreboard | Comment
        ),
        Profile::Belt => matches!(a, RecordCoaching | Comment),
        Profile::Controller => matches!(a, ValidateSavings | ViewScoreboard | Comment),
        Profile::Instructor => matches!(a, RecordLearning | Teach | Comment),
        Profile::Leadership => matches!(a, ViewLeadership | Comment),
        Profile::Student => matches!(a, Comment),
    }
}

/// The profiles that may do an action.
pub fn who_can(a: Action) -> Vec<Profile> {
    Profile::ALL
        .into_iter()
        .filter(|p| allowed(*p, a))
        .collect()
}

/// An action refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denied {
    pub action: Action,
    pub needed: Vec<Profile>,
    pub has: Vec<Profile>,
    /// The unit the action was about, when a grant of another unit did not cover it.
    pub out_of_scope: bool,
}

impl fmt::Display for Denied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = |v: &[Profile]| v.iter().map(|p| p.label()).collect::<Vec<_>>().join(", ");
        let has = if self.has.is_empty() {
            "nenhum perfil no programa".to_string()
        } else {
            format!("seu perfil: {}", names(&self.has))
        };
        if self.out_of_scope {
            write!(
                f,
                "ação fora da sua unidade; ela é de quem tem o perfil na unidade ({has})"
            )
        } else {
            write!(f, "ação reservada a: {} ({has})", names(&self.needed))
        }
    }
}

impl std::error::Error for Denied {}

/// Whether `person` may do `action` (on `unit`, for the scoped actions).
pub fn check(
    state: &State,
    founder: Id,
    person: Id,
    action: Action,
    unit: Option<Id>,
) -> Result<(), Denied> {
    let ps = profiles(state, founder, person);
    let mut scope_miss = false;
    for (p, scope) in &ps {
        if !allowed(*p, action) {
            continue;
        }
        let in_scope = !action.scoped()
            || *p == Profile::Mbb
            || scope.is_none()
            || unit.is_none_or(|u| *scope == Some(u));
        if in_scope {
            return Ok(());
        }
        scope_miss = true;
    }
    let mut has: Vec<Profile> = ps.iter().map(|(p, _)| *p).collect();
    has.dedup();
    Err(Denied {
        action,
        needed: who_can(action),
        has,
        out_of_scope: scope_miss,
    })
}

/// Fields a person may write about themselves (registering, consent).
pub const SELF_FIELDS: [&str; 5] = [
    dfields::person::NAME,
    dfields::person::EMAIL,
    pf::CONSENT,
    pf::PURPOSE,
    pf::CONSENT_ON,
];

/// The action an edit of the program journal needs (`None`: anyone may), with the unit it
/// concerns. `lookup` gives the value of another field of the same entity (from the batch
/// being written, else from the state).
pub fn required(
    kind: Kind,
    entity: Id,
    field: &str,
    author: Id,
    lookup: &dyn Fn(Id, &str) -> Option<Value>,
) -> Option<(Action, Option<Id>)> {
    if field.starts_with('$') {
        return None;
    }
    let id_of = |f: &str| -> Option<Id> { lookup(entity, f)?.as_str()?.parse().ok() };
    let a = match kind {
        Kind::Program | Kind::BusinessUnit | Kind::StrategicObjective | Kind::ReviewQuestion => {
            Action::ManageProgram
        }
        Kind::ProgramTarget => Action::SetTargets,
        Kind::ProgramRole => Action::ManageRoles,
        Kind::Person => {
            if field == pf::ANONYMISED {
                Action::Anonymise
            } else if entity == author && SELF_FIELDS.contains(&field) {
                return None;
            } else {
                Action::ManagePeople
            }
        }
        Kind::Absence => {
            if id_of(crate::designacao::fields::PERSON) == Some(author) {
                return None;
            }
            Action::ManagePeople
        }
        Kind::Opportunity => {
            if matches!(field, pf::DECISION | pf::REASON | pf::MEETING | pf::BELT) {
                Action::CommitteeDecide
            } else {
                return Some((Action::SendToCommittee, id_of(pf::UNIT)));
            }
        }
        Kind::CommitteeDecision => Action::CommitteeDecide,
        Kind::Meeting | Kind::MinutesItem => Action::CommitteeAgenda,
        Kind::Assignment => Action::Assign,
        Kind::Competence => {
            let level: Option<mentoria::Level> = lookup(entity, mentoria::fields::LEVEL)
                .and_then(|v| serde_json::from_value(v).ok());
            if level == Some(mentoria::Level::Demonstrated) {
                Action::ConfirmCompetence
            } else {
                Action::RecordLearning
            }
        }
        Kind::TollgateReview => Action::ReviewTollgate,
        Kind::Wave
        | Kind::TrainingClass
        | Kind::CurriculumModule
        | Kind::NominationCriterion
        | Kind::TrainingGroup
        | Kind::Instructor
        | Kind::CertificationRequirement => Action::ManageTraining,
        Kind::ClassSession | Kind::Exam => Action::Teach,
        Kind::Certification => Action::Certify,
        Kind::Nomination => {
            use crate::formacao::{NominationStatus, fields as nf};
            let person = id_of(nf::PERSON);
            let manager = id_of(nf::MANAGER);
            if matches!(
                field,
                nf::MANAGER_APPROVAL | nf::MANAGER_ON | nf::MANAGER_REASON
            ) {
                // The manager decides (never the nominee); the MBB may override.
                if manager == Some(author) && person != Some(author) {
                    return None;
                }
                Action::SelectTrainees
            } else if matches!(
                field,
                nf::STATUS
                    | nf::CLASS
                    | nf::POSITION
                    | nf::STATUS_REASON
                    | nf::DECIDED_BY
                    | nf::DECIDED_ON
            ) {
                // The person may withdraw; the rest is the MBB's selection.
                let withdrawing = lookup(entity, nf::STATUS)
                    .and_then(|v| serde_json::from_value::<NominationStatus>(v).ok())
                    == Some(NominationStatus::Withdrawn);
                if person == Some(author)
                    && withdrawing
                    && matches!(field, nf::STATUS | nf::STATUS_REASON)
                {
                    return None;
                }
                Action::SelectTrainees
            } else if id_of(nf::BY).is_some_and(|b| b != author) {
                // A nomination is changed only by who made it (or the MBB).
                Action::SelectTrainees
            } else {
                // Anyone nominates (the manager, the champion or the person).
                return None;
            }
        }
        Kind::CoachingLog | Kind::CoachingSession | Kind::Commitment => Action::RecordCoaching,
        _ => Action::ManageProgram,
    };
    Some((a, None))
}

/// Checks a batch of edits of the program journal by `author` before it is written.
pub fn check_edits(state: &State, founder: Id, author: Id, edits: &[Edit]) -> Result<(), Denied> {
    let lookup = |entity: Id, field: &str| -> Option<Value> {
        edits
            .iter()
            .rev()
            .find(|e| e.entity.id == entity && e.field == field)
            .map(|e| e.value.clone())
            .or_else(|| state.entity(entity).and_then(|e| e.get(field)).cloned())
    };
    for e in edits {
        if let Some((a, unit)) = required(e.entity.kind, e.entity.id, &e.field, author, &lookup) {
            check(state, founder, author, a, unit)?;
        }
    }
    Ok(())
}

/// An operation that had no effect: its author did not hold the profile at that instant.
#[derive(Debug, Clone, PartialEq)]
pub struct Unauthorized {
    pub op: Id,
    pub author: Id,
    pub at_ms: i64,
    pub entity: EntityRef,
    pub field: String,
    pub action: Action,
}

/// The state built only from authorized operations, and the others.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Authorized {
    pub state: State,
    pub unauthorized: Vec<Unauthorized>,
}

fn sorted<'a>(ops: impl IntoIterator<Item = &'a Op>) -> BTreeMap<OrderKey, &'a Op> {
    ops.into_iter().map(|o| (o.key(), o)).collect()
}

/// Rebuilds the program state checking each operation against the profiles of its author
/// in the state built so far (the instant of the operation in the journal's order).
pub fn replay<'a>(ops: impl IntoIterator<Item = &'a Op>, founder: Id) -> Authorized {
    let mut out = Authorized::default();
    for op in sorted(ops).into_values() {
        let st = &out.state;
        let lookup = |entity: Id, field: &str| -> Option<Value> {
            if entity == op.entity.id && field == op.field {
                Some(op.value.clone())
            } else {
                st.entity(entity).and_then(|e| e.get(field)).cloned()
            }
        };
        if let Some((a, unit)) =
            required(op.entity.kind, op.entity.id, &op.field, op.author, &lookup)
        {
            if check(st, founder, op.author, a, unit).is_err() {
                out.unauthorized.push(Unauthorized {
                    op: op.id,
                    author: op.author,
                    at_ms: op.at_ms,
                    entity: op.entity,
                    field: op.field.clone(),
                    action: a,
                });
                continue;
            }
        }
        let _ = out.state.apply(op);
    }
    out
}

/// The coaching log of `belt`: only the belt and the MBBs write in it (a person may still
/// register their own name, as on every folder).
pub fn replay_coaching<'a>(
    ops: impl IntoIterator<Item = &'a Op>,
    belt: Id,
    mbbs: &[Id],
) -> Authorized {
    let mut out = Authorized::default();
    for op in sorted(ops).into_values() {
        let own = op.entity.kind == Kind::Person
            && op.entity.id == op.author
            && SELF_FIELDS.contains(&op.field.as_str());
        let ok = op.is_meta() || own || op.author == belt || mbbs.contains(&op.author);
        if !ok {
            out.unauthorized.push(Unauthorized {
                op: op.id,
                author: op.author,
                at_ms: op.at_ms,
                entity: op.entity,
                field: op.field.clone(),
                action: Action::RecordCoaching,
            });
            continue;
        }
        let _ = out.state.apply(op);
    }
    out
}

/// Checks a batch for the coaching log of `belt`.
pub fn check_coaching(author: Id, belt: Id, mbbs: &[Id], edits: &[Edit]) -> Result<(), Denied> {
    if author == belt || mbbs.contains(&author) {
        return Ok(());
    }
    let only_self = edits.iter().all(|e| {
        e.entity.kind == Kind::Person
            && e.entity.id == author
            && SELF_FIELDS.contains(&e.field.as_str())
    });
    if only_self {
        return Ok(());
    }
    Err(Denied {
        action: Action::RecordCoaching,
        needed: vec![Profile::Mbb, Profile::Belt],
        has: Vec::new(),
        out_of_scope: true,
    })
}

/// A restricted folder of the training (section 8.9): the question bank, the grades or a
/// class folder. The staff (MBBs and instructors) write in all of them; in a class folder
/// each trainee writes only their own answers, until they hand the paper in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrainingWriters {
    pub staff: Vec<Id>,
    /// Trainees of the class (empty for the bank and the grades).
    pub students: Vec<Id>,
}

impl TrainingWriters {
    /// Whether an edit (entity, field, value) of `author` may be written given the state
    /// so far.
    fn may(
        &self,
        state: &State,
        author: Id,
        kind: Kind,
        entity: Id,
        field: &str,
        value: &Value,
    ) -> bool {
        use crate::formacao::{exams::fields as xf, fields as tf};
        if field.starts_with('$') || self.staff.contains(&author) {
            return true;
        }
        if kind == Kind::Person && entity == author && SELF_FIELDS.contains(&field) {
            return true;
        }
        if kind != Kind::ExamAnswer || !self.students.contains(&author) {
            return false;
        }
        let me = Value::from(author.to_string());
        let existing = state.entity(entity);
        if existing.is_some_and(|e| e.get(xf::SUBMITTED_ON).is_some_and(|v| !v.is_null())) {
            return false;
        }
        let owner_ok = existing
            .and_then(|e| e.get(tf::PERSON))
            .is_none_or(|p| *p == me);
        owner_ok && (field != tf::PERSON || *value == me)
    }
}

/// Rebuilds a training folder's state from its authorized operations.
pub fn replay_training<'a>(
    ops: impl IntoIterator<Item = &'a Op>,
    w: &TrainingWriters,
) -> Authorized {
    let mut out = Authorized::default();
    for op in sorted(ops).into_values() {
        if !w.may(
            &out.state,
            op.author,
            op.entity.kind,
            op.entity.id,
            &op.field,
            &op.value,
        ) {
            out.unauthorized.push(Unauthorized {
                op: op.id,
                author: op.author,
                at_ms: op.at_ms,
                entity: op.entity,
                field: op.field.clone(),
                action: Action::Teach,
            });
            continue;
        }
        let _ = out.state.apply(op);
    }
    out
}

/// Checks a batch for a training folder, in order (an answer's person goes first).
pub fn check_training(
    state: &State,
    author: Id,
    w: &TrainingWriters,
    edits: &[Edit],
) -> Result<(), Denied> {
    let mut st = state.clone();
    for (i, e) in edits.iter().enumerate() {
        if !w.may(&st, author, e.entity.kind, e.entity.id, &e.field, &e.value) {
            return Err(Denied {
                action: Action::Teach,
                needed: vec![Profile::Mbb, Profile::Instructor],
                has: Vec::new(),
                out_of_scope: true,
            });
        }
        // Apply in a scratch state so the next edits see the person just written.
        let op = Op {
            id: Id::from_bytes([0, 0, 0, 0, 0, 0, 0x70, 0, 0x80, 0, 0, 0, 0, 0, 0, i as u8]),
            author,
            at_ms: 0,
            lamport: st.max_lamport() + 1,
            entity: e.entity,
            field: e.field.clone(),
            value: e.value.clone(),
            base: None,
        };
        let _ = st.apply(&op);
    }
    Ok(())
}

/// Whether the units of the program have a champion of their own (for the screen's
/// warnings): units without one.
pub fn units_without_champion(state: &State, program: Id) -> Vec<Id> {
    let gs = grants(state);
    programa::units(state, program)
        .into_iter()
        .filter(|u| {
            !gs.iter().any(|g| {
                g.profile == Profile::Champion && (g.unit == Some(u.id) || g.unit.is_none())
            }) && u.champion.is_none()
        })
        .map(|u| u.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;
    use crate::domain::ProjectStatus;
    use crate::programa::testkit::*;

    /// Every action, per profile, exactly as the table of section 8.8.
    #[test]
    fn matrix_per_profile() {
        use Action::*;
        let expect: [(Profile, &[Action]); 8] = [
            (
                Profile::Mbb,
                &[
                    ManageProgram,
                    ManageRoles,
                    SetTargets,
                    Assign,
                    ReviewTollgate,
                    ConfirmCompetence,
                    CommitteeDecide,
                    ViewMbbDashboard,
                    Anonymise,
                ],
            ),
            (
                Profile::Champion,
                &[
                    SendToCommittee,
                    CommitteeAgenda,
                    ApproveTollgate,
                    ViewScoreboard,
                ],
            ),
            (Profile::Sponsor, &[ApproveTollgate, ViewScoreboard]),
            (Profile::Belt, &[RecordCoaching]),
            (Profile::Controller, &[ValidateSavings, ViewScoreboard]),
            (Profile::Instructor, &[RecordLearning]),
            (Profile::Leadership, &[ViewLeadership]),
            (Profile::Student, &[Comment]),
        ];
        for (p, yes) in expect {
            for a in yes {
                assert!(allowed(p, *a), "{p:?} {a:?}");
            }
        }
        for p in Profile::ALL.into_iter().filter(|p| *p != Profile::Mbb) {
            for a in [
                ManageRoles,
                SetTargets,
                Assign,
                ReviewTollgate,
                ConfirmCompetence,
                ViewMbbDashboard,
                Anonymise,
                CommitteeDecide,
            ] {
                assert!(!allowed(p, a), "{p:?} {a:?}");
            }
        }
        assert!(!allowed(Profile::Mbb, ValidateSavings));
        assert!(!allowed(Profile::Leadership, ViewMbbDashboard));
        assert!(!allowed(Profile::Belt, ViewLeadership));
        assert_eq!(who_can(ValidateSavings), vec![Profile::Controller]);
    }

    #[test]
    fn checks_with_clear_messages_and_unit_scope() {
        let mut t = T::new();
        let p = t.program;
        let founder = t.me;
        let (u1, u2) = (t.id(), t.id());
        t.commit(programa::add_unit(p, u1, "Planta 1"));
        t.commit(programa::add_unit(p, u2, "Planta 2"));
        let (champ, belt, nobody) = (t.id(), t.id(), t.id());
        let g = t.id();
        t.commit(grant(p, g, champ, Profile::Champion, Some(u1)));
        let g = t.id();
        t.commit(grant(p, g, belt, Profile::Belt, None));
        assert!(is_mbb(&t.s, founder, founder));
        assert!(check(&t.s, founder, founder, Action::SetTargets, None).is_ok());
        let e = check(&t.s, founder, belt, Action::SetTargets, None).unwrap_err();
        assert_eq!(e.to_string(), "ação reservada a: MBB (seu perfil: belt)");
        let e = check(&t.s, founder, nobody, Action::ViewLeadership, None).unwrap_err();
        assert!(e.to_string().contains("nenhum perfil"), "{e}");
        assert!(check(&t.s, founder, champ, Action::SendToCommittee, Some(u1)).is_ok());
        let e = check(&t.s, founder, champ, Action::SendToCommittee, Some(u2)).unwrap_err();
        assert!(e.out_of_scope);
        assert!(e.to_string().contains("fora da sua unidade"));
        // Edits: a champion sends an idea of their unit, not of the other.
        let i1 = idea(t.id(), Some(u1), ProjectStatus::Screening, 50.0);
        let i2 = idea(t.id(), Some(u2), ProjectStatus::Screening, 50.0);
        let today = Date::new(2026, 11, 3).unwrap();
        let e1 = programa::send_to_committee(p, t.id(), &i1, &[], champ, today).unwrap();
        assert!(check_edits(&t.s, founder, champ, &e1).is_ok());
        let e2 = programa::send_to_committee(p, t.id(), &i2, &[], champ, today).unwrap();
        assert!(check_edits(&t.s, founder, champ, &e2).is_err());
        // Anyone registers their own name and consent; not someone else's.
        let own = vec![
            Edit::new(
                EntityRef::new(Kind::Person, belt),
                dfields::person::NAME,
                "Bia",
            ),
            Edit::new(EntityRef::new(Kind::Person, belt), pf::CONSENT, true),
        ];
        assert!(check_edits(&t.s, founder, belt, &own).is_ok());
        assert!(check_edits(&t.s, founder, nobody, &own).is_err());
        let level = vec![Edit::new(
            EntityRef::new(Kind::Person, belt),
            pf::LEVEL,
            "black_belt",
        )];
        assert!(
            check_edits(&t.s, founder, belt, &level).is_err(),
            "nobody raises their own level"
        );
        // Competence: an instructor records `estudou`, only the MBB `demonstrou`.
        let ins = t.id();
        let g = t.id();
        t.commit(grant(p, g, ins, Profile::Instructor, None));
        let studied = mentoria::record_level(
            p,
            t.id(),
            belt,
            "msa",
            mentoria::Level::Studied,
            mentoria::Source::Module,
            "",
            ins,
            today,
        );
        assert!(check_edits(&t.s, founder, ins, &studied).is_ok());
        let demo = mentoria::record_level(
            p,
            t.id(),
            belt,
            "msa",
            mentoria::Level::Demonstrated,
            mentoria::Source::Mbb,
            "",
            ins,
            today,
        );
        assert!(check_edits(&t.s, founder, ins, &demo).is_err());
        assert!(check_edits(&t.s, founder, founder, &demo).is_ok());
        assert_eq!(units_without_champion(&t.s, p), vec![u2]);
    }

    /// Operations of authors without the profile are ignored and listed; a role given in
    /// the middle of the journal counts from its instant on; a role removed stops counting.
    #[test]
    fn replay_respects_the_instant_of_each_role() {
        let mut t = T::new();
        let p = t.program;
        let founder = t.me;
        let x = t.id();
        let u = t.id();
        t.commit(programa::add_unit(p, u, "Planta 1"));
        // X sets a target before being an MBB: no effect.
        let tgt = t.id();
        let e = programa::set_target(
            &t.s,
            p,
            tgt,
            u,
            2026,
            programa::Metric::ProjectsClosed,
            None,
            3.0,
        )
        .unwrap();
        t.commit_as(x, e);
        // X forges a role for themselves: no effect either.
        let forged = t.id();
        t.commit_as(x, grant(p, forged, x, Profile::Mbb, None));
        // The MBB gives X the MBB profile; X's later target counts.
        let g = t.id();
        t.commit(grant(p, g, x, Profile::Mbb, None));
        let tgt2 = t.id();
        t.commit_as(
            x,
            vec![Edit::new(
                EntityRef::new(Kind::ProgramTarget, tgt2),
                pf::VALUE,
                5.0,
            )],
        );
        // The role is removed; X's next change is ignored.
        t.commit(vec![Edit::new(
            EntityRef::new(Kind::ProgramRole, g),
            crate::op::FIELD_DELETED,
            true,
        )]);
        t.commit_as(
            x,
            vec![Edit::new(
                EntityRef::new(Kind::Program, p),
                pf::NAME,
                "Outro nome",
            )],
        );
        // The raw state of the test kept everything; the replay over the ops does not.
        let a = replay(&t.ops, founder);
        // Any order of arrival gives the same result.
        let mut rev = t.ops.clone();
        rev.reverse();
        assert_eq!(replay(&rev, founder), a);
        assert!(a.state.entity(tgt).is_none(), "target before the role");
        assert!(a.state.entity(forged).is_none(), "forged role");
        assert_eq!(a.state.entity(tgt2).unwrap().number(pf::VALUE), Some(5.0));
        assert_eq!(a.state.entity(p).unwrap().text(pf::NAME), "Programa LSS");
        assert!(a.unauthorized.iter().all(|u| u.author == x));
        let actions: Vec<Action> = a.unauthorized.iter().map(|u| u.action).collect();
        assert!(actions.contains(&Action::SetTargets));
        assert!(actions.contains(&Action::ManageRoles));
        assert!(actions.contains(&Action::ManageProgram));
        // Coaching log: only the belt and the MBBs.
        let belt = t.id();
        let log_ops = coaching_ops(belt, x, founder);
        let c = replay_coaching(&log_ops, belt, &[founder]);
        assert_eq!(c.unauthorized.len(), 1);
        assert_eq!(c.unauthorized[0].author, x);
        assert!(
            check_coaching(
                x,
                belt,
                &[founder],
                &[Edit::new(
                    EntityRef::new(Kind::CoachingSession, belt),
                    "temas",
                    "x"
                )]
            )
            .is_err()
        );
        assert!(check_coaching(belt, belt, &[founder], &[]).is_ok());
    }

    fn coaching_ops(belt: Id, intruder: Id, mbb: Id) -> Vec<Op> {
        let mk = |n: u64, author: Id, field: &str| Op {
            id: Id::v7(n, [n as u8; 10]),
            author,
            at_ms: 0,
            lamport: n,
            entity: EntityRef::new(Kind::CoachingSession, Id::v7(500, [5; 10])),
            field: field.into(),
            value: Value::from("x"),
            base: None,
        };
        vec![
            mk(1, mbb, "temas"),
            mk(2, belt, "dificuldades"),
            mk(3, intruder, "fase"),
        ]
    }
}
