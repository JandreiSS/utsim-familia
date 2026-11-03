//! Cause and effect matrix (`docs/plano_lean_seis_sigma.md`, 2.3): outputs Y weighted by
//! the customer (1 to 10), inputs X scored against each output with 0, 1, 3 or 9, total of
//! each input = Σ weight × score, the ranking of the inputs and the Pareto of the totals.
//!
//! Each score is a field of the input, `nota_<id of the output>`, so two people scoring
//! different cells never collide.

use serde_json::Value;

use super::pareto::{self, Pareto};
use super::{ItemRef, item_ref, items, rating};
use crate::id::Id;
use crate::op::Edit;
use crate::state::State;

pub const OUTPUT: &str = "saida";
pub const INPUT: &str = "entrada";

pub mod fields {
    pub const NAME: &str = "nome";
    /// Output weight, 1 to 10.
    pub const WEIGHT: &str = "peso";
}

/// The scores allowed in a cell.
pub const SCORES: [u8; 4] = [0, 1, 3, 9];

/// Field of the score of an input against `output`.
pub fn score_field(output: Id) -> String {
    let hex: String = output.to_string().chars().filter(|c| *c != '-').collect();
    format!("nota_{hex}")
}

/// A score read from a cell: only 0, 1, 3 and 9 count; anything else is no score.
pub fn score(input: &ItemRef, output: Id) -> Option<u8> {
    input
        .entity
        .integer(&score_field(output))
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| SCORES.contains(v))
}

/// Edit that scores a cell (`None` clears it).
pub fn set_score(input: Id, output: Id, v: Option<u8>) -> Edit {
    let value = v
        .filter(|v| SCORES.contains(v))
        .map_or(Value::from(""), Value::from);
    Edit::new(item_ref(input), score_field(output), value)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub id: Id,
    pub name: String,
    /// Absent weight counts as 1.
    pub weight: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    pub id: Id,
    pub name: String,
    /// One per output, in the outputs' order.
    pub scores: Vec<Option<u8>>,
    pub total: f64,
    /// Position in the ranking (1 = highest total).
    pub rank: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub outputs: Vec<Output>,
    /// In the order they were entered.
    pub inputs: Vec<Input>,
    /// Indices into `inputs`, highest total first (ties: entry order).
    pub ranking: Vec<usize>,
    pub pareto: Pareto,
}

impl Matrix {
    /// Total of each output column: Σ score (unweighted), to see which Y the inputs explain.
    pub fn column_sums(&self) -> Vec<u32> {
        (0..self.outputs.len())
            .map(|j| {
                self.inputs
                    .iter()
                    .map(|i| u32::from(i.scores[j].unwrap_or(0)))
                    .sum()
            })
            .collect()
    }
}

/// Computes the matrix from weights and scores (pure; used by [`of_deliverable`]).
pub fn compute(outputs: Vec<Output>, inputs: Vec<(Id, String, Vec<Option<u8>>)>) -> Matrix {
    let mut rows: Vec<Input> = inputs
        .into_iter()
        .map(|(id, name, scores)| {
            let total = outputs
                .iter()
                .zip(&scores)
                .map(|(o, s)| f64::from(o.weight) * f64::from(s.unwrap_or(0)))
                .sum();
            Input {
                id,
                name,
                scores,
                total,
                rank: 0,
            }
        })
        .collect();
    let mut ranking: Vec<usize> = (0..rows.len()).collect();
    ranking.sort_by(|a, b| rows[*b].total.total_cmp(&rows[*a].total).then(a.cmp(b)));
    for (pos, i) in ranking.iter().enumerate() {
        rows[*i].rank = pos + 1;
    }
    let data: Vec<(String, f64)> = rows
        .iter()
        .map(|r| {
            let name = if r.name.trim().is_empty() {
                format!("X{}", r.rank)
            } else {
                r.name.clone()
            };
            (name, r.total)
        })
        .collect();
    Matrix {
        outputs,
        inputs: rows,
        ranking,
        pareto: pareto::pareto(&data, 0.0),
    }
}

/// The matrix of a deliverable.
pub fn of_deliverable(state: &State, deliverable: Id) -> Matrix {
    let outputs: Vec<Output> = items(state, deliverable, OUTPUT)
        .iter()
        .map(|o| Output {
            id: o.id,
            name: o.text(fields::NAME),
            weight: rating(o.entity, fields::WEIGHT).unwrap_or(1),
        })
        .collect();
    let inputs = items(state, deliverable, INPUT)
        .iter()
        .map(|i| {
            let scores = outputs.iter().map(|o| score(i, o.id)).collect();
            (i.id, i.text(fields::NAME), scores)
        })
        .collect();
    compute(outputs, inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    fn id(n: u8) -> Id {
        Id::v7(u64::from(n), [n; 10])
    }

    #[test]
    fn hand_computed_example() {
        // Outputs: diâmetro (10), acabamento (6), prazo (3).
        let outputs = vec![
            Output {
                id: id(1),
                name: "Diâmetro".into(),
                weight: 10,
            },
            Output {
                id: id(2),
                name: "Acabamento".into(),
                weight: 6,
            },
            Output {
                id: id(3),
                name: "Prazo".into(),
                weight: 3,
            },
        ];
        let inputs = vec![
            // 10·9 + 6·3 + 3·0 = 108
            (
                id(10),
                "Temperatura".into(),
                vec![Some(9), Some(3), Some(0)],
            ),
            // 10·1 + 6·9 + 3·1 = 67
            (id(11), "Ferramenta".into(), vec![Some(1), Some(9), Some(1)]),
            // 10·3 + 6·1 + 3·9 = 63
            (id(12), "Setup".into(), vec![Some(3), Some(1), Some(9)]),
            // blanks count as 0: 10·9 = 90
            (id(13), "Matéria-prima".into(), vec![Some(9), None, None]),
        ];
        let m = compute(outputs, inputs);
        let totals: Vec<f64> = m.inputs.iter().map(|i| i.total).collect();
        assert_eq!(totals, [108.0, 67.0, 63.0, 90.0]);
        assert_eq!(m.ranking, [0, 3, 1, 2]);
        let ranks: Vec<usize> = m.inputs.iter().map(|i| i.rank).collect();
        assert_eq!(ranks, [1, 3, 4, 2]);
        assert_eq!(m.column_sums(), [22, 13, 10]);
        let names: Vec<&str> = m.pareto.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            ["Temperatura", "Matéria-prima", "Ferramenta", "Setup"]
        );
        assert_eq!(m.pareto.total, 328.0);
    }

    #[test]
    fn ranking_is_sorted_and_stable() {
        let outputs = vec![Output {
            id: id(1),
            name: "Y".into(),
            weight: 5,
        }];
        let inputs: Vec<(Id, String, Vec<Option<u8>>)> = [3u8, 9, 0, 9, 1, 3]
            .iter()
            .enumerate()
            .map(|(k, s)| (id(10 + k as u8), format!("x{k}"), vec![Some(*s)]))
            .collect();
        let m = compute(outputs, inputs);
        let t: Vec<f64> = m.ranking.iter().map(|i| m.inputs[*i].total).collect();
        assert!(t.windows(2).all(|w| w[0] >= w[1]));
        assert_eq!(m.ranking, [1, 3, 0, 5, 4, 2], "ties keep entry order");
    }

    #[test]
    fn cells_through_the_journal() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::CauseEffect);
        let y1 = f.item(
            d,
            OUTPUT,
            None,
            vec![(fields::NAME, "Y1".into()), (fields::WEIGHT, 10.into())],
        );
        let y2 = f.item(d, OUTPUT, None, vec![(fields::NAME, "Y2".into())]);
        let x = f.item(d, INPUT, None, vec![(fields::NAME, "X".into())]);
        f.commit(vec![set_score(x, y1, Some(9)), set_score(x, y2, Some(3))]);
        let m = of_deliverable(&f.state, d);
        assert_eq!(m.outputs[1].weight, 1, "absent weight counts 1");
        assert_eq!(m.inputs[0].scores, [Some(9), Some(3)]);
        assert_eq!(m.inputs[0].total, 93.0);
        // Not an allowed score: ignored.
        f.set(x, &score_field(y2), 5);
        assert_eq!(of_deliverable(&f.state, d).inputs[0].scores[1], None);
        f.commit(vec![set_score(x, y1, Some(4))]);
        assert_eq!(
            of_deliverable(&f.state, d).inputs[0].scores[0],
            None,
            "4 clears"
        );
        assert!(score_field(y1).len() <= crate::op::MAX_FIELD_LEN);
        assert!(crate::op::validate_field(&score_field(y1)).is_ok());
    }
}
