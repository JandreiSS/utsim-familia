//! Risk register (`docs/plano_lean_seis_sigma.md`, 1.7): probability × impact on a 5 × 5
//! matrix, response, owner, review date, and the response turned into a card on the board.

use crate::board::{self, CardSpec};
use crate::date::Date;
use crate::domain::{Origin, Risk, RiskStatus, enum_value, fields as dfields, of_project};
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::State;

pub mod fields {
    /// Next review date.
    pub const REVIEW: &str = "revisao";
    /// What the response consists of (the action, in words).
    pub const RESPONSE_PLAN: &str = "plano_de_resposta";
}

/// Level band of the matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    /// Level 1 to 4.
    Low,
    /// Level 5 to 14.
    Medium,
    /// Level 15 to 25 (the red zone, as [`Risk::is_high`]).
    High,
}

pub fn band(level: u8) -> Band {
    match level {
        0..=4 => Band::Low,
        5..=14 => Band::Medium,
        _ => Band::High,
    }
}

/// Risks of a project, highest level first (then creation order).
pub fn risks(state: &State, project: Id) -> Vec<Risk> {
    let mut v = of_project(state, Kind::Risk, project, Risk::read);
    v.sort_by(|a, b| b.level().cmp(&a.level()).then(a.id.cmp(&b.id)));
    v
}

/// Count of open risks in each cell: `matrix[probability - 1][impact - 1]`.
pub fn matrix(risks: &[Risk]) -> [[usize; 5]; 5] {
    let mut m = [[0; 5]; 5];
    for r in risks.iter().filter(|r| r.status == RiskStatus::Open) {
        m[usize::from(r.probability - 1)][usize::from(r.impact - 1)] += 1;
    }
    m
}

/// The review date of a risk.
pub fn review_date(state: &State, risk: Id) -> Option<Date> {
    state
        .entity(risk)
        .and_then(|e| e.opt_text(fields::REVIEW))
        .and_then(|s| s.parse().ok())
}

/// Open risks whose review date is on or before `day`.
pub fn due_for_review(state: &State, project: Id, day: Date) -> Vec<Risk> {
    risks(state, project)
        .into_iter()
        .filter(|r| r.status == RiskStatus::Open)
        .filter(|r| review_date(state, r.id).is_some_and(|d| d <= day))
        .collect()
}

/// Edits that create a risk.
pub fn new_risk(project: Id, id: Id, description: &str, probability: u8, impact: u8) -> Vec<Edit> {
    use dfields::risk as f;
    let r = EntityRef::new(Kind::Risk, id);
    vec![
        Edit::new(r, f::PROJECT, project.to_string()),
        Edit::new(r, f::DESCRIPTION, description.trim()),
        Edit::new(r, f::PROBABILITY, probability.clamp(1, 5)),
        Edit::new(r, f::IMPACT, impact.clamp(1, 5)),
        Edit::new(r, f::STATUS, enum_value(RiskStatus::Open)),
    ]
}

/// Edits that turn the response of a risk into a card (owner and review date carried), linked
/// to the risk. `title` is the card title (the response plan, or the risk itself).
pub fn response_card(state: &State, project: Id, risk: &Risk, card: Id, title: &str) -> Vec<Edit> {
    let spec = CardSpec {
        title: title.to_string(),
        why: risk.description.clone(),
        who: risk.owner,
        due: review_date(state, risk.id),
        phase: None,
    };
    board::linked_card(
        state,
        project,
        card,
        &spec,
        Origin::Risk,
        EntityRef::new(Kind::Risk, risk.id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Approach, Project};
    use proptest::prelude::*;

    #[test]
    fn scoring_matrix_review_and_card() {
        let mut n = 0u64;
        let mut ids = || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        };
        let (me, p) = (ids(), ids());
        let mut s = State::replay(&State::new().prepare(
            me,
            0,
            Project::create(p, "P", Approach::Dmaic, &mut ids),
            &mut ids,
        ));
        let mut commit = |s: &mut State, e: Vec<Edit>| {
            for op in s.prepare(me, 0, e, &mut ids) {
                s.apply(&op).unwrap();
            }
        };
        let (a, b, c) = (
            Id::v7(50, [1; 10]),
            Id::v7(51, [2; 10]),
            Id::v7(52, [3; 10]),
        );
        commit(&mut s, new_risk(p, a, "Fornecedor atrasa", 4, 5));
        commit(&mut s, new_risk(p, b, "Dado incompleto", 2, 2));
        commit(&mut s, new_risk(p, c, "Turno C resiste", 9, 3));
        let rs = risks(&s, p);
        assert_eq!(
            rs.iter().map(|r| r.level()).collect::<Vec<_>>(),
            [20, 15, 4]
        );
        assert_eq!(band(rs[0].level()), Band::High);
        assert_eq!(band(4), Band::Low);
        assert_eq!(band(5), Band::Medium);
        let m = matrix(&rs);
        assert_eq!(m[3][4], 1);
        assert_eq!(m[4][2], 1, "probability clamped to 5");
        let rr = EntityRef::new(Kind::Risk, a);
        commit(
            &mut s,
            vec![
                Edit::new(rr, fields::REVIEW, "2026-11-10"),
                Edit::new(rr, dfields::risk::OWNER, me.to_string()),
            ],
        );
        let day = Date::new(2026, 11, 10).unwrap();
        assert_eq!(due_for_review(&s, p, day).len(), 1);
        assert!(due_for_review(&s, p, day.add_days(-1)).is_empty());
        let risk = risks(&s, p).remove(0);
        let card = Id::v7(99, [9; 10]);
        let e = response_card(&s, p, &risk, card, "Qualificar segundo fornecedor");
        commit(&mut s, e);
        let cards = board::cards_linked_to(&s, p, a);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].due, Some(day));
        assert_eq!(cards[0].who, Some(me));
        assert_eq!(cards[0].why, "Fornecedor atrasa");
        commit(
            &mut s,
            vec![Edit::new(
                rr,
                dfields::risk::STATUS,
                enum_value(RiskStatus::Closed),
            )],
        );
        assert!(due_for_review(&s, p, day).is_empty());
        assert_eq!(
            matrix(&risks(&s, p))[3][4],
            0,
            "closed risks leave the matrix"
        );
    }

    proptest! {
        #[test]
        fn level_is_the_product_and_bands_are_monotone(p in 1u8..=5, i in 1u8..=5) {
            let r = Risk {
                id: Id::NIL, project: None, description: String::new(), cause: String::new(),
                effect: String::new(), probability: p, impact: i,
                response: Default::default(), owner: None, status: RiskStatus::Open,
            };
            prop_assert_eq!(r.level(), p * i);
            prop_assert_eq!(r.is_high(), band(r.level()) == Band::High);
            if p < 5 {
                prop_assert!(band((p + 1) * i) >= band(p * i));
            }
            let m = matrix(&[r]);
            prop_assert_eq!(m.iter().flatten().sum::<usize>(), 1);
        }
    }
}
