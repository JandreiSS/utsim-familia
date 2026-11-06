//! Pareto analysis: categories sorted by value, percentage and cumulative percentage, the
//! vital few up to 80 % and an "outros" category for the small ones. Pure: the Seis Sigma
//! app (`utsim_lss_core::tools::pareto`) and the quality app read their inputs and call it.

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
            Some((n, q)) => match crate::parse_number(q) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tally_counts_and_quantities() {
        let t = tally(
            "risco
rebarba;3
Risco

",
        );
        assert_eq!(
            t,
            vec![("risco".to_string(), 2.0), ("rebarba".to_string(), 3.0)]
        );
        let p = pareto(&t, 0.0);
        assert_eq!(p.rows[0].name, "rebarba");
    }
}
