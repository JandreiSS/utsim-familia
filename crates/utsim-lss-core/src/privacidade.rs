//! Personal data of the program (`docs/plano_lean_seis_sigma.md`, 8.8 and decision MB8):
//! consent and purpose recorded in the person's entry, the coaching log seen only by the
//! MBB and the belt ([`crate::mentoria::can_see`]), a retention period set by the program
//! and the anonymisation operation, which replaces the personal fields of a person (and
//! the texts of their coaching log) by neutral values. The store also rewrites the
//! history of those fields in the program folder and the log
//! (`utsim_lss_store::program::redact`), so the original values do not survive in old
//! operation files either.

use serde_json::Value;

use crate::date::Date;
use crate::designacao;
use crate::domain::fields as dfields;
use crate::id::Id;
use crate::mentoria;
use crate::op::{Edit, EntityRef, Kind};
use crate::perfis;
use crate::programa::{Member, ProjectRecord, fields as pf};
use crate::state::State;

/// On the program: months a person with no role and no activity is kept.
pub const RETENTION: &str = "retencao_meses";
pub const DEFAULT_RETENTION_MONTHS: i64 = 60;

/// Personal fields of a person of the program.
pub const PERSONAL_FIELDS: [&str; 4] = [
    dfields::person::NAME,
    dfields::person::EMAIL,
    pf::AREA,
    pf::PURPOSE,
];

/// Free-text fields of a coaching log.
pub const COACHING_TEXT_FIELDS: [&str; 5] = [
    mentoria::fields::TOPICS,
    mentoria::fields::DIFFICULTIES,
    mentoria::fields::PHASE,
    mentoria::fields::TEXT,
    designacao::fields::REASON,
];

/// Edits that record (or withdraw) the consent of a person with its purpose.
pub fn consent(person: Id, given: bool, purpose: &str, date: Date) -> Vec<Edit> {
    let r = Member::entity_ref(person);
    vec![
        Edit::new(r, pf::CONSENT, given),
        Edit::new(r, pf::PURPOSE, purpose.trim()),
        Edit::new(r, pf::CONSENT_ON, date.to_string()),
    ]
}

pub fn retention_months(state: &State, program: Id) -> i64 {
    state
        .entity(program)
        .and_then(|e| e.integer(RETENTION))
        .filter(|m| *m > 0)
        .unwrap_or(DEFAULT_RETENTION_MONTHS)
}

/// The neutral name of an anonymised person.
pub fn placeholder(person: Id) -> String {
    format!("Pessoa anonimizada {}", person.short())
}

/// The value a personal field takes after the anonymisation (also used to rewrite the
/// history).
pub fn redacted_value(field: &str, person: Id) -> Value {
    if field == dfields::person::NAME {
        Value::from(placeholder(person))
    } else {
        Value::from("")
    }
}

/// Edits that anonymise a person in the program journal: personal fields replaced,
/// consent withdrawn, the mark `anonimizado`.
pub fn anonymise(state: &State, person: Id) -> Vec<Edit> {
    let r = Member::entity_ref(person);
    let Some(e) = state.entity(person) else {
        return Vec::new();
    };
    let mut v: Vec<Edit> = PERSONAL_FIELDS
        .iter()
        .filter(|f| e.get(f).is_some())
        .map(|f| Edit::new(r, *f, redacted_value(f, person)))
        .collect();
    v.push(Edit::new(r, pf::CONSENT, false));
    v.push(Edit::new(r, pf::ANONYMISED, true));
    // Absences carry a reason typed by people.
    for (id, a) in state.of_kind(Kind::Absence) {
        if a.id(designacao::fields::PERSON) == Some(person)
            && !a.text(designacao::fields::REASON).is_empty()
        {
            v.push(Edit::new(
                EntityRef::new(Kind::Absence, id),
                designacao::fields::REASON,
                "",
            ));
        }
    }
    v
}

/// Edits that empty the free texts of a coaching log (sessions and commitments) and
/// replace the personal fields of `person` registered in it.
pub fn anonymise_coaching(state: &State, person: Id) -> Vec<Edit> {
    let mut v = Vec::new();
    if let Some(e) = state.entity(person).filter(|e| e.kind == Kind::Person) {
        let r = Member::entity_ref(person);
        for f in PERSONAL_FIELDS {
            let new = redacted_value(f, person);
            if e.get(f).is_some_and(|x| *x != new) {
                v.push(Edit::new(r, f, new));
            }
        }
    }
    for (id, e) in state.entities.iter() {
        if !matches!(e.kind, Kind::CoachingSession | Kind::Commitment) || e.is_deleted() {
            continue;
        }
        for f in COACHING_TEXT_FIELDS {
            if e.get(f)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
            {
                v.push(Edit::new(EntityRef::new(e.kind, *id), f, ""));
            }
        }
    }
    v
}

/// Why a person is due for anonymisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// The consent was withdrawn.
    ConsentWithdrawn,
    /// No profile, no running project and no change for longer than the retention.
    Retention,
}

/// The people the MBB should anonymise (never automatic: the MBB confirms).
pub fn due(
    state: &State,
    program: Id,
    founder: Id,
    members: &[Member],
    projects: &[ProjectRecord],
    today: Date,
    offset_minutes: i32,
) -> Vec<(Id, Due)> {
    let months = retention_months(state, program);
    let limit = today.add_days(-(months * 30));
    let grants = perfis::grants(state);
    members
        .iter()
        .filter(|m| !m.anonymised && m.id != founder)
        .filter_map(|m| {
            if !m.consent && m.consent_on.is_some() {
                return Some((m.id, Due::ConsentWithdrawn));
            }
            let has_role = grants.iter().any(|g| g.person == Some(m.id));
            let busy = projects.iter().any(|p| {
                p.is_running()
                    && (p.belts.contains(&m.id)
                        || p.champions.contains(&m.id)
                        || p.sponsors.contains(&m.id))
            });
            let last = state
                .entity(m.id)
                .and_then(|e| e.last_change())
                .map(|f| Date::from_unix_ms(f.at_ms, offset_minutes));
            let stale = last.is_none_or(|d| d < limit);
            (!has_role && !busy && stale).then_some((m.id, Due::Retention))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::programa::testkit::*;
    use crate::programa::{self, BeltLevel};

    fn every_text(s: &State) -> String {
        serde_json::to_string(&s.entities).unwrap()
    }

    #[test]
    fn no_personal_value_survives_the_anonymisation() {
        let mut t = T::new();
        let p = t.program;
        let ana = t.person("Ana Souza Lima", Some(BeltLevel::Green), None);
        t.commit(vec![
            Edit::new(
                Member::entity_ref(ana),
                dfields::person::EMAIL,
                "ana.souza@empresa.com",
            ),
            Edit::new(Member::entity_ref(ana), pf::AREA, "Estamparia turno B"),
        ]);
        t.commit(consent(
            ana,
            true,
            "acompanhar a formação de belts",
            d(2026, 1, 10),
        ));
        let a = t.id();
        t.commit(designacao::add_absence(
            a,
            ana,
            d(2026, 3, 1),
            d(2026, 3, 5),
            "cirurgia no joelho",
        ));
        assert!(every_text(&t.s).contains("Ana Souza Lima"));
        t.commit(anonymise(&t.s, ana));
        let text = every_text(&t.s);
        for original in [
            "Ana Souza Lima",
            "ana.souza@empresa.com",
            "Estamparia turno B",
            "acompanhar a formação",
            "cirurgia",
        ] {
            assert!(!text.contains(original), "{original}");
        }
        let m = programa::members(&t.s)
            .into_iter()
            .find(|m| m.id == ana)
            .unwrap();
        assert!(m.anonymised);
        assert!(!m.consent);
        assert_eq!(m.name, placeholder(ana));
        // The coaching log: texts emptied.
        let mut log = T::new();
        let s = log.id();
        let belt = Member {
            consent: true,
            ..m.clone()
        };
        let e = mentoria::new_session(
            &belt,
            s,
            &mentoria::SessionInput {
                belt: ana,
                mentor: log.me,
                date: d(2026, 4, 1),
                minutes: 60.0,
                form: mentoria::Form::Individual,
                project: None,
                phase: "Medir".into(),
                topics: "dificuldade pessoal com o chefe".into(),
                tools: vec![],
            },
        )
        .unwrap();
        log.commit(e);
        let c = log.id();
        log.commit(
            mentoria::add_commitment(s, c, "conversar com o RH", None, mentoria::Target::Personal)
                .unwrap(),
        );
        log.commit(anonymise_coaching(&log.s, ana));
        let text = every_text(&log.s);
        assert!(!text.contains("chefe") && !text.contains("RH"));
        assert!(anonymise_coaching(&log.s, ana).is_empty(), "nothing left");
        let _ = p;
    }

    #[test]
    fn due_by_withdrawn_consent_or_retention() {
        let mut t = T::new();
        let p = t.program;
        let (ana, bia, caio) = (
            t.person("Ana", Some(BeltLevel::Green), None),
            t.person("Bia", None, None),
            t.person("Caio", Some(BeltLevel::Green), None),
        );
        t.commit(consent(ana, false, "", d(2026, 1, 1)));
        let g = t.id();
        t.commit(perfis::grant(p, g, caio, perfis::Profile::Belt, None));
        let ms = programa::members(&t.s);
        // The kit writes at instant 0 (1970): everyone without a role is stale.
        let v = due(&t.s, p, t.me, &ms, &[], d(2026, 11, 3), 0);
        assert!(v.contains(&(ana, Due::ConsentWithdrawn)));
        assert!(v.contains(&(bia, Due::Retention)));
        assert!(!v.iter().any(|(x, _)| *x == caio), "has a profile");
        let mut busy = project(t.id(), None, crate::domain::ProjectStatus::Running);
        busy.sponsors = vec![bia];
        let v = due(&t.s, p, t.me, &ms, &[busy], d(2026, 11, 3), 0);
        assert!(
            !v.iter().any(|(x, _)| *x == bia),
            "sponsor of a running project"
        );
        assert_eq!(retention_months(&t.s, p), DEFAULT_RETENTION_MONTHS);
    }
}
