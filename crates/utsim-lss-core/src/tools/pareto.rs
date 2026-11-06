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

pub use utsim_ferramentas::pareto::{OTHERS, Pareto, Row, VITAL_PCT, pareto, tally};

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
