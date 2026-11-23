//! Action priority (AP) of the FMEA: high, medium or low for each of the 1000 combinations
//! of severity S, occurrence O and detection D (1 to 10, 10 the worst).
//!
//! The built-in table is UTSim's own (owner decision T1, `docs/plano_lean_seis_sigma.md`,
//! section 7), written from the public logic of the method: severity weighs first, then
//! occurrence, then detection. It is a set of rules "S ≥ a and O ≥ b and D ≥ c", each one
//! closed upwards, so the priority never goes down when S, O or D goes up (tested on the
//! whole grid). A company that has the right to use another table imports it from CSV
//! ([`ApTable::from_csv`]) and it applies to the whole project.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Action priority, ordered `Low < Medium < High`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Ap {
    #[default]
    #[serde(rename = "L")]
    Low,
    #[serde(rename = "M")]
    Medium,
    #[serde(rename = "H")]
    High,
}

impl Ap {
    pub const ALL: [Ap; 3] = [Ap::High, Ap::Medium, Ap::Low];

    pub fn letter(self) -> char {
        match self {
            Ap::Low => 'L',
            Ap::Medium => 'M',
            Ap::High => 'H',
        }
    }

    pub fn from_letter(c: char) -> Option<Ap> {
        match c.to_ascii_uppercase() {
            'L' => Some(Ap::Low),
            'M' => Some(Ap::Medium),
            'H' => Some(Ap::High),
            _ => None,
        }
    }
}

/// One rule of the built-in table: applies when `S ≥ s`, `O ≥ o` and `D ≥ d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub s: u8,
    pub o: u8,
    pub d: u8,
    pub ap: Ap,
}

const fn rule(s: u8, o: u8, d: u8, ap: Ap) -> Rule {
    Rule { s, o, d, ap }
}

/// The rules of the built-in table, documented in the manual (chapter "UTSim LSS",
/// FMEA). The priority of a combination is the highest among the rules it meets; none: low.
///
/// - severity 9–10 (safety, legal): high with occurrence 4 or more, or occurrence 2–3 with
///   detection 5 or worse; medium with occurrence 2–3 and good detection, or occurrence 1
///   with detection 7 or worse;
/// - severity 7–8: high with occurrence 6 or more, or occurrence 4–5 with detection 7 or
///   worse; medium with occurrence 4–5, or occurrence 2–3 with detection 5 or worse;
/// - severity 4–6: high only with occurrence 8 or more and detection 5 or worse; medium with
///   occurrence 6 or more, or occurrence 4–5 with detection 7 or worse;
/// - severity 2–3: medium only with occurrence 8 or more and detection 7 or worse;
/// - severity 1 (no effect): always low.
pub const BUILTIN_RULES: [Rule; 12] = [
    rule(9, 4, 1, Ap::High),
    rule(9, 2, 5, Ap::High),
    rule(7, 6, 1, Ap::High),
    rule(7, 4, 7, Ap::High),
    rule(4, 8, 5, Ap::High),
    rule(9, 2, 1, Ap::Medium),
    rule(9, 1, 7, Ap::Medium),
    rule(7, 4, 1, Ap::Medium),
    rule(7, 2, 5, Ap::Medium),
    rule(4, 6, 1, Ap::Medium),
    rule(4, 4, 7, Ap::Medium),
    rule(2, 8, 7, Ap::Medium),
];

/// The priority of the built-in rules (S, O, D from 1 to 10).
pub fn builtin(s: u8, o: u8, d: u8) -> Ap {
    BUILTIN_RULES
        .iter()
        .filter(|r| s >= r.s && o >= r.o && d >= r.d)
        .map(|r| r.ap)
        .max()
        .unwrap_or(Ap::Low)
}

fn index(s: u8, o: u8, d: u8) -> Option<usize> {
    let ok = |x: u8| (1..=10).contains(&x);
    (ok(s) && ok(o) && ok(d))
        .then(|| (usize::from(s) - 1) * 100 + (usize::from(o) - 1) * 10 + usize::from(d) - 1)
}

/// A full AP table: the 1000 cells.
#[derive(Clone, PartialEq, Eq)]
pub struct ApTable {
    cells: Vec<Ap>,
}

impl fmt::Debug for ApTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApTable({})", self.to_compact())
    }
}

impl Default for ApTable {
    fn default() -> Self {
        Self::builtin()
    }
}

/// A problem found in a CSV table.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CsvError {
    #[error("linha {line}: esperado S,O,D,AP")]
    Columns { line: usize },
    #[error("linha {line}: {what} deve ser um inteiro de 1 a 10 (veio \"{value}\")")]
    Rating {
        line: usize,
        what: &'static str,
        value: String,
    },
    #[error("linha {line}: AP deve ser H, M ou L (veio \"{value}\")")]
    Priority { line: usize, value: String },
    #[error("linha {line}: S={s}, O={o}, D={d} repetida (já veio na linha {first})")]
    Duplicate {
        line: usize,
        first: usize,
        s: u8,
        o: u8,
        d: u8,
    },
    #[error("faltam {count} combinações, a primeira S={s}, O={o}, D={d}")]
    Missing { count: usize, s: u8, o: u8, d: u8 },
}

impl ApTable {
    pub fn builtin() -> Self {
        let mut cells = Vec::with_capacity(1000);
        for s in 1..=10 {
            for o in 1..=10 {
                for d in 1..=10 {
                    cells.push(builtin(s, o, d));
                }
            }
        }
        Self { cells }
    }

    /// The priority of one combination; outside 1–10, `None`.
    pub fn get(&self, s: u8, o: u8, d: u8) -> Option<Ap> {
        index(s, o, d).map(|i| self.cells[i])
    }

    /// The 1000 letters in the order S, then O, then D (the project field `tabela_ap`).
    pub fn to_compact(&self) -> String {
        self.cells.iter().map(|a| a.letter()).collect()
    }

    pub fn from_compact(text: &str) -> Option<Self> {
        let cells: Option<Vec<Ap>> = text.chars().map(Ap::from_letter).collect();
        cells
            .filter(|c| c.len() == 1000)
            .map(|cells| Self { cells })
    }

    /// The table as CSV `S,O,D,AP` with a header, the format [`ApTable::from_csv`] reads.
    pub fn to_csv(&self) -> String {
        let mut out = String::from("S,O,D,AP\n");
        for s in 1..=10u8 {
            for o in 1..=10u8 {
                for d in 1..=10u8 {
                    let ap = self.get(s, o, d).expect("in range").letter();
                    out.push_str(&format!("{s},{o},{d},{ap}\n"));
                }
            }
        }
        out
    }

    /// Reads a company table: lines `S,O,D,AP` (comma, semicolon or tab), an optional header
    /// line, empty lines skipped. Every one of the 1000 combinations must appear exactly
    /// once, with AP `H`, `M` or `L`. Returns every problem found (at most 20 listed).
    pub fn from_csv(text: &str) -> Result<Self, Vec<CsvError>> {
        let mut cells: Vec<Option<(Ap, usize)>> = vec![None; 1000];
        let mut errors = Vec::new();
        let mut seen_line = false;
        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let raw = raw.trim().trim_start_matches('\u{feff}');
            if raw.is_empty() {
                continue;
            }
            let cols: Vec<&str> = raw.split([',', ';', '\t']).map(str::trim).collect();
            if !seen_line {
                seen_line = true;
                if cols.first().is_some_and(|c| c.parse::<u8>().is_err()) {
                    // Header.
                    continue;
                }
            }
            if cols.len() != 4 {
                errors.push(CsvError::Columns { line });
                continue;
            }
            let mut r = [0u8; 3];
            let mut bad = false;
            for (k, what) in ["S", "O", "D"].into_iter().enumerate() {
                match cols[k].parse::<u8>() {
                    Ok(v) if (1..=10).contains(&v) => r[k] = v,
                    _ => {
                        errors.push(CsvError::Rating {
                            line,
                            what,
                            value: cols[k].to_string(),
                        });
                        bad = true;
                    }
                }
            }
            let ap = match cols[3].chars().collect::<Vec<_>>().as_slice() {
                [c] => Ap::from_letter(*c),
                _ => None,
            };
            let Some(ap) = ap else {
                errors.push(CsvError::Priority {
                    line,
                    value: cols[3].to_string(),
                });
                continue;
            };
            if bad {
                continue;
            }
            let idx = index(r[0], r[1], r[2]).expect("checked");
            match cells[idx] {
                Some((_, first)) => errors.push(CsvError::Duplicate {
                    line,
                    first,
                    s: r[0],
                    o: r[1],
                    d: r[2],
                }),
                None => cells[idx] = Some((ap, line)),
            }
        }
        let missing: Vec<usize> = (0..1000).filter(|i| cells[*i].is_none()).collect();
        if let Some(first) = missing.first() {
            errors.push(CsvError::Missing {
                count: missing.len(),
                s: (first / 100 + 1) as u8,
                o: (first / 10 % 10 + 1) as u8,
                d: (first % 10 + 1) as u8,
            });
        }
        if !errors.is_empty() {
            errors.truncate(20);
            return Err(errors);
        }
        Ok(Self {
            cells: cells.into_iter().map(|c| c.expect("complete").0).collect(),
        })
    }

    /// Combinations where raising S, O or D by one lowers the priority (empty for the
    /// built-in table; a company table may have some, and the screen warns).
    pub fn non_monotone(&self) -> Vec<(u8, u8, u8)> {
        let mut out = Vec::new();
        for s in 1..=10u8 {
            for o in 1..=10u8 {
                for d in 1..=10u8 {
                    let here = self.get(s, o, d).expect("in range");
                    let up = [(s + 1, o, d), (s, o + 1, d), (s, o, d + 1)];
                    if up
                        .iter()
                        .any(|(a, b, c)| self.get(*a, *b, *c).is_some_and(|n| n < here))
                    {
                        out.push((s, o, d));
                    }
                }
            }
        }
        out
    }

    /// How many cells of each priority (high, medium, low).
    pub fn counts(&self) -> [usize; 3] {
        let mut c = [0; 3];
        for a in &self.cells {
            match a {
                Ap::High => c[0] += 1,
                Ap::Medium => c[1] += 1,
                Ap::Low => c[2] += 1,
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_covers_the_1000_cells_and_never_decreases() {
        let t = ApTable::builtin();
        assert_eq!(t.to_compact().len(), 1000);
        assert!(t.non_monotone().is_empty());
        // Every cell, against every cell that dominates it (not only the neighbours).
        let all: Vec<(u8, u8, u8)> = (1..=10u8)
            .flat_map(|s| (1..=10u8).flat_map(move |o| (1..=10u8).map(move |d| (s, o, d))))
            .collect();
        assert_eq!(all.len(), 1000);
        for &(s, o, d) in &all {
            let here = t.get(s, o, d).unwrap();
            assert_eq!(here, builtin(s, o, d));
            for &(s2, o2, d2) in &all {
                if s2 >= s && o2 >= o && d2 >= d {
                    assert!(t.get(s2, o2, d2).unwrap() >= here, "({s},{o},{d})");
                }
            }
        }
        assert_eq!(t.get(0, 1, 1), None);
        assert_eq!(t.get(1, 1, 11), None);
    }

    #[test]
    fn builtin_rules_by_severity_band() {
        for o in 1..=10 {
            for d in 1..=10 {
                assert_eq!(builtin(1, o, d), Ap::Low, "no effect: low");
                assert_ne!(builtin(3, o, d), Ap::High, "S 2-3 never high");
                let high6 = builtin(6, o, d) == Ap::High;
                assert_eq!(
                    high6,
                    o >= 8 && d >= 5,
                    "S 4-6 high only when frequent and hard to see"
                );
            }
        }
        // Severity weighs first: the same O and D give a priority at least as high with a
        // higher severity, and the worst severity with frequent failure is always high.
        for o in 4..=10 {
            for d in 1..=10 {
                assert_eq!(builtin(9, o, d), Ap::High);
                assert_eq!(builtin(10, o, d), Ap::High);
            }
        }
        assert_eq!(builtin(10, 1, 1), Ap::Low);
        assert_eq!(builtin(10, 1, 7), Ap::Medium);
        assert_eq!(builtin(9, 3, 4), Ap::Medium);
        assert_eq!(builtin(9, 2, 5), Ap::High);
        assert_eq!(builtin(8, 6, 1), Ap::High);
        assert_eq!(builtin(8, 5, 6), Ap::Medium);
        assert_eq!(builtin(7, 5, 7), Ap::High);
        assert_eq!(builtin(7, 3, 4), Ap::Low);
        assert_eq!(builtin(7, 3, 5), Ap::Medium);
        assert_eq!(builtin(5, 8, 5), Ap::High);
        assert_eq!(builtin(5, 7, 10), Ap::Medium);
        assert_eq!(builtin(5, 5, 7), Ap::Medium);
        assert_eq!(builtin(5, 5, 6), Ap::Low);
        assert_eq!(builtin(3, 8, 7), Ap::Medium);
        assert_eq!(builtin(3, 10, 6), Ap::Low);
        // Golden counts of the whole table (documented in the manual).
        assert_eq!(ApTable::builtin().counts(), GOLDEN_COUNTS);
    }

    /// High, medium and low cells of the built-in table.
    const GOLDEN_COUNTS: [usize; 3] = [334, 216, 450];

    #[test]
    fn compact_and_csv_round_trip() {
        let t = ApTable::builtin();
        assert_eq!(ApTable::from_compact(&t.to_compact()), Some(t.clone()));
        assert_eq!(ApTable::from_compact("HML"), None);
        assert_eq!(ApTable::from_csv(&t.to_csv()), Ok(t.clone()));
        // Semicolons, no header, lower case, blank lines, BOM.
        let alt: String = t
            .to_csv()
            .lines()
            .skip(1)
            .map(|l| l.replace(',', ";").to_lowercase() + "\n\n")
            .collect();
        assert_eq!(ApTable::from_csv(&format!("\u{feff}{alt}")), Ok(t));
    }

    #[test]
    fn csv_validation_reports_every_problem() {
        let t = ApTable::builtin();
        let mut lines: Vec<String> = t.to_csv().lines().map(String::from).collect();
        lines[1] = "1,1,1,X".into(); // bad AP
        lines[2] = "1,1,11,L".into(); // bad D
        lines[3] = "1,1,1".into(); // columns
        lines[4] = "1,1,5,H".into(); // duplicate of line 6? no: (1,1,4) is replaced
        let errs = ApTable::from_csv(&lines.join("\n")).unwrap_err();
        assert!(errs.contains(&CsvError::Priority {
            line: 2,
            value: "X".into()
        }));
        assert!(errs.contains(&CsvError::Rating {
            line: 3,
            what: "D",
            value: "11".into()
        }));
        assert!(errs.contains(&CsvError::Columns { line: 4 }));
        assert!(errs.iter().any(|e| matches!(
            e,
            CsvError::Duplicate {
                s: 1,
                o: 1,
                d: 5,
                ..
            }
        )));
        assert!(errs.iter().any(|e| matches!(
            e,
            CsvError::Missing {
                count: 4,
                s: 1,
                o: 1,
                d: 1
            }
        )));
        assert!(!errs[0].to_string().is_empty());
        // A complete company table with a non monotone cell is accepted and flagged.
        let mut c = t.to_compact().into_bytes();
        c[999] = b'L';
        let company = ApTable::from_compact(std::str::from_utf8(&c).unwrap()).unwrap();
        assert_eq!(ApTable::from_csv(&company.to_csv()), Ok(company.clone()));
        assert!(!company.non_monotone().is_empty());
        assert!(ApTable::from_csv("").is_err());
    }
}
