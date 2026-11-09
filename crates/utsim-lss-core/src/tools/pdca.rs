//! PDCA cycles (`docs/plano_lean_seis_sigma.md`, 1.5, etapa P6): four light steps without a
//! formal tollgate. Each cycle records the hypothesis and the plan, what was done, what was
//! checked and what was acted on (standardised or adjusted), with the verdict on the
//! hypothesis. Cycles are chained in order in the same deliverable: a new one starts from
//! what the previous one learned.

use serde::{Deserialize, Serialize};

use super::items;
use crate::id::Id;
use crate::state::State;

pub const CYCLE: &str = "ciclo";

pub mod fields {
    pub const HYPOTHESIS: &str = "hipotese";
    pub const PLAN: &str = "planejar";
    pub const DO: &str = "fazer";
    pub const CHECK: &str = "verificar";
    pub const ACT: &str = "agir";
    pub const VERDICT: &str = "resultado";
}

/// Where a cycle is: the first step still empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    Plan,
    Do,
    Check,
    Act,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Verdict {
    /// The change worked: standardise it.
    #[serde(rename = "confirmada")]
    Confirmed,
    /// It did not: the next cycle tries something else.
    #[serde(rename = "refutada")]
    Refuted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cycle {
    pub id: Id,
    /// 1 based, in order.
    pub number: usize,
    pub hypothesis: String,
    pub step: Step,
    pub verdict: Option<Verdict>,
    /// What the previous cycle acted on, to start from.
    pub previous_act: Option<String>,
}

fn step_of(texts: [&str; 4], verdict: Option<Verdict>) -> Step {
    let [plan, doing, check, act] = texts.map(|t| !t.trim().is_empty());
    if !plan {
        Step::Plan
    } else if !doing {
        Step::Do
    } else if !check || verdict.is_none() {
        Step::Check
    } else if !act {
        Step::Act
    } else {
        Step::Closed
    }
}

pub fn cycles(state: &State, del: Id) -> Vec<Cycle> {
    let mut prev: Option<String> = None;
    items(state, del, CYCLE)
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let verdict = c.entity.typed(fields::VERDICT);
            let plan = format!("{}{}", c.text(fields::HYPOTHESIS), c.text(fields::PLAN));
            let step = step_of(
                [
                    plan.as_str(),
                    &c.text(fields::DO),
                    &c.text(fields::CHECK),
                    &c.text(fields::ACT),
                ],
                verdict,
            );
            let act = c.text(fields::ACT);
            let out = Cycle {
                id: c.id,
                number: i + 1,
                hypothesis: c.text(fields::HYPOTHESIS),
                step,
                verdict,
                previous_act: prev.take(),
            };
            prev = (!act.trim().is_empty()).then_some(act);
            out
        })
        .collect()
}

/// Counts of the cycles: confirmed, refuted, still open.
pub fn summary(cs: &[Cycle]) -> (usize, usize, usize) {
    let confirmed = cs
        .iter()
        .filter(|c| c.verdict == Some(Verdict::Confirmed))
        .count();
    let refuted = cs
        .iter()
        .filter(|c| c.verdict == Some(Verdict::Refuted))
        .count();
    let open = cs.iter().filter(|c| c.step != Step::Closed).count();
    (confirmed, refuted, open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    #[test]
    fn steps_verdicts_and_the_chain() {
        let mut fx = Fixture::new();
        let d = fx.deliverable(Tool::Pdca);
        let a = fx.item(
            d,
            CYCLE,
            None,
            vec![
                (fields::HYPOTHESIS, "A fila cai com um segundo caixa".into()),
                (fields::PLAN, "Abrir o caixa 2 das 11 às 14 h".into()),
                (fields::DO, "Aberto por duas semanas".into()),
                (fields::CHECK, "Fila média de 9 para 4 clientes".into()),
            ],
        );
        let cs = cycles(&fx.state, d);
        assert_eq!(cs[0].step, Step::Check, "no verdict yet");
        fx.set(a, fields::VERDICT, "confirmada");
        assert_eq!(cycles(&fx.state, d)[0].step, Step::Act);
        fx.set(a, fields::ACT, "Escala do caixa 2 no padrão do turno");
        fx.item(
            d,
            CYCLE,
            None,
            vec![(fields::HYPOTHESIS, "O pico das 18 h também cai".into())],
        );
        let cs = cycles(&fx.state, d);
        assert_eq!(cs[0].step, Step::Closed);
        assert_eq!(cs[1].number, 2);
        assert_eq!(cs[1].step, Step::Do, "the hypothesis counts as the plan");
        assert_eq!(
            cs[1].previous_act.as_deref(),
            Some("Escala do caixa 2 no padrão do turno")
        );
        assert_eq!(summary(&cs), (1, 0, 1));
    }
}
