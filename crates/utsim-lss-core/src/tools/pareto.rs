//! Pareto analysis (`docs/plano_lean_seis_sigma.md`, 2.1): categories sorted by value,
//! percentage and cumulative percentage, the vital few up to 80 %, an "outros" category for
//! the small ones, and the weighting by unit cost.
//!
//! The deliverable holds categories (items `categoria` with `nome`, `contagem` and
//! optionally `custo`) or a table of occurrences (field `tabela`, one occurrence per line),
//! which [`tally`] counts.

use super::items;
use crate::id::Id;
use crate::state::State;

pub const ITEM: &str = "categoria";

pub mod fields {
    pub const NAME: &str = "nome";
    pub const COUNT: &str = "contagem";
    /// Unit cost, for the Pareto weighted by cost.
    pub const COST: &str = "custo";
    /// Deliverable: `contagens` (default) or `tabela`.
    pub const MODE: &str = "modo";
    /// Deliverable: the occurrences, one per line.
    pub const TABLE: &str = "tabela";
    /// Deliverable: categories below this percentage go to "outros" (0: none).
    pub const OTHERS_BELOW: &str = "limiar_outros";
    /// Deliverable: weight the counts by the unit cost.
    pub const BY_COST: &str = "ponderar_custo";
}

/// Name of the grouped small categories.
pub const OTHERS: &str = "Outros";

/// The cumulative percentage that marks the vital few.
pub const VITAL_PCT: f64 = 80.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub value: f64,
    pub pct: f64,
    pub cum_pct: f64,
    /// Among the vital few: the categories needed to reach 80 % (the one that crosses it
    /// included).
    pub vital: bool,
    pub others: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pareto {
    pub rows: Vec<Row>,
    pub total: f64,
}

impl Pareto {
    pub fn vital(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(|r| r.vital)
    }
}

/// The Pareto of `(category, value)`. Negative and non finite values count as zero;
/// categories with the same name are added. Sorted by value (descending, then name); the
/// categories under `others_below_pct` of the total go to one "Outros" row at the end.
pub fn pareto(input: &[(String, f64)], others_below_pct: f64) -> Pareto {
    let mut merged: Vec<(String, f64)> = Vec::new();
    for (name, v) in input {
        let v = if v.is_finite() && *v > 0.0 { *v } else { 0.0 };
        let name = name.trim();
        match merged
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            Some(m) => m.1 += v,
            None => merged.push((name.to_string(), v)),
        }
    }
    let total: f64 = merged.iter().map(|(_, v)| v).sum();
    if total <= 0.0 {
        return Pareto {
            rows: merged
                .into_iter()
                .map(|(name, value)| Row {
                    name,
                    value,
                    pct: 0.0,
                    cum_pct: 0.0,
                    vital: false,
                    others: false,
                })
                .collect(),
            total: 0.0,
        };
    }
    merged.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut main = Vec::new();
    let mut small = Vec::new();
    for (name, v) in merged {
        if others_below_pct > 0.0 && v / total * 100.0 < others_below_pct {
            small.push((name, v));
        } else {
            main.push((name, v, false));
        }
    }
    if small.len() == 1 {
        // A single small category stays itself.
        let (name, v) = small.pop().expect("one");
        main.push((name, v, false));
    } else if !small.is_empty() {
        main.push((OTHERS.to_string(), small.iter().map(|(_, v)| v).sum(), true));
    }
    let mut cum = 0.0;
    let mut reached = false;
    let n = main.len();
    let rows = main
        .into_iter()
        .enumerate()
        .map(|(k, (name, value, others))| {
            let before = cum;
            cum += value;
            let pct = value / total * 100.0;
            // The last row closes at exactly 100 % (no rounding residue).
            let cum_pct = if k + 1 == n || cum >= total {
                100.0
            } else {
                cum / total * 100.0
            };
            let vital = !reached && !others && before / total * 100.0 < VITAL_PCT;
            if cum_pct >= VITAL_PCT {
                reached = true;
            }
            Row {
                name,
                value,
                pct,
                cum_pct,
                vital,
                others,
            }
        })
        .collect();
    Pareto { rows, total }
}

/// Counts a table of occurrences: one category per line (empty lines skipped). A line may
/// end in `;n` or a tab and `n` to give a quantity. Same name in any case is one category;
/// the first spelling is kept, in order of first appearance.
pub fn tally(text: &str) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, qty) = match line.rsplit_once([';', '\t']) {
            Some((n, q)) => match super::parse_number(q) {
                Some(q) => (n.trim(), q),
                None => (line, 1.0),
            },
            None => (line, 1.0),
        };
        match out.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(m) => m.1 += qty,
            None => out.push((name.to_string(), qty)),
        }
    }
    out
}

/// What a Pareto deliverable holds, ready for [`pareto`].
pub fn input(state: &State, deliverable: Id) -> Vec<(String, f64)> {
    let Some(d) = state.entity(deliverable) else {
        return Vec::new();
    };
    let by_cost = d.boolean(fields::BY_COST);
    if d.text(fields::MODE) == "tabela" {
        return tally(&d.text(fields::TABLE));
    }
    items(state, deliverable, ITEM)
        .iter()
        .map(|i| {
            let count = i.entity.number(fields::COUNT).unwrap_or(0.0);
            let cost = i.entity.number(fields::COST).unwrap_or(1.0);
            let v = if by_cost { count * cost } else { count };
            (i.text(fields::NAME), v)
        })
        .collect()
}

/// The Pareto of a deliverable with its settings.
pub fn of_deliverable(state: &State, deliverable: Id) -> Pareto {
    let below = state
        .entity(deliverable)
        .and_then(|d| d.number(fields::OTHERS_BELOW))
        .unwrap_or(0.0);
    pareto(&input(state, deliverable), below)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    fn v(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
        pairs.iter().map(|(n, x)| (n.to_string(), *x)).collect()
    }

    #[test]
    fn known_cumulative() {
        // 50, 30, 10, 6, 4 of 100.
        let p = pareto(
            &v(&[
                ("e", 4.0),
                ("b", 30.0),
                ("a", 50.0),
                ("d", 6.0),
                ("c", 10.0),
            ]),
            0.0,
        );
        let names: Vec<&str> = p.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c", "d", "e"]);
        let cum: Vec<f64> = p.rows.iter().map(|r| r.cum_pct).collect();
        assert_eq!(cum, [50.0, 80.0, 90.0, 96.0, 100.0]);
        let vital: Vec<&str> = p.vital().map(|r| r.name.as_str()).collect();
        assert_eq!(vital, ["a", "b"], "80 % reached exactly by b");
        assert_eq!(p.total, 100.0);
    }

    #[test]
    fn crossing_row_is_vital_and_others_group() {
        let p = pareto(
            &v(&[
                ("a", 40.0),
                ("b", 30.0),
                ("c", 20.0),
                ("d", 5.0),
                ("e", 3.0),
                ("f", 2.0),
            ]),
            6.0,
        );
        let names: Vec<&str> = p.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c", OTHERS]);
        assert_eq!(p.rows[3].value, 10.0);
        assert!(p.rows[3].others);
        let vital: Vec<&str> = p.vital().map(|r| r.name.as_str()).collect();
        assert_eq!(vital, ["a", "b", "c"], "c crosses 80 %");
        // A lone small category keeps its name.
        let p = pareto(&v(&[("a", 95.0), ("z", 5.0)]), 6.0);
        assert_eq!(p.rows[1].name, "z");
        assert!(!p.rows[1].others);
    }

    #[test]
    fn cumulative_always_ends_at_100() {
        let mut seed = 7u64;
        for n in 1..40 {
            let data: Vec<(String, f64)> = (0..n)
                .map(|i| {
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    (format!("c{i}"), ((seed >> 33) % 1000) as f64 / 7.0 + 0.1)
                })
                .collect();
            for below in [0.0, 3.0] {
                let p = pareto(&data, below);
                assert_eq!(p.rows.last().unwrap().cum_pct, 100.0);
                assert!(p.rows.windows(2).all(|w| w[0].cum_pct <= w[1].cum_pct));
                let sum: f64 = p.rows.iter().map(|r| r.pct).sum();
                assert!((sum - 100.0).abs() < 1e-9);
                assert!(p.vital().count() >= 1);
            }
        }
    }

    #[test]
    fn degenerate_inputs() {
        assert!(pareto(&[], 0.0).rows.is_empty());
        let p = pareto(&v(&[("a", 0.0), ("b", -3.0), ("A", f64::NAN)]), 0.0);
        assert_eq!(p.total, 0.0);
        assert_eq!(p.rows.len(), 2, "same name in any case merges");
    }

    #[test]
    fn tally_counts_lines() {
        let t = tally("Risco\nmancha\n\nRISCO\nbolha;3\nmancha\t2\nsem número;x\n");
        assert_eq!(
            t,
            v(&[
                ("Risco", 2.0),
                ("mancha", 3.0),
                ("bolha", 3.0),
                ("sem número;x", 1.0)
            ])
        );
    }

    #[test]
    fn deliverable_counts_table_and_cost() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Pareto);
        f.item(
            d,
            ITEM,
            None,
            vec![
                (fields::NAME, "a".into()),
                (fields::COUNT, 10.into()),
                (fields::COST, 1.into()),
            ],
        );
        f.item(
            d,
            ITEM,
            None,
            vec![
                (fields::NAME, "b".into()),
                (fields::COUNT, 2.into()),
                (fields::COST, 20.into()),
            ],
        );
        assert_eq!(of_deliverable(&f.state, d).rows[0].name, "a");
        f.set(d, fields::BY_COST, true);
        assert_eq!(of_deliverable(&f.state, d).rows[0].name, "b", "cost weighs");
        f.set(d, fields::MODE, "tabela");
        f.set(d, fields::TABLE, "x\ny\ny");
        let p = of_deliverable(&f.state, d);
        assert_eq!((p.rows[0].name.as_str(), p.rows[0].value), ("y", 2.0));
    }
}
