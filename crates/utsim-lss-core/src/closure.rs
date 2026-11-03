//! Closing a project (`docs/plano_lean_seis_sigma.md`, 1.9): the result of the Y against
//! the goal, the savings, the control plan accepted by the process owner, the lessons, and
//! the checks that must pass before the project is closed.

use crate::date::Date;
use crate::domain::{
    Column, Project, ProjectStatus, RoleKind, SavingStatus, enum_value, fields as dfields,
    of_project, phases,
};
use crate::id::Id;
use crate::op::{Edit, Kind};
use crate::roles;
use crate::state::State;
use crate::tollgate::{self, TollgateStatus};

pub mod fields {
    /// On the project.
    pub const RESULT: &str = "resultado_y";
    pub const NOTES: &str = "encerramento";
    pub const OWNER_ACCEPTED: &str = "aceite_do_dono";
    pub const OWNER_ACCEPTED_BY: &str = "aceite_do_dono_por";
    pub const OWNER_ACCEPTED_ON: &str = "aceite_do_dono_em";
    pub const CLOSED_ON: &str = "encerrado_em";
}

/// One condition of the closing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// Every tollgate approved (required).
    TollgatesApproved,
    /// The process owner accepted the transfer (required).
    OwnerAccepted,
    /// At least one lesson learned (required).
    LessonsRecorded,
    /// The result of the Y written (required).
    ResultWritten,
    /// Every saving at least forecast; the follow-up goes on after the closing (advice).
    SavingsForecast,
    /// Every action done (advice: the open ones go to the process owner).
    ActionsDone,
}

impl Requirement {
    pub const ALL: [Requirement; 6] = [
        Requirement::TollgatesApproved,
        Requirement::OwnerAccepted,
        Requirement::LessonsRecorded,
        Requirement::ResultWritten,
        Requirement::SavingsForecast,
        Requirement::ActionsDone,
    ];

    pub fn required(self) -> bool {
        !matches!(
            self,
            Requirement::SavingsForecast | Requirement::ActionsDone
        )
    }
}

/// Each requirement and whether it holds.
pub fn check(state: &State, project: Id) -> Vec<(Requirement, bool)> {
    let e = state.entity(project);
    let tollgates_ok = {
        let ph = phases(state, project);
        !ph.is_empty()
            && ph.iter().all(|p| {
                tollgate::of_phase(state, p.id)
                    .is_some_and(|t| t.status == TollgateStatus::Approved)
            })
    };
    let savings = of_project(state, Kind::Saving, project, crate::domain::Saving::read);
    let actions = of_project(state, Kind::Action, project, crate::domain::Action::read);
    Requirement::ALL
        .iter()
        .map(|r| {
            let ok = match r {
                Requirement::TollgatesApproved => tollgates_ok,
                Requirement::OwnerAccepted => e.is_some_and(|e| e.boolean(fields::OWNER_ACCEPTED)),
                Requirement::LessonsRecorded => !crate::lessons::lessons(state, project).is_empty(),
                Requirement::ResultWritten => {
                    e.is_some_and(|e| !e.text(fields::RESULT).trim().is_empty())
                }
                Requirement::SavingsForecast => {
                    savings.iter().all(|s| s.status >= SavingStatus::Forecast)
                }
                Requirement::ActionsDone => actions.iter().all(|a| a.column == Column::Done),
            };
            (*r, ok)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClosureError {
    #[error("faltam {0} condições obrigatórias para encerrar")]
    Missing(usize),
    #[error("só o dono do processo aceita a transferência; defina-o em Papéis")]
    NotOwner,
}

/// Edits that record the process owner's acceptance (only a process owner of the project).
pub fn accept(state: &State, project: Id, by: Id, today: Date) -> Result<Vec<Edit>, ClosureError> {
    if !roles::has_role(state, project, by, RoleKind::ProcessOwner) {
        return Err(ClosureError::NotOwner);
    }
    let p = Project::entity_ref(project);
    Ok(vec![
        Edit::new(p, fields::OWNER_ACCEPTED, true),
        Edit::new(p, fields::OWNER_ACCEPTED_BY, by.to_string()),
        Edit::new(p, fields::OWNER_ACCEPTED_ON, today.to_string()),
    ])
}

/// Edits that close the project, when every required condition holds.
pub fn close(state: &State, project: Id, today: Date) -> Result<Vec<Edit>, ClosureError> {
    let missing = check(state, project)
        .iter()
        .filter(|(r, ok)| r.required() && !ok)
        .count();
    if missing > 0 {
        return Err(ClosureError::Missing(missing));
    }
    let p = Project::entity_ref(project);
    Ok(vec![
        Edit::new(
            p,
            dfields::project::STATUS,
            enum_value(ProjectStatus::Closed),
        ),
        Edit::new(p, fields::CLOSED_ON, today.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Approach;
    use crate::op::EntityRef;
    use crate::tollgate::{Decision, DecisionInput};

    #[test]
    fn closing_needs_tollgates_acceptance_lessons_and_result() {
        let mut n = 0u64;
        let mut ids = move || {
            n += 1;
            Id::v7(n, [(n % 251) as u8; 10])
        };
        let (me, owner, p) = (ids(), ids(), ids());
        let mut s = State::replay(&State::new().prepare(
            me,
            0,
            Project::create(p, "P", Approach::A3, &mut ids),
            &mut ids,
        ));
        let commit = |s: &mut State, e: Vec<Edit>, ids: &mut dyn FnMut() -> Id| {
            for op in s.prepare(me, 0, e, ids) {
                s.apply(&op).unwrap();
            }
        };
        let today = Date::new(2026, 11, 3).unwrap();
        assert_eq!(close(&s, p, today), Err(ClosureError::Missing(4)));
        assert_eq!(accept(&s, p, owner, today), Err(ClosureError::NotOwner));
        let e = roles::add_role(&s, p, ids(), owner, RoleKind::ProcessOwner);
        commit(&mut s, e, &mut ids);
        let e = roles::add_role(&s, p, ids(), me, RoleKind::Sponsor);
        commit(&mut s, e, &mut ids);
        let e = accept(&s, p, owner, today).unwrap();
        commit(&mut s, e, &mut ids);
        commit(
            &mut s,
            crate::lessons::new_lesson(p, ids(), "Ouvir o turno"),
            &mut ids,
        );
        commit(
            &mut s,
            vec![Edit::new(
                EntityRef::new(Kind::Project, p),
                fields::RESULT,
                "refugo 1,8 %",
            )],
            &mut ids,
        );
        assert_eq!(
            close(&s, p, today),
            Err(ClosureError::Missing(1)),
            "the A3 tollgate"
        );
        // Approve the single A3 tollgate.
        let ph = phases(&s, p)[0].id;
        let e = tollgate::prepare(&s, p, ph, &mut ids);
        commit(&mut s, e, &mut ids);
        let tg = tollgate::of_phase(&s, ph).unwrap();
        let item = tollgate::items(&s, tg.id)[0].id;
        commit(
            &mut s,
            vec![Edit::new(
                EntityRef::new(Kind::ChecklistItem, item),
                dfields::checklist_item::DONE,
                true,
            )],
            &mut ids,
        );
        let e = tollgate::submit(&s, p, tg.id, today).unwrap();
        commit(&mut s, e, &mut ids);
        let input = DecisionInput {
            approver: me,
            decision: Decision::Approve,
            reservations: vec![],
            comment: String::new(),
            date: today,
        };
        let e = tollgate::decide(&s, p, tg.id, &input, &mut ids).unwrap();
        commit(&mut s, e, &mut ids);
        let c = check(&s, p);
        assert!(c.iter().all(|(_, ok)| *ok), "{c:?}");
        let e = close(&s, p, today).unwrap();
        commit(&mut s, e, &mut ids);
        assert_eq!(
            Project::read(p, s.entity(p).unwrap()).status,
            ProjectStatus::Closed
        );
    }
}
