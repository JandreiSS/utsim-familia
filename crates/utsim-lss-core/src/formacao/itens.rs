//! Analysis of the items of the exams (P9 of `docs/plano_lean_seis_sigma.md`): from the
//! graded papers (the right or wrong of each item, kept with each result), the **difficulty**
//! of each question (the share of right answers), its **discrimination** (the share of
//! right answers among the best 27 % of the papers minus among the worst 27 %, and the
//! correlation of the item with the score of the rest of the paper) and what to do with it:
//! a question nobody misses or everybody misses teaches nothing about who learned; one that
//! the weaker trainees get more often than the stronger ones has a problem in the wording
//! or in the key.

use std::collections::BTreeMap;

use super::exams::ExamResult;
use crate::id::Id;

/// Fewer papers than this and the indices are only shown, not judged.
pub const MIN_PAPERS: usize = 10;

/// What the analysis says about a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// Discriminates and is neither too easy nor too hard.
    Good,
    /// Almost everybody gets it (more than 90 %).
    TooEasy,
    /// Almost nobody gets it (less than 20 %).
    TooHard,
    /// Discrimination below 0,20: the strong and the weak get it alike.
    Weak,
    /// The weak get it more often than the strong: check the wording and the key.
    Negative,
    /// Fewer than [`MIN_PAPERS`] papers.
    FewPapers,
}

/// The indices of one question.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStat {
    pub question: Id,
    /// Papers where the question was drawn.
    pub n: usize,
    /// Share of right answers, 0 to 1.
    pub difficulty: f64,
    /// Right share of the upper 27 % minus of the lower 27 % (by the rest of the paper).
    pub discrimination: Option<f64>,
    /// Correlation of the item (0/1) with the share of right answers in the other items.
    pub item_rest: Option<f64>,
    pub flag: Flag,
}

fn pearson(x: &[f64], y: &[f64]) -> Option<f64> {
    let n = x.len() as f64;
    if x.len() < 3 {
        return None;
    }
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let sxx: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    let syy: f64 = y.iter().map(|b| (b - my).powi(2)).sum();
    (sxx > 0.0 && syy > 0.0).then(|| sxy / (sxx * syy).sqrt())
}

/// The analysis of every question in `results` (each paper's detail; the latest grading of
/// each paper is in `results` once), in order of question id.
pub fn analysis(results: &[ExamResult]) -> Vec<ItemStat> {
    // question → [(right, rest score)]
    let mut by: BTreeMap<Id, Vec<(bool, f64)>> = BTreeMap::new();
    for r in results {
        let total = r.detail.len();
        if total < 2 {
            continue;
        }
        let right = r.detail.iter().filter(|i| i.right).count();
        for it in &r.detail {
            let rest = (right - usize::from(it.right)) as f64 / (total - 1) as f64;
            by.entry(it.question).or_default().push((it.right, rest));
        }
    }
    by.into_iter()
        .map(|(question, mut v)| {
            let n = v.len();
            let p = v.iter().filter(|x| x.0).count() as f64 / n as f64;
            let x: Vec<f64> = v.iter().map(|x| if x.0 { 1.0 } else { 0.0 }).collect();
            let y: Vec<f64> = v.iter().map(|x| x.1).collect();
            let item_rest = pearson(&x, &y);
            v.sort_by(|a, b| a.1.total_cmp(&b.1));
            let g = ((n as f64) * 0.27).round().max(1.0) as usize;
            let discrimination = (n >= 4).then(|| {
                let share =
                    |s: &[(bool, f64)]| s.iter().filter(|x| x.0).count() as f64 / s.len() as f64;
                share(&v[n - g..]) - share(&v[..g])
            });
            let flag = if n < MIN_PAPERS {
                Flag::FewPapers
            } else if discrimination.is_some_and(|d| d < 0.0) {
                Flag::Negative
            } else if p > 0.9 {
                Flag::TooEasy
            } else if p < 0.2 {
                Flag::TooHard
            } else if discrimination.is_some_and(|d| d < 0.2) {
                Flag::Weak
            } else {
                Flag::Good
            };
            ItemStat {
                question,
                n,
                difficulty: p,
                discrimination,
                item_rest,
                flag,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formacao::exams::ItemGrade;
    use serde_json::Value;

    fn q(n: u8) -> Id {
        Id::v7(u64::from(n), [n; 10])
    }

    fn paper(rights: &[bool]) -> ExamResult {
        ExamResult {
            id: Id::v7(1, [0; 10]),
            exam: None,
            person: None,
            class: None,
            score: 0.0,
            passed: false,
            mode: Default::default(),
            graded_by: None,
            graded_on: None,
            detail: rights
                .iter()
                .enumerate()
                .map(|(i, r)| ItemGrade {
                    question: q(i as u8 + 1),
                    right: *r,
                    expected: Value::Null,
                    given: Value::Null,
                })
                .collect(),
        }
    }

    #[test]
    fn difficulty_discrimination_and_flags() {
        // Twenty papers, ability k = 0..19 (k-th paper gets the first k/4 of the five
        // "ladder" items right). Q1 everybody gets; Q2 the strong get; Q3 the weak get.
        let mut res = Vec::new();
        for k in 0..20 {
            let strong = k >= 10;
            let ladder: Vec<bool> = (0..5).map(|i| k >= 4 * i).collect();
            let mut r = vec![true, strong, !strong];
            r.extend(ladder);
            res.push(paper(&r));
        }
        let a = analysis(&res);
        let get = |n: u8| a.iter().find(|s| s.question == q(n)).unwrap();
        assert_eq!(get(1).difficulty, 1.0);
        assert_eq!(get(1).flag, Flag::TooEasy);
        assert_eq!(get(2).flag, Flag::Good);
        assert!(get(2).discrimination.unwrap() > 0.9);
        assert!(get(2).item_rest.unwrap() > 0.5);
        assert_eq!(get(3).flag, Flag::Negative);
        assert!(get(3).discrimination.unwrap() < -0.9);
        assert_eq!(get(1).n, 20);
        // Few papers: shown, not judged.
        let few = analysis(&res[..5]);
        assert!(few.iter().all(|s| s.flag == Flag::FewPapers));
        assert!(analysis(&[]).is_empty());
    }
}
