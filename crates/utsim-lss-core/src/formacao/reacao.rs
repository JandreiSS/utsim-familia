//! The reaction evaluation of a class (P9 of `docs/plano_lean_seis_sigma.md`): at the end of
//! the class each trainee answers five statements from 1 (discordo totalmente) to 5
//! (concordo totalmente) and may leave a comment, in the class folder, like the answers of
//! an exam (the trainee writes only their own, and nothing after sending it). The staff see
//! the summary of the class, never one person's answers, and only from [`MIN_ANSWERS`]
//! answers on, so nobody is identified by elimination.

use crate::date::Date;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};

use super::exams::fields::SUBMITTED_ON;
use super::fields as tf;

/// The statements, in order (the keys of the scores are `nota_1`...`nota_5`).
pub const STATEMENTS: [&str; 5] = [
    "O conteúdo vai ser útil no meu trabalho.",
    "O instrutor explicou com clareza.",
    "Os exercícios e o projeto ajudaram a aprender.",
    "A carga horária foi adequada ao conteúdo.",
    "Eu recomendaria esta formação a um colega.",
];

/// Fewer answers than this and the summary is not shown.
pub const MIN_ANSWERS: usize = 3;

pub mod fields {
    pub const COMMENT: &str = "comentario";
    /// `nota_<n>`, 1 to 5.
    pub fn score(n: usize) -> String {
        format!("nota_{n}")
    }
}

/// One answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Reaction {
    pub id: Id,
    pub class: Option<Id>,
    pub person: Option<Id>,
    /// One per statement, 1 to 5 (`None` when left blank).
    pub scores: [Option<u8>; 5],
    pub comment: String,
    pub sent: bool,
}

impl Reaction {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            class: e.id(tf::CLASS),
            person: e.id(tf::PERSON),
            scores: std::array::from_fn(|i| {
                e.integer(&fields::score(i + 1))
                    .filter(|v| (1..=5).contains(v))
                    .map(|v| v as u8)
            }),
            comment: e.text(fields::COMMENT),
            sent: e.opt_text(SUBMITTED_ON).is_some(),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Reaction, id)
    }
}

pub fn reactions(class_state: &State) -> Vec<Reaction> {
    let mut v: Vec<Reaction> = class_state
        .of_kind(Kind::Reaction)
        .filter(|(_, e)| !e.is_deleted())
        .map(|(id, e)| Reaction::read(id, e))
        .collect();
    v.sort_by_key(|r| r.id);
    v
}

/// The edits of a trainee's answer, sent on `date` (the person first: the rule of the
/// folder reads it). Scores out of 1–5 are left blank.
pub fn answer(
    class: Id,
    id: Id,
    person: Id,
    scores: &[Option<u8>; 5],
    comment: &str,
    date: Date,
) -> Vec<Edit> {
    let r = Reaction::entity_ref(id);
    let mut e = vec![
        Edit::new(r, tf::PERSON, person.to_string()),
        Edit::new(r, tf::CLASS, class.to_string()),
    ];
    for (i, s) in scores.iter().enumerate() {
        if let Some(v) = s.filter(|v| (1..=5).contains(v)) {
            e.push(Edit::new(r, fields::score(i + 1), v));
        }
    }
    if !comment.trim().is_empty() {
        e.push(Edit::new(r, fields::COMMENT, comment.trim()));
    }
    e.push(Edit::new(r, SUBMITTED_ON, date.to_string()));
    e
}

/// The summary of a class.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Summary {
    /// Answers sent.
    pub n: usize,
    /// Mean of each statement (`None` with nobody scoring it).
    pub means: [Option<f64>; 5],
    /// Share of 4 and 5 in each statement.
    pub agree: [Option<f64>; 5],
    pub overall: Option<f64>,
    /// The comments, in a fixed order that does not follow who wrote them.
    pub comments: Vec<String>,
}

/// The summary of the answers sent in `class`; `None` with fewer than [`MIN_ANSWERS`].
pub fn summary(all: &[Reaction], class: Id) -> Option<Summary> {
    let sent: Vec<&Reaction> = all
        .iter()
        .filter(|r| r.sent && r.class == Some(class))
        .collect();
    if sent.len() < MIN_ANSWERS {
        return None;
    }
    let col = |i: usize| -> Vec<f64> {
        sent.iter()
            .filter_map(|r| r.scores[i].map(f64::from))
            .collect()
    };
    let means = std::array::from_fn(|i| {
        let c = col(i);
        (!c.is_empty()).then(|| c.iter().sum::<f64>() / c.len() as f64)
    });
    let agree = std::array::from_fn(|i| {
        let c = col(i);
        (!c.is_empty()).then(|| c.iter().filter(|v| **v >= 4.0).count() as f64 / c.len() as f64)
    });
    let all_scores: Vec<f64> = (0..5).flat_map(col).collect();
    let mut comments: Vec<String> = sent
        .iter()
        .map(|r| r.comment.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    comments.sort();
    Some(Summary {
        n: sent.len(),
        means,
        agree,
        overall: (!all_scores.is_empty())
            .then(|| all_scores.iter().sum::<f64>() / all_scores.len() as f64),
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perfis::{TrainingWriters, check_training};
    use crate::programa::testkit::*;

    #[test]
    fn answers_rules_and_summary() {
        let mut t = T::new();
        let class = t.id();
        let (a, b, c, d) = (t.id(), t.id(), t.id(), t.id());
        let mbb = t.id();
        let w = TrainingWriters {
            staff: vec![mbb],
            students: vec![a, b, c, d],
        };
        let today = crate::date::Date::new(2026, 11, 8).unwrap();
        let mut ids = Vec::new();
        for (p, s, com) in [
            (
                a,
                [Some(5), Some(4), Some(5), Some(3), Some(5)],
                "Muito bom",
            ),
            (
                b,
                [Some(4), Some(4), Some(4), Some(2), Some(4)],
                "Carga curta",
            ),
        ] {
            let id = t.id();
            let e = answer(class, id, p, &s, com, today);
            assert!(check_training(&t.s, p, &w, &e).is_ok());
            t.commit(e);
            ids.push(id);
        }
        // Two answers: no summary yet.
        assert_eq!(summary(&reactions(&t.s), class), None);
        // Someone else's answer, or changing one sent, is refused.
        let forged = answer(class, t.id(), a, &[Some(1); 5], "", today);
        assert!(check_training(&t.s, c, &w, &forged).is_err());
        let change = vec![Edit::new(Reaction::entity_ref(ids[0]), fields::score(1), 1)];
        assert!(check_training(&t.s, a, &w, &change).is_err());
        let id = t.id();
        t.commit(answer(
            class,
            id,
            c,
            &[Some(3), None, Some(4), Some(9), Some(4)],
            "",
            today,
        ));
        let s = summary(&reactions(&t.s), class).unwrap();
        assert_eq!(s.n, 3);
        assert_eq!(s.means[0], Some(4.0));
        assert_eq!(s.means[3], Some(2.5), "9 is out of the scale");
        assert_eq!(s.agree[1], Some(1.0));
        assert_eq!(
            s.comments,
            vec!["Carga curta".to_string(), "Muito bom".to_string()]
        );
        assert!(summary(&reactions(&t.s), t.id()).is_none());
    }
}
