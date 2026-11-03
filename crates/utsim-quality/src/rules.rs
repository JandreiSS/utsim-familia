//! Rules for special causes on a control chart: the Western Electric rules (1956) and the
//! eight rules of Nelson (1984), selectable one by one.
//!
//! The rules look at the standardised distance of each point to the centre line,
//! `z = (x − centre) / σ_ponto`, where `σ_ponto = (LSC − centre) / 3` (so they also work
//! with limits that vary from point to point). A rule *fires at* the point that completes
//! its pattern; zone rules (2 of 3, 4 of 5) only fire at a point that is itself in the zone.

use serde::{Deserialize, Serialize};

/// One rule. The numbers in [`Rule::code`] are Nelson's; Western Electric's rules are
/// Nelson 1, 5 and 6 plus "8 points on the same side" ([`Rule::Run`] with 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rule {
    /// One point beyond 3σ (Nelson 1, Western Electric 1).
    Beyond3,
    /// `len` points in a row on the same side of the centre (Nelson 2 with 9, Western
    /// Electric 4 with 8).
    Run(usize),
    /// `len` points in a row steadily increasing or decreasing (Nelson 3 with 6).
    Trend(usize),
    /// 14 points in a row alternating up and down (Nelson 4).
    Alternating,
    /// 2 of 3 points in a row beyond 2σ on the same side (Nelson 5, Western Electric 2).
    TwoOfThree,
    /// 4 of 5 points in a row beyond 1σ on the same side (Nelson 6, Western Electric 3).
    FourOfFive,
    /// 15 points in a row within 1σ, on both sides (Nelson 7: stratification).
    Within1(usize),
    /// 8 points in a row beyond 1σ, on both sides (Nelson 8: mixture).
    Beyond1BothSides(usize),
}

impl Rule {
    /// Short code: `N1`..`N8` for Nelson's rules, `WE4` for the run of 8.
    pub fn code(&self) -> String {
        match self {
            Rule::Beyond3 => "N1".into(),
            Rule::Run(9) => "N2".into(),
            Rule::Run(8) => "WE4".into(),
            Rule::Run(n) => format!("seq{n}"),
            Rule::Trend(6) => "N3".into(),
            Rule::Trend(n) => format!("tend{n}"),
            Rule::Alternating => "N4".into(),
            Rule::TwoOfThree => "N5".into(),
            Rule::FourOfFive => "N6".into(),
            Rule::Within1(15) => "N7".into(),
            Rule::Within1(n) => format!("dentro{n}"),
            Rule::Beyond1BothSides(8) => "N8".into(),
            Rule::Beyond1BothSides(n) => format!("fora{n}"),
        }
    }

    /// Description in Portuguese.
    pub fn describe(&self) -> String {
        match self {
            Rule::Beyond3 => "1 ponto além de 3σ".into(),
            Rule::Run(n) => format!("{n} pontos seguidos do mesmo lado da linha central"),
            Rule::Trend(n) => format!("{n} pontos seguidos subindo ou descendo"),
            Rule::Alternating => "14 pontos seguidos alternando para cima e para baixo".into(),
            Rule::TwoOfThree => "2 de 3 pontos seguidos além de 2σ do mesmo lado".into(),
            Rule::FourOfFive => "4 de 5 pontos seguidos além de 1σ do mesmo lado".into(),
            Rule::Within1(n) => format!("{n} pontos seguidos dentro de 1σ (estratificação)"),
            Rule::Beyond1BothSides(n) => {
                format!("{n} pontos seguidos fora de 1σ, dos dois lados (mistura)")
            }
        }
    }

    /// Number of points the rule looks at.
    pub fn window(&self) -> usize {
        match *self {
            Rule::Beyond3 => 1,
            Rule::Run(n) | Rule::Trend(n) | Rule::Within1(n) | Rule::Beyond1BothSides(n) => n,
            Rule::Alternating => 14,
            Rule::TwoOfThree => 3,
            Rule::FourOfFive => 5,
        }
    }

    /// Whether the rule fires at point `i` of the standardised series `z`.
    pub fn fires_at(&self, z: &[f64], i: usize) -> bool {
        let w = self.window();
        if i + 1 < w || i >= z.len() {
            return false;
        }
        let s = &z[i + 1 - w..=i];
        match *self {
            Rule::Beyond3 => z[i].abs() > 3.0,
            Rule::Run(_) => s.iter().all(|&v| v > 0.0) || s.iter().all(|&v| v < 0.0),
            Rule::Trend(_) => {
                s.windows(2).all(|p| p[1] > p[0]) || s.windows(2).all(|p| p[1] < p[0])
            }
            Rule::Alternating => {
                let d: Vec<f64> = s.windows(2).map(|p| p[1] - p[0]).collect();
                d.iter().all(|&v| v != 0.0) && d.windows(2).all(|p| p[0] * p[1] < 0.0)
            }
            Rule::TwoOfThree => zone(s, z[i], 2.0, 2),
            Rule::FourOfFive => zone(s, z[i], 1.0, 4),
            Rule::Within1(_) => {
                s.iter().all(|&v| v.abs() < 1.0)
                    && s.iter().any(|&v| v > 0.0)
                    && s.iter().any(|&v| v < 0.0)
            }
            Rule::Beyond1BothSides(_) => {
                s.iter().all(|&v| v.abs() > 1.0)
                    && s.iter().any(|&v| v > 0.0)
                    && s.iter().any(|&v| v < 0.0)
            }
        }
    }
}

/// At least `count` points of `s` beyond `limit` on the side of `last`, `last` among them.
fn zone(s: &[f64], last: f64, limit: f64, count: usize) -> bool {
    if last.abs() <= limit {
        return false;
    }
    let sign = last.signum();
    s.iter().filter(|&&v| v * sign > limit).count() >= count
}

/// A selection of rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSet(pub Vec<Rule>);

impl RuleSet {
    /// Only one point beyond 3σ.
    pub fn basic() -> Self {
        RuleSet(vec![Rule::Beyond3])
    }

    /// The four Western Electric rules.
    pub fn western_electric() -> Self {
        RuleSet(vec![
            Rule::Beyond3,
            Rule::TwoOfThree,
            Rule::FourOfFive,
            Rule::Run(8),
        ])
    }

    /// Nelson's eight rules.
    pub fn nelson() -> Self {
        RuleSet(vec![
            Rule::Beyond3,
            Rule::Run(9),
            Rule::Trend(6),
            Rule::Alternating,
            Rule::TwoOfThree,
            Rule::FourOfFive,
            Rule::Within1(15),
            Rule::Beyond1BothSides(8),
        ])
    }

    /// Parses `1`, `we`, `nelson` or a list such as `1,2,5` (Nelson numbers) or
    /// `we1,we4` (Western Electric numbers).
    pub fn parse(text: &str) -> Result<Self, String> {
        let t = text.trim().to_lowercase();
        match t.as_str() {
            "we" | "western" | "western-electric" => return Ok(Self::western_electric()),
            "nelson" | "todas" => return Ok(Self::nelson()),
            "" | "basica" | "básica" => return Ok(Self::basic()),
            _ => {}
        }
        let mut rules = Vec::new();
        for part in t.split([',', ' ']).filter(|s| !s.is_empty()) {
            let rule = if let Some(k) = part.strip_prefix("we") {
                match k {
                    "1" => Rule::Beyond3,
                    "2" => Rule::TwoOfThree,
                    "3" => Rule::FourOfFive,
                    "4" => Rule::Run(8),
                    _ => return Err(format!("regra '{part}' desconhecida: use we1 a we4")),
                }
            } else {
                let k = part.strip_prefix('n').unwrap_or(part);
                match k {
                    "1" => Rule::Beyond3,
                    "2" => Rule::Run(9),
                    "3" => Rule::Trend(6),
                    "4" => Rule::Alternating,
                    "5" => Rule::TwoOfThree,
                    "6" => Rule::FourOfFive,
                    "7" => Rule::Within1(15),
                    "8" => Rule::Beyond1BothSides(8),
                    _ => {
                        return Err(format!(
                            "regra '{part}' desconhecida: use 1 a 8 (Nelson), we1 a we4, \
                             'we' ou 'nelson'"
                        ));
                    }
                }
            };
            if !rules.contains(&rule) {
                rules.push(rule);
            }
        }
        if rules.is_empty() {
            return Err("nenhuma regra escolhida".into());
        }
        Ok(RuleSet(rules))
    }

    /// Whether any rule fires at point `i`.
    pub fn any_fires_at(&self, z: &[f64], i: usize) -> bool {
        self.0.iter().any(|r| r.fires_at(z, i))
    }

    /// Every (rule, point) that fires on the series. `None` entries (excluded points,
    /// the first moving range) are skipped: the rules run on the remaining points, and
    /// the indices refer to the original series.
    #[allow(clippy::needless_range_loop)] // `k` indexes the values and their positions
    pub fn evaluate(&self, z: &[Option<f64>]) -> Vec<Violation> {
        let (idx, vals): (Vec<usize>, Vec<f64>) = z
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|v| (i, v)))
            .unzip();
        let mut out = Vec::new();
        for k in 0..vals.len() {
            for rule in &self.0 {
                if rule.fires_at(&vals, k) {
                    out.push(Violation {
                        rule: *rule,
                        index: idx[k],
                    });
                }
            }
        }
        out
    }
}

/// A rule that fired at a point (0-based index of the subgroup).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub rule: Rule,
    pub index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fires(rule: Rule, z: &[f64]) -> Vec<usize> {
        (0..z.len()).filter(|&i| rule.fires_at(z, i)).collect()
    }

    #[test]
    fn each_planted_pattern_is_found() {
        // Synthetic series with one pattern each (indices of the completing point).
        let mut z = vec![0.5, -0.5, 0.2, 3.5, 0.1];
        assert_eq!(fires(Rule::Beyond3, &z), vec![3]);
        z = vec![-0.1, 0.3, 0.2, 0.4, 0.1, 0.6, 0.9, 0.2, 0.3, 0.5];
        assert_eq!(fires(Rule::Run(9), &z), vec![9]);
        assert_eq!(fires(Rule::Run(8), &z), vec![8, 9]);
        z = vec![0.0, -1.0, -0.5, 0.0, 0.2, 0.9, 1.4, 1.3];
        assert_eq!(fires(Rule::Trend(6), &z), vec![6]);
        z = (0..14)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        assert_eq!(fires(Rule::Alternating, &z), vec![13]);
        z = vec![0.0, 2.5, 1.0, 2.2, 0.0];
        assert_eq!(fires(Rule::TwoOfThree, &z), vec![3]);
        z = vec![-2.5, 0.0, -2.1];
        assert_eq!(fires(Rule::TwoOfThree, &z), vec![2]);
        // Opposite sides do not count.
        assert!(fires(Rule::TwoOfThree, &[2.5, 0.0, -2.1]).is_empty());
        z = vec![1.5, 1.2, 0.5, 1.1, 1.8];
        assert_eq!(fires(Rule::FourOfFive, &z), vec![4]);
        z = (0..15)
            .map(|i| if i % 3 == 0 { -0.4 } else { 0.3 })
            .collect();
        assert_eq!(fires(Rule::Within1(15), &z), vec![14]);
        z = vec![1.5, -1.2, 1.1, -1.8, 2.0, -1.4, 1.3, -1.6];
        assert_eq!(fires(Rule::Beyond1BothSides(8), &z), vec![7]);
        // All on one side is not a mixture.
        assert!(fires(Rule::Beyond1BothSides(8), &[1.5; 8]).is_empty());
    }

    #[test]
    fn parse_selections() {
        assert_eq!(RuleSet::parse("we").unwrap(), RuleSet::western_electric());
        assert_eq!(RuleSet::parse("Nelson").unwrap().0.len(), 8);
        assert_eq!(
            RuleSet::parse("1,5").unwrap().0,
            vec![Rule::Beyond3, Rule::TwoOfThree]
        );
        assert_eq!(RuleSet::parse("we4").unwrap().0, vec![Rule::Run(8)]);
        assert!(RuleSet::parse("9").is_err());
    }

    #[test]
    fn evaluation_skips_missing_points_and_keeps_indices() {
        let z = [None, Some(0.0), Some(4.0), None, Some(-3.5)];
        let v = RuleSet::basic().evaluate(&z);
        assert_eq!(v.iter().map(|v| v.index).collect::<Vec<_>>(), vec![2, 4]);
    }
}
