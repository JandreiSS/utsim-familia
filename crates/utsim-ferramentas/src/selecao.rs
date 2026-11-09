//! Choosing among solutions or concepts (`docs/plano_lean_seis_sigma.md`, 2.4 and the
//! DMADV of 1.5): the weighted criteria matrix, the Pugh matrix against a reference concept
//! and the effort × impact grid. Pure: the apps read the criteria, options and scores from
//! their files and call [`compute`].

use serde::{Deserialize, Serialize};

/// How the options are scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Method {
    /// Each option gets 1 to 5 on each criterion; total = Σ weight × score.
    #[default]
    #[serde(rename = "ponderada")]
    Weighted,
    /// Each option is compared with the reference: better (+1), same (0) or worse (−1).
    #[serde(rename = "pugh")]
    Pugh,
}

impl Method {
    /// The scores allowed in a cell.
    pub fn scores(self) -> &'static [i8] {
        match self {
            Method::Weighted => &[1, 2, 3, 4, 5],
            Method::Pugh => &[-1, 0, 1],
        }
    }

    pub fn allows(self, v: i8) -> bool {
        self.scores().contains(&v)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Criterion {
    pub name: String,
    /// 1 to 10; absent counts as 1.
    pub weight: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionIn {
    pub name: String,
    /// The reference concept of the Pugh matrix (its cells are all "same").
    pub reference: bool,
    /// One per criterion, in the criteria's order; `None` when not scored yet.
    pub scores: Vec<Option<i8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionOut {
    pub name: String,
    pub reference: bool,
    /// Σ weight × score (Pugh: −1, 0, +1; the reference is 0).
    pub total: f64,
    /// Weighted: total over the highest possible total, in %. Pugh: 0.
    pub pct: f64,
    /// Pugh: counts of better, same and worse.
    pub plus: usize,
    pub same: usize,
    pub minus: usize,
    /// Cells not scored yet.
    pub missing: usize,
    /// Position in the ranking (1 = best).
    pub rank: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub method: Method,
    pub options: Vec<OptionOut>,
    /// Indices into `options`, best first (ties: entry order).
    pub ranking: Vec<usize>,
    /// Pugh without a reference, or with more than one.
    pub reference_issue: bool,
}

impl Selection {
    /// The best option, when every cell of it is scored.
    pub fn best(&self) -> Option<&OptionOut> {
        self.ranking
            .first()
            .map(|i| &self.options[*i])
            .filter(|o| o.missing == 0)
    }
}

/// The ranking of the options. Scores out of the method's range count as not scored.
pub fn compute(method: Method, criteria: &[Criterion], options: &[OptionIn]) -> Selection {
    let max_score = f64::from(*method.scores().iter().max().unwrap_or(&1));
    let weights: Vec<f64> = criteria
        .iter()
        .map(|c| f64::from(c.weight.clamp(1, 10)))
        .collect();
    let max_total: f64 = weights.iter().sum::<f64>() * max_score;
    let refs = options.iter().filter(|o| o.reference).count();
    let mut out: Vec<OptionOut> = options
        .iter()
        .map(|o| {
            let mut r = OptionOut {
                name: o.name.clone(),
                reference: o.reference,
                total: 0.0,
                pct: 0.0,
                plus: 0,
                same: 0,
                minus: 0,
                missing: 0,
                rank: 0,
            };
            for (j, w) in weights.iter().enumerate() {
                let s = if method == Method::Pugh && o.reference {
                    Some(0)
                } else {
                    o.scores
                        .get(j)
                        .copied()
                        .flatten()
                        .filter(|v| method.allows(*v))
                };
                match s {
                    None => r.missing += 1,
                    Some(v) => {
                        r.total += w * f64::from(v);
                        match v.cmp(&0) {
                            std::cmp::Ordering::Greater if method == Method::Pugh => r.plus += 1,
                            std::cmp::Ordering::Less => r.minus += 1,
                            std::cmp::Ordering::Equal => r.same += 1,
                            _ => {}
                        }
                    }
                }
            }
            if method == Method::Weighted && max_total > 0.0 {
                r.pct = r.total / max_total * 100.0;
            }
            r
        })
        .collect();
    let mut ranking: Vec<usize> = (0..out.len()).collect();
    ranking.sort_by(|a, b| out[*b].total.total_cmp(&out[*a].total).then(a.cmp(b)));
    for (pos, i) in ranking.iter().enumerate() {
        out[*i].rank = pos + 1;
    }
    Selection {
        method,
        options: out,
        ranking,
        reference_issue: method == Method::Pugh && refs != 1,
    }
}

/// The four squares of the effort × impact grid (impact and effort from 1 to 10, the middle
/// at 5,5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Quadrant {
    /// High impact, low effort: do it now.
    #[serde(rename = "fazer_ja")]
    QuickWin,
    /// High impact, high effort: plan it as a project.
    #[serde(rename = "planejar")]
    Plan,
    /// Low impact, low effort: do it when there is time.
    #[serde(rename = "se_sobrar_tempo")]
    Filler,
    /// Low impact, high effort: drop it.
    #[serde(rename = "descartar")]
    Drop,
}

/// The square of an option; `None` when impact or effort is out of 1 to 10.
pub fn quadrant(impact: u8, effort: u8) -> Option<Quadrant> {
    if !(1..=10).contains(&impact) || !(1..=10).contains(&effort) {
        return None;
    }
    let high_impact = impact > 5;
    let high_effort = effort > 5;
    Some(match (high_impact, high_effort) {
        (true, false) => Quadrant::QuickWin,
        (true, true) => Quadrant::Plan,
        (false, false) => Quadrant::Filler,
        (false, true) => Quadrant::Drop,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, weight: u8) -> Criterion {
        Criterion {
            name: name.into(),
            weight,
        }
    }

    fn o(name: &str, reference: bool, scores: &[Option<i8>]) -> OptionIn {
        OptionIn {
            name: name.into(),
            reference,
            scores: scores.to_vec(),
        }
    }

    #[test]
    fn weighted_matrix_by_hand() {
        // Weights 5, 3, 2; max total = 10 × 5 = 50.
        let cr = [c("Custo", 5), c("Prazo", 3), c("Risco", 2)];
        let s = compute(
            Method::Weighted,
            &cr,
            &[
                o("A", false, &[Some(4), Some(2), Some(5)]), // 20 + 6 + 10 = 36
                o("B", false, &[Some(5), Some(5), Some(1)]), // 25 + 15 + 2 = 42
                o("C", false, &[Some(3), None, Some(9)]),    // 15, two missing
            ],
        );
        assert_eq!(s.options[0].total, 36.0);
        assert_eq!(s.options[1].total, 42.0);
        assert!((s.options[0].pct - 72.0).abs() < 1e-9);
        assert_eq!(s.options[2].missing, 2, "9 is out of 1 to 5");
        assert_eq!(s.ranking, [1, 0, 2]);
        assert_eq!(s.best().unwrap().name, "B");
        assert!(!s.reference_issue);
    }

    #[test]
    fn pugh_against_the_reference() {
        let cr = [c("Custo", 3), c("Prazo", 1), c("Segurança", 5)];
        let s = compute(
            Method::Pugh,
            &cr,
            &[
                o("Atual", true, &[]),
                o("Robô", false, &[Some(-1), Some(1), Some(1)]), // −3 + 1 + 5 = 3
                o("Gabarito", false, &[Some(1), Some(0), Some(0)]), // 3
                o("Turno extra", false, &[Some(-1), Some(1), Some(-1)]), // −7
            ],
        );
        assert_eq!(s.options[0].total, 0.0);
        assert_eq!(s.options[0].same, 3);
        let robo = &s.options[1];
        assert_eq!((robo.plus, robo.same, robo.minus), (2, 0, 1));
        assert_eq!(robo.total, 3.0);
        assert_eq!(s.ranking, [1, 2, 0, 3], "ties keep the entry order");
        assert!(!s.reference_issue);
        let none = compute(Method::Pugh, &cr, &[o("X", false, &[Some(1)])]);
        assert!(none.reference_issue);
    }

    #[test]
    fn effort_and_impact() {
        assert_eq!(quadrant(8, 2), Some(Quadrant::QuickWin));
        assert_eq!(quadrant(8, 9), Some(Quadrant::Plan));
        assert_eq!(quadrant(5, 5), Some(Quadrant::Filler));
        assert_eq!(quadrant(2, 6), Some(Quadrant::Drop));
        assert_eq!(quadrant(0, 6), None);
    }
}
