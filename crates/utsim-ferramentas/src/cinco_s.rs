//! 5S audits (`docs/plano_lean_seis_sigma.md`, 2.4): a checklist of questions per sense,
//! each audit scores every question from 0 to 4, and the result is the sum, the mean and the
//! percentage of each sense (for the radar) and of the audit, compared over time. The
//! checklist is the company's; [`STARTER`] is only a starting point, in our own wording.

use serde::{Deserialize, Serialize};

/// The five senses, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Sense {
    /// Sort: keep only what is used.
    #[serde(rename = "utilizacao")]
    Sort,
    /// Set in order: a place for everything.
    #[serde(rename = "organizacao")]
    Order,
    /// Shine: clean and inspect while cleaning.
    #[serde(rename = "limpeza")]
    Shine,
    /// Standardise: the first three made routine and visible.
    #[serde(rename = "padronizacao")]
    Standardise,
    /// Sustain: the habit, checked.
    #[serde(rename = "disciplina")]
    Sustain,
}

impl Sense {
    pub const ALL: [Sense; 5] = [
        Sense::Sort,
        Sense::Order,
        Sense::Shine,
        Sense::Standardise,
        Sense::Sustain,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Sense::Sort => "utilizacao",
            Sense::Order => "organizacao",
            Sense::Shine => "limpeza",
            Sense::Standardise => "padronizacao",
            Sense::Sustain => "disciplina",
        }
    }

    pub fn from_key(k: &str) -> Option<Sense> {
        Sense::ALL.into_iter().find(|s| s.key() == k)
    }

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Sense::Sort => "Utilização",
            Sense::Order => "Organização",
            Sense::Shine => "Limpeza",
            Sense::Standardise => "Padronização",
            Sense::Sustain => "Disciplina",
        }
    }
}

/// Highest score of a question.
pub const MAX_SCORE: u8 = 4;

/// A starting checklist, two questions per sense.
pub const STARTER: [(Sense, &str); 10] = [
    (
        Sense::Sort,
        "Só há na área o que é usado no trabalho do dia",
    ),
    (
        Sense::Sort,
        "O que não é usado está identificado e tem destino",
    ),
    (
        Sense::Order,
        "Cada ferramenta e material tem lugar marcado e está nele",
    ),
    (
        Sense::Order,
        "Corredores e faixas estão livres e demarcados",
    ),
    (Sense::Shine, "Piso, máquinas e bancadas estão limpos"),
    (
        Sense::Shine,
        "As fontes de sujeira e vazamento estão registradas e tratadas",
    ),
    (
        Sense::Standardise,
        "Há padrão visual da área (fotos, etiquetas, cores) e ele é seguido",
    ),
    (
        Sense::Standardise,
        "A rotina de limpeza e arrumação tem responsável e frequência",
    ),
    (
        Sense::Sustain,
        "As auditorias acontecem na frequência combinada",
    ),
    (
        Sense::Sustain,
        "As ações das auditorias anteriores foram concluídas",
    ),
];

/// A question of the checklist with its score in one audit (`None`: not scored).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub sense: Sense,
    pub score: Option<u8>,
}

/// The result of one sense.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SenseScore {
    pub sense: Sense,
    pub sum: u32,
    pub questions: usize,
    pub scored: usize,
    /// Mean of the scored questions (0 to 4); `None` when nothing scored.
    pub mean: Option<f64>,
    /// Sum over the highest possible sum of the scored questions, in %.
    pub pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditScore {
    /// One per sense, in [`Sense::ALL`] order.
    pub senses: Vec<SenseScore>,
    pub sum: u32,
    pub scored: usize,
    pub questions: usize,
    pub pct: Option<f64>,
}

impl AuditScore {
    /// The sense with the lowest percentage (where to act first).
    pub fn weakest(&self) -> Option<Sense> {
        self.senses
            .iter()
            .filter_map(|s| s.pct.map(|p| (s.sense, p)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(s, _)| s)
    }

    pub fn complete(&self) -> bool {
        self.questions > 0 && self.scored == self.questions
    }
}

/// The scores of one audit. Scores above [`MAX_SCORE`] count as not scored.
pub fn score(answers: &[Answer]) -> AuditScore {
    let pct = |sum: u32, n: usize| {
        (n > 0).then(|| f64::from(sum) / (n as f64 * f64::from(MAX_SCORE)) * 100.0)
    };
    let senses: Vec<SenseScore> = Sense::ALL
        .iter()
        .map(|s| {
            let qs: Vec<&Answer> = answers.iter().filter(|a| a.sense == *s).collect();
            let vals: Vec<u8> = qs
                .iter()
                .filter_map(|a| a.score.filter(|v| *v <= MAX_SCORE))
                .collect();
            let sum: u32 = vals.iter().map(|v| u32::from(*v)).sum();
            SenseScore {
                sense: *s,
                sum,
                questions: qs.len(),
                scored: vals.len(),
                mean: (!vals.is_empty()).then(|| f64::from(sum) / vals.len() as f64),
                pct: pct(sum, vals.len()),
            }
        })
        .collect();
    let sum = senses.iter().map(|s| s.sum).sum();
    let scored = senses.iter().map(|s| s.scored).sum();
    AuditScore {
        sum,
        scored,
        questions: answers.len(),
        pct: pct(sum, scored),
        senses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(sense: Sense, score: Option<u8>) -> Answer {
        Answer { sense, score }
    }

    #[test]
    fn sums_means_and_percentages_by_hand() {
        let r = score(&[
            a(Sense::Sort, Some(4)),
            a(Sense::Sort, Some(2)),
            a(Sense::Order, Some(1)),
            a(Sense::Order, None),
            a(Sense::Shine, Some(9)), // out of range: not scored
            a(Sense::Sustain, Some(3)),
        ]);
        let sort = r.senses[0];
        assert_eq!((sort.sum, sort.scored, sort.mean), (6, 2, Some(3.0)));
        assert_eq!(sort.pct, Some(75.0));
        assert_eq!(r.senses[1].pct, Some(25.0));
        assert_eq!(r.senses[2].mean, None);
        assert_eq!(r.senses[3].questions, 0);
        assert_eq!((r.sum, r.scored, r.questions), (10, 4, 6));
        assert_eq!(r.pct, Some(62.5));
        assert_eq!(r.weakest(), Some(Sense::Order));
        assert!(!r.complete());
    }

    #[test]
    fn starter_and_keys() {
        for s in Sense::ALL {
            assert_eq!(Sense::from_key(s.key()), Some(s));
            assert_eq!(STARTER.iter().filter(|(x, _)| *x == s).count(), 2);
            assert_eq!(
                serde_json::to_value(s).unwrap(),
                serde_json::Value::from(s.key())
            );
        }
    }
}
