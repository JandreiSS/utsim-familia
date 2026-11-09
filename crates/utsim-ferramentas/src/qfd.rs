//! The house of quality, QFD (`docs/plano_lean_seis_sigma.md`, 1.5, the Measure phase of
//! the DMADV): the customer needs with their importance (the rows), the technical
//! requirements with their direction and target (the columns), the relationship of each
//! need with each requirement (0, 1, 3 or 9), the correlations between requirements (the
//! roof) and, optionally, how the current product and the competitors meet each need.
//! [`compute`] gives the weight of each requirement (Σ importance × relationship), its
//! share and ranking, the improvement ratio and sales point of each need, and the points
//! the team should look at (needs without a requirement, requirements without a need,
//! conflicting requirements among the most important).
//!
//! The types are the exchange format of the tool (serde, `Default`); the screens live in
//! the quality app and draw the numbers.

use serde::{Deserialize, Serialize};

/// The strengths allowed in a relationship cell.
pub const RELATIONSHIPS: [u8; 4] = [0, 1, 3, 9];

/// What a requirement should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    #[serde(rename = "maior")]
    Higher,
    #[serde(rename = "menor")]
    Lower,
    #[serde(rename = "alvo")]
    Target,
}

/// A correlation of the roof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Correlation {
    /// Improving one improves the other a lot.
    #[serde(rename = "++")]
    StrongPositive,
    #[serde(rename = "+")]
    Positive,
    #[serde(rename = "-")]
    Negative,
    /// Improving one worsens the other a lot: a trade-off to resolve.
    #[serde(rename = "--")]
    StrongNegative,
}

impl Correlation {
    pub fn negative(self) -> bool {
        matches!(self, Correlation::Negative | Correlation::StrongNegative)
    }
}

/// A customer need (a row).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Need {
    #[serde(rename = "texto")]
    pub text: String,
    /// 1 to 5.
    #[serde(rename = "importancia")]
    pub importance: f64,
    /// How the current product meets it, 1 to 5 (optional).
    #[serde(rename = "atual")]
    pub current: Option<f64>,
    /// How the best competitor meets it, 1 to 5 (optional).
    #[serde(rename = "concorrente")]
    pub competitor: Option<f64>,
    /// Where the team wants to be, 1 to 5 (optional).
    #[serde(rename = "meta")]
    pub goal: Option<f64>,
    /// A strong selling point: 1.5; a moderate one: 1.2; none: 1 (optional).
    #[serde(rename = "ponto_de_venda")]
    pub sales_point: Option<f64>,
}

/// A technical requirement (a column).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Requirement {
    #[serde(rename = "texto")]
    pub text: String,
    #[serde(rename = "unidade")]
    pub unit: String,
    #[serde(rename = "sentido")]
    pub direction: Direction,
    #[serde(rename = "alvo")]
    pub target: Option<f64>,
}

/// A roof cell: the correlation between requirements `a` and `b` (indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoofCell {
    pub a: usize,
    pub b: usize,
    #[serde(rename = "correlacao")]
    pub correlation: Correlation,
}

/// The house.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct House {
    #[serde(rename = "necessidades")]
    pub needs: Vec<Need>,
    #[serde(rename = "requisitos")]
    pub requirements: Vec<Requirement>,
    /// `relationships[need][requirement]`, 0, 1, 3 or 9; missing cells count as 0.
    #[serde(rename = "relacoes")]
    pub relationships: Vec<Vec<u8>>,
    #[serde(rename = "telhado")]
    pub roof: Vec<RoofCell>,
}

impl House {
    /// The relationship of a cell, 0 when absent or not one of 0, 1, 3, 9.
    pub fn relationship(&self, need: usize, req: usize) -> u8 {
        self.relationships
            .get(need)
            .and_then(|r| r.get(req))
            .copied()
            .filter(|v| RELATIONSHIPS.contains(v))
            .unwrap_or(0)
    }
}

/// The result of one need.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NeedOut {
    /// Goal over current, when both are given.
    pub improvement_ratio: Option<f64>,
    /// Importance × improvement ratio × sales point (the planning weight).
    pub weight: f64,
    /// `weight` over the sum, in %.
    pub weight_pct: f64,
}

/// The result of one requirement.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RequirementOut {
    /// Σ need weight × relationship.
    pub weight: f64,
    pub weight_pct: f64,
    /// 1 = most important.
    pub rank: usize,
}

/// A point the team should look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Finding {
    /// A need no requirement answers (index): the design does not deal with it.
    NeedUncovered(usize),
    /// A need answered only weakly (no 9 or 3 in the row).
    NeedWeak(usize),
    /// A requirement related to no need: why measure it?
    RequirementUnused(usize),
    /// A requirement without a target.
    NoTarget(usize),
    /// Two requirements among the most important that work against each other.
    Conflict(usize, usize),
}

impl Finding {
    /// The text in Portuguese (the quality app may use its own catalogue).
    pub fn text(&self, h: &House) -> String {
        let need = |i: &usize| h.needs.get(*i).map_or("?", |n| n.text.as_str());
        let req = |i: &usize| h.requirements.get(*i).map_or("?", |r| r.text.as_str());
        match self {
            Finding::NeedUncovered(i) => {
                format!("Nenhum requisito atende à necessidade \"{}\".", need(i))
            }
            Finding::NeedWeak(i) => format!(
                "A necessidade \"{}\" só tem relações fracas: falta um requisito que a meça bem.",
                need(i)
            ),
            Finding::RequirementUnused(i) => format!(
                "O requisito \"{}\" não se relaciona com nenhuma necessidade.",
                req(i)
            ),
            Finding::NoTarget(i) => format!("O requisito \"{}\" está sem valor-alvo.", req(i)),
            Finding::Conflict(a, b) => format!(
                "\"{}\" e \"{}\" estão entre os mais importantes e trabalham um contra o outro: resolva o conflito no projeto.",
                req(a),
                req(b)
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Outcome {
    pub needs: Vec<NeedOut>,
    pub requirements: Vec<RequirementOut>,
    /// Indices of the requirements, most important first (ties: input order).
    pub ranking: Vec<usize>,
    pub findings: Vec<Finding>,
}

/// How many of the top requirements are checked for conflicts in the roof.
pub const TOP: usize = 5;

/// The house of quality. Pure.
pub fn compute(h: &House) -> Outcome {
    let needs: Vec<NeedOut> = h
        .needs
        .iter()
        .map(|n| {
            let ratio = match (n.goal, n.current) {
                (Some(g), Some(c)) if c > 0.0 => Some(g / c),
                _ => None,
            };
            let imp = if n.importance.is_finite() && n.importance > 0.0 {
                n.importance
            } else {
                0.0
            };
            let sp = n.sales_point.filter(|s| *s > 0.0).unwrap_or(1.0);
            NeedOut {
                improvement_ratio: ratio,
                weight: imp * ratio.unwrap_or(1.0) * sp,
                weight_pct: 0.0,
            }
        })
        .collect();
    let need_total: f64 = needs.iter().map(|n| n.weight).sum();
    let needs: Vec<NeedOut> = needs
        .into_iter()
        .map(|mut n| {
            if need_total > 0.0 {
                n.weight_pct = n.weight / need_total * 100.0;
            }
            n
        })
        .collect();
    let mut reqs: Vec<RequirementOut> = (0..h.requirements.len())
        .map(|j| RequirementOut {
            weight: (0..h.needs.len())
                .map(|i| needs[i].weight * f64::from(h.relationship(i, j)))
                .sum(),
            ..RequirementOut::default()
        })
        .collect();
    let total: f64 = reqs.iter().map(|r| r.weight).sum();
    let mut ranking: Vec<usize> = (0..reqs.len()).collect();
    ranking.sort_by(|a, b| reqs[*b].weight.total_cmp(&reqs[*a].weight).then(a.cmp(b)));
    for (pos, j) in ranking.iter().enumerate() {
        reqs[*j].rank = pos + 1;
        if total > 0.0 {
            reqs[*j].weight_pct = reqs[*j].weight / total * 100.0;
        }
    }
    let mut findings = Vec::new();
    for i in 0..h.needs.len() {
        let row: Vec<u8> = (0..h.requirements.len())
            .map(|j| h.relationship(i, j))
            .collect();
        if row.iter().all(|v| *v == 0) {
            findings.push(Finding::NeedUncovered(i));
        } else if row.iter().all(|v| *v <= 1) {
            findings.push(Finding::NeedWeak(i));
        }
    }
    for (j, r) in h.requirements.iter().enumerate() {
        if (0..h.needs.len()).all(|i| h.relationship(i, j) == 0) {
            findings.push(Finding::RequirementUnused(j));
        }
        if r.direction == Direction::Target && r.target.is_none() {
            findings.push(Finding::NoTarget(j));
        }
    }
    let top: Vec<usize> = ranking
        .iter()
        .copied()
        .filter(|j| reqs[*j].weight > 0.0)
        .take(TOP)
        .collect();
    for c in &h.roof {
        if c.a != c.b && c.correlation.negative() && top.contains(&c.a) && top.contains(&c.b) {
            let (a, b) = (c.a.min(c.b), c.a.max(c.b));
            if !findings.contains(&Finding::Conflict(a, b)) {
                findings.push(Finding::Conflict(a, b));
            }
        }
    }
    Outcome {
        needs,
        requirements: reqs,
        ranking,
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn need(text: &str, importance: f64) -> Need {
        Need {
            text: text.into(),
            importance,
            ..Need::default()
        }
    }

    fn req(text: &str) -> Requirement {
        Requirement {
            text: text.into(),
            ..Requirement::default()
        }
    }

    /// A kettle: three needs, four requirements.
    fn kettle() -> House {
        House {
            needs: vec![
                need("Ferve rápido", 5.0),
                need("Não queima a mão", 4.0),
                need("Fácil de limpar", 2.0),
            ],
            requirements: vec![
                req("Potência (W)"),
                req("Temperatura da alça (°C)"),
                req("Abertura da tampa (mm)"),
                req("Cor"),
            ],
            relationships: vec![vec![9, 1, 0, 0], vec![3, 9, 0], vec![0, 0, 9, 0]],
            roof: vec![RoofCell {
                a: 1,
                b: 0,
                correlation: Correlation::StrongNegative,
            }],
        }
    }

    #[test]
    fn weights_by_hand() {
        let o = compute(&kettle());
        // Potência: 5·9 + 4·3 = 57; alça: 5·1 + 4·9 = 41; tampa: 2·9 = 18; cor: 0.
        let w: Vec<f64> = o.requirements.iter().map(|r| r.weight).collect();
        assert_eq!(w, [57.0, 41.0, 18.0, 0.0]);
        assert_eq!(o.ranking, [0, 1, 2, 3]);
        assert!((o.requirements[0].weight_pct - 57.0 / 116.0 * 100.0).abs() < 1e-9);
        assert_eq!(o.requirements[3].rank, 4);
        assert!(o.findings.contains(&Finding::RequirementUnused(3)));
        assert!(o.findings.contains(&Finding::Conflict(0, 1)));
        assert!(
            !o.findings
                .iter()
                .any(|f| matches!(f, Finding::NeedUncovered(_)))
        );
        assert!(Finding::Conflict(0, 1).text(&kettle()).contains("Potência"));
    }

    #[test]
    fn planning_weights_and_findings() {
        let mut h = kettle();
        // "Fácil de limpar": current 2, goal 4, strong selling point → 2 × 2 × 1,5 = 6.
        h.needs[2].current = Some(2.0);
        h.needs[2].goal = Some(4.0);
        h.needs[2].sales_point = Some(1.5);
        h.needs.push(need("Bonita", 3.0));
        h.relationships.push(vec![0, 0, 0, 1]);
        h.requirements[2].direction = Direction::Target;
        let o = compute(&h);
        assert_eq!(o.needs[2].improvement_ratio, Some(2.0));
        assert_eq!(o.needs[2].weight, 6.0);
        assert_eq!(o.requirements[2].weight, 54.0);
        assert!(o.findings.contains(&Finding::NeedWeak(3)));
        assert!(o.findings.contains(&Finding::NoTarget(2)));
        assert!(!o.findings.contains(&Finding::RequirementUnused(3)));
        let total: f64 = o.needs.iter().map(|n| n.weight_pct).sum();
        assert!((total - 100.0).abs() < 1e-9);
    }

    #[test]
    fn bad_cells_and_the_exchange_format() {
        let mut h = kettle();
        h.relationships[0][1] = 5; // not 0, 1, 3 or 9
        assert_eq!(h.relationship(0, 1), 0);
        assert_eq!(h.relationship(9, 9), 0);
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("\"necessidades\"") && json.contains("\"--\""));
        assert_eq!(serde_json::from_str::<House>(&json).unwrap(), h);
        assert_eq!(
            serde_json::from_str::<House>("{}").unwrap(),
            House::default()
        );
        assert_eq!(compute(&House::default()), Outcome::default());
    }
}
