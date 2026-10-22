//! Reading samples from CSV/TXT files (Brazilian formats included).
//!
//! Separator (`;`, tab or `,`), header row and decimal comma are auto-detected.
//! Errors are written in Portuguese and carry the line number of the offending cell.

use std::path::Path;

/// How to pick the sample out of the file.
#[derive(Clone, Debug, Default)]
pub struct DataOptions {
    /// Column name (header) or 1-based index. `None` = first numeric column.
    pub column: Option<String>,
    /// The column holds increasing instants; use the differences as the sample.
    pub timestamps: bool,
}

/// The sample plus what was learnt while reading it.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub values: Vec<f64>,
    /// Column label (header name or "coluna N").
    pub column: String,
    /// Number of empty cells that were skipped.
    pub skipped_empty: usize,
    pub warnings: Vec<String>,
}

/// Reads and parses a file.
pub fn load(path: &Path, options: &DataOptions) -> Result<Loaded, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("não foi possível ler '{}': {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    parse(&text, options)
}

fn detect_separator(line: &str, allow_comma: bool) -> Option<char> {
    [';', '\t', ',']
        .into_iter()
        .filter(|c| allow_comma || *c != ',')
        .find(|c| line.contains(*c))
}

fn parse_number(cell: &str, sep: Option<char>) -> Option<f64> {
    let t = cell.trim().trim_matches('"').trim();
    if t.is_empty() {
        return None;
    }
    let normalized = if sep == Some(',') {
        t.to_string()
    } else {
        t.replace(',', ".")
    };
    normalized.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn split(line: &str, sep: Option<char>) -> Vec<&str> {
    match sep {
        Some(c) => line.split(c).collect(),
        None => vec![line],
    }
}

/// Parses the text of a data file.
pub fn parse(text: &str, options: &DataOptions) -> Result<Loaded, String> {
    // (1-based line number, content) of every non-blank, non-comment line.
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim_start_matches('\u{feff}').trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .collect();
    if lines.is_empty() {
        return Err("o arquivo não tem dados".into());
    }

    // A one-word header followed by "1,5" lines: the comma is a decimal mark, so it
    // only counts as separator when the first line itself contains one.
    let sep = detect_separator(lines[0].1, true)
        .or_else(|| lines.get(1).and_then(|(_, l)| detect_separator(l, false)));

    let first_cells = split(lines[0].1, sep);
    let has_header = lines.len() > 1
        && first_cells
            .iter()
            .all(|c| !c.trim().is_empty() && parse_number(c, sep).is_none());
    let (header, rows): (Option<Vec<String>>, &[(usize, &str)]) = if has_header {
        (
            Some(
                first_cells
                    .iter()
                    .map(|c| c.trim().trim_matches('"').trim().to_string())
                    .collect(),
            ),
            &lines[1..],
        )
    } else {
        (None, &lines[..])
    };
    let first_row = split(rows[0].1, sep);
    let n_cols = first_row.len().max(first_cells.len());

    let by_index = |s: &str| -> Option<usize> {
        s.trim()
            .parse::<usize>()
            .ok()
            .filter(|&i| i >= 1 && i <= n_cols)
            .map(|i| i - 1)
    };
    let col = match options.column.as_deref() {
        Some(c) => {
            let named = header
                .as_ref()
                .and_then(|h| h.iter().position(|n| n.eq_ignore_ascii_case(c.trim())));
            named.or_else(|| by_index(c)).ok_or_else(|| {
                let avail = match &header {
                    Some(h) => format!("colunas disponíveis: {}", h.join(", ")),
                    None => format!("o arquivo tem {n_cols} coluna(s), sem cabeçalho"),
                };
                format!("coluna '{c}' não encontrada ({avail})")
            })?
        }
        None => (0..n_cols)
            .find(|&j| {
                first_row
                    .get(j)
                    .is_some_and(|c| parse_number(c, sep).is_some())
            })
            .ok_or_else(|| format!("linha {}: nenhuma coluna numérica encontrada", rows[0].0))?,
    };
    let label = header
        .as_ref()
        .and_then(|h| h.get(col).cloned())
        .unwrap_or_else(|| format!("coluna {}", col + 1));

    let mut values: Vec<(usize, f64)> = Vec::new();
    let mut skipped = 0usize;
    for &(line_no, line) in rows {
        let cells = split(line, sep);
        match cells.get(col) {
            Some(cell) if !cell.trim().is_empty() => match parse_number(cell, sep) {
                Some(v) => values.push((line_no, v)),
                None => {
                    return Err(format!(
                        "linha {line_no}: '{}' não é um número (coluna '{label}')",
                        cell.trim()
                    ));
                }
            },
            _ => skipped += 1,
        }
    }

    let mut warnings = Vec::new();
    if skipped > 0 {
        warnings.push(format!("{skipped} célula(s) vazia(s) ignorada(s)"));
    }
    let values = if options.timestamps {
        if values.len() < 2 {
            return Err("com --timestamps são necessários pelo menos 2 instantes".into());
        }
        let mut diffs = Vec::with_capacity(values.len() - 1);
        for w in values.windows(2) {
            let d = w[1].1 - w[0].1;
            if d < 0.0 {
                return Err(format!(
                    "linha {}: instante {} é anterior ao da linha {} ({}); com --timestamps a coluna deve ser crescente",
                    w[1].0, w[1].1, w[0].0, w[0].1
                ));
            }
            diffs.push(d);
        }
        diffs
    } else {
        values.into_iter().map(|(_, v)| v).collect()
    };
    if values.is_empty() {
        return Err(format!("a coluna '{label}' não tem valores"));
    }
    Ok(Loaded {
        values,
        column: label,
        skipped_empty: skipped,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> DataOptions {
        DataOptions::default()
    }

    fn col(name: &str) -> DataOptions {
        DataOptions {
            column: Some(name.into()),
            ..opts()
        }
    }

    #[test]
    fn plain_single_column() {
        let l = parse("1\n2.5\n3\n", &opts()).unwrap();
        assert_eq!(l.values, vec![1.0, 2.5, 3.0]);
        assert_eq!(l.column, "coluna 1");
    }

    #[test]
    fn semicolon_with_decimal_comma_and_header() {
        let l = parse("tempo;peca\n1,5;a\n2,25;b\n", &opts()).unwrap();
        assert_eq!(l.values, vec![1.5, 2.25]);
        assert_eq!(l.column, "tempo");
    }

    #[test]
    fn comma_and_tab_separators() {
        let l = parse("a,b\n1,10\n2,20\n", &col("b")).unwrap();
        assert_eq!(l.values, vec![10.0, 20.0]);
        let l = parse("a\tb\n1.5\t3\n2\t4\n", &col("2")).unwrap();
        assert_eq!(l.values, vec![3.0, 4.0]);
    }

    #[test]
    fn single_column_with_decimal_comma() {
        let l = parse("tempo\n1,5\n2,5\n", &opts()).unwrap();
        assert_eq!(l.values, vec![1.5, 2.5]);
    }

    #[test]
    fn default_is_first_numeric_column() {
        let l = parse("id;valor\nA;1,5\nB;2,5\n", &opts()).unwrap();
        assert_eq!(l.values, vec![1.5, 2.5]);
        assert_eq!(l.column, "valor");
    }

    #[test]
    fn column_by_name_or_index_and_unknown() {
        let t = "x;y\n1;10\n2;20\n";
        assert_eq!(parse(t, &col("Y")).unwrap().values, vec![10.0, 20.0]);
        assert_eq!(parse(t, &col("1")).unwrap().values, vec![1.0, 2.0]);
        let e = parse(t, &col("z")).unwrap_err();
        assert!(e.contains("não encontrada") && e.contains("x, y"));
    }

    #[test]
    fn timestamps_become_intervals() {
        let o = DataOptions {
            timestamps: true,
            ..opts()
        };
        let l = parse("t\n0\n1,5\n4\n", &o).unwrap();
        assert_eq!(l.values, vec![1.5, 2.5]);
        let e = parse("t\n0\n5\n3\n", &o).unwrap_err();
        assert!(e.starts_with("linha 4"), "{e}");
    }

    #[test]
    fn error_reports_line_number() {
        let e = parse("v\n1\n2\nabc\n5\n", &opts()).unwrap_err();
        assert!(e.starts_with("linha 4") && e.contains("'abc'"), "{e}");
    }

    #[test]
    fn empty_cells_are_skipped_with_warning() {
        let l = parse("a;b\n1;2\n;3\n4;5\n", &opts()).unwrap();
        assert_eq!(l.values, vec![1.0, 4.0]);
        assert_eq!(l.skipped_empty, 1);
        assert_eq!(l.warnings.len(), 1);
    }

    #[test]
    fn comments_blank_lines_and_empty_file() {
        let l = parse("# dados\n\n1\n\n2\n", &opts()).unwrap();
        assert_eq!(l.values, vec![1.0, 2.0]);
        assert!(parse("  \n# nada\n", &opts()).is_err());
    }
}
