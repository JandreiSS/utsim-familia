//! Reading the tables of a quality study from CSV text (no file access here: the caller
//! reads the file). The separator is `;` when the first line has one (then `,` is the
//! decimal mark), else tab, else `,`. A first line with any non-numeric cell is a header.
//!
//! Layouts:
//! - **measures** ([`subgroups`]): one row per subgroup, every numeric cell a value; a
//!   first column named `subgrupo`, `amostra`, `lote`, `data`, `hora` or `id` is a label;
//!   with `size`, a single column is cut into consecutive subgroups of that size;
//! - **attributes** ([`attributes`]): `contagem[,tamanho]` per row (the size is 1 when
//!   missing, as in a c chart);
//! - **gauge study** ([`gage`]): `peça, operador, valor` (by header name, else by
//!   position);
//! - **attribute study** ([`ratings`]): `item, avaliador, resposta[, padrão]`;
//! - **bias and linearity** ([`reference_pairs`]): `referência, valor`.

use crate::attribute::Rating;
use crate::msa::Measurement;

/// A parsed table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub header: Option<Vec<String>>,
    pub rows: Vec<Vec<String>>,
    /// The decimal comma is accepted (separator `;`).
    pub decimal_comma: bool,
}

/// Parses a number with `.` or, when `decimal_comma`, `,` as decimal mark.
pub fn number(cell: &str, decimal_comma: bool) -> Option<f64> {
    let t = cell.trim().trim_matches('"');
    if t.is_empty() {
        return None;
    }
    let t = if decimal_comma {
        t.replace('.', "").replace(',', ".")
    } else {
        t.to_string()
    };
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Splits the text into a table.
pub fn parse(text: &str) -> Table {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .collect();
    let Some(first) = lines.first() else {
        return Table::default();
    };
    let sep = if first.contains(';') {
        ';'
    } else if first.contains('\t') {
        '\t'
    } else {
        ','
    };
    let decimal_comma = sep == ';';
    let split = |l: &str| -> Vec<String> {
        l.split(sep)
            .map(|c| c.trim().trim_matches('"').to_string())
            .collect()
    };
    let first_cells = split(first);
    let header = first_cells
        .iter()
        .any(|c| !c.is_empty() && number(c, decimal_comma).is_none())
        .then_some(first_cells);
    let start = usize::from(header.is_some());
    Table {
        header,
        rows: lines[start..].iter().map(|l| split(l)).collect(),
        decimal_comma,
    }
}

fn normalized(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace('ç', "c")
        .replace(['á', 'à', 'ã', 'â'], "a")
        .replace(['é', 'ê'], "e")
        .replace('í', "i")
        .replace(['ó', 'õ', 'ô'], "o")
        .replace('ú', "u")
}

impl Table {
    /// Index of the column whose header is one of `names` (accents and case ignored).
    pub fn column(&self, names: &[&str]) -> Option<usize> {
        let h = self.header.as_ref()?;
        h.iter()
            .position(|c| names.iter().any(|n| normalized(c) == normalized(n)))
    }

    fn label_first_column(&self) -> bool {
        self.column(&["subgrupo", "amostra", "lote", "data", "hora", "id", "grupo"]) == Some(0)
    }
}

/// Subgroups of measures. `size`: cut the values (row by row) into consecutive
/// subgroups of that size (the last incomplete one is dropped, with a note).
pub fn subgroups(
    table: &Table,
    size: Option<usize>,
) -> Result<(Vec<Vec<f64>>, Vec<String>), String> {
    let skip = usize::from(table.label_first_column());
    let mut notes = Vec::new();
    let mut groups = Vec::new();
    for (k, row) in table.rows.iter().enumerate() {
        let vals: Vec<f64> = row
            .iter()
            .skip(skip)
            .filter(|c| !c.trim().is_empty())
            .filter_map(|c| number(c, table.decimal_comma))
            .collect();
        let cells = row
            .iter()
            .skip(skip)
            .filter(|c| !c.trim().is_empty())
            .count();
        if vals.len() < cells {
            notes.push(format!(
                "linha {}: {} célula(s) não numérica(s) ignorada(s)",
                k + 1 + usize::from(table.header.is_some()),
                cells - vals.len()
            ));
        }
        if !vals.is_empty() {
            groups.push(vals);
        }
    }
    if groups.is_empty() {
        return Err("nenhum valor numérico no arquivo".into());
    }
    if let Some(n) = size {
        if n == 0 {
            return Err("--subgrupo deve ser ao menos 1".into());
        }
        let all: Vec<f64> = groups.into_iter().flatten().collect();
        let full = all.len() / n;
        if all.len() % n != 0 {
            notes.push(format!(
                "{} valor(es) no fim não completam um subgrupo de {n} e ficaram de fora",
                all.len() % n
            ));
        }
        groups = (0..full)
            .map(|i| all[i * n..(i + 1) * n].to_vec())
            .collect();
        if groups.is_empty() {
            return Err(format!("menos de {n} valores: nenhum subgrupo completo"));
        }
    }
    Ok((groups, notes))
}

/// Counts and sizes of an attribute chart.
pub fn attributes(table: &Table) -> Result<(Vec<f64>, Vec<f64>), String> {
    let skip = usize::from(table.label_first_column());
    let ci = table
        .column(&["contagem", "defeitos", "defeituosos", "nc", "count"])
        .unwrap_or(skip);
    let ni = table
        .column(&["tamanho", "n", "unidades", "inspecionados", "size"])
        .filter(|&i| i != ci)
        .or_else(|| {
            table
                .rows
                .first()
                .is_some_and(|r| r.len() > ci + 1)
                .then_some(ci + 1)
        });
    let (mut counts, mut sizes) = (Vec::new(), Vec::new());
    for (k, row) in table.rows.iter().enumerate() {
        let line = k + 1 + usize::from(table.header.is_some());
        let c = row
            .get(ci)
            .and_then(|c| number(c, table.decimal_comma))
            .ok_or_else(|| format!("linha {line}: contagem ausente ou não numérica"))?;
        let n = match ni {
            Some(i) => row
                .get(i)
                .and_then(|c| number(c, table.decimal_comma))
                .ok_or_else(|| format!("linha {line}: tamanho ausente ou não numérico"))?,
            None => 1.0,
        };
        counts.push(c);
        sizes.push(n);
    }
    if counts.is_empty() {
        return Err("nenhuma linha de dados".into());
    }
    Ok((counts, sizes))
}

fn three_columns(table: &Table, a: &[&str], b: &[&str], c: &[&str]) -> (usize, usize, usize) {
    match (table.column(a), table.column(b), table.column(c)) {
        (Some(x), Some(y), Some(z)) => (x, y, z),
        _ => (0, 1, 2),
    }
}

/// Measurements of a gauge study.
pub fn gage(table: &Table) -> Result<Vec<Measurement>, String> {
    let (pi, oi, vi) = three_columns(
        table,
        &["peca", "peça", "part", "item"],
        &["operador", "avaliador", "operator"],
        &["valor", "medida", "medicao", "medição", "value"],
    );
    table
        .rows
        .iter()
        .enumerate()
        .map(|(k, row)| {
            let line = k + 1 + usize::from(table.header.is_some());
            let get = |i: usize| {
                row.get(i)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .ok_or_else(|| format!("linha {line}: faltam colunas (peça, operador, valor)"))
            };
            Ok(Measurement {
                part: get(pi)?,
                operator: get(oi)?,
                value: number(&get(vi)?, table.decimal_comma)
                    .ok_or_else(|| format!("linha {line}: valor não numérico"))?,
            })
        })
        .collect()
}

/// Ratings of an attribute agreement study.
pub fn ratings(table: &Table) -> Result<Vec<Rating>, String> {
    let (ii, ai, ri) = three_columns(
        table,
        &["item", "peca", "peça"],
        &["avaliador", "operador", "appraiser"],
        &[
            "resposta",
            "avaliacao",
            "avaliação",
            "resultado",
            "response",
        ],
    );
    let si = table
        .column(&["padrao", "padrão", "referencia", "referência", "standard"])
        .or_else(|| {
            (table.header.is_none() && table.rows.first().is_some_and(|r| r.len() > 3)).then_some(3)
        });
    table
        .rows
        .iter()
        .enumerate()
        .map(|(k, row)| {
            let line = k + 1 + usize::from(table.header.is_some());
            let get = |i: usize| {
                row.get(i)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .ok_or_else(|| {
                        format!("linha {line}: faltam colunas (item, avaliador, resposta)")
                    })
            };
            Ok(Rating {
                item: get(ii)?,
                appraiser: get(ai)?,
                response: get(ri)?,
                standard: si.and_then(|i| row.get(i).filter(|s| !s.is_empty()).cloned()),
            })
        })
        .collect()
}

/// `(reference, measured)` pairs of a bias or linearity study.
pub fn reference_pairs(table: &Table) -> Result<Vec<(f64, f64)>, String> {
    let ri = table
        .column(&["referencia", "referência", "padrao", "padrão", "reference"])
        .unwrap_or(0);
    let vi = table
        .column(&["valor", "medida", "medicao", "medição", "value"])
        .unwrap_or(if ri == 0 { 1 } else { 0 });
    table
        .rows
        .iter()
        .enumerate()
        .map(|(k, row)| {
            let line = k + 1 + usize::from(table.header.is_some());
            let get = |i: usize| {
                row.get(i)
                    .and_then(|c| number(c, table.decimal_comma))
                    .ok_or_else(|| format!("linha {line}: referência ou valor não numérico"))
            };
            Ok((get(ri)?, get(vi)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_headers_and_decimal_comma() {
        let t = parse("amostra;x1;x2\n1;10,5;11\n2;9,8;10,2\n");
        assert!(t.decimal_comma);
        let (g, notes) = subgroups(&t, None).unwrap();
        assert_eq!(g, vec![vec![10.5, 11.0], vec![9.8, 10.2]]);
        assert!(notes.is_empty());
        let t = parse("1.5\n2.5\n3.5\n4.5\n5.5\n");
        assert!(t.header.is_none());
        let (g, notes) = subgroups(&t, Some(2)).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn attribute_columns() {
        let t = parse("dia,defeituosos,n\n1,3,100\n2,5,120\n");
        // The first column "dia" is not a known label: the named columns are used.
        let (c, n) = attributes(&t).unwrap();
        assert_eq!(c, vec![3.0, 5.0]);
        assert_eq!(n, vec![100.0, 120.0]);
        let (c, n) = attributes(&parse("7\n9\n")).unwrap();
        assert_eq!((c, n), (vec![7.0, 9.0], vec![1.0, 1.0]));
    }

    #[test]
    fn gage_and_ratings_by_header() {
        let t = parse("operador,peça,valor\nA,1,2.5\nB,1,2.6\n");
        let m = gage(&t).unwrap();
        assert_eq!(m[0].part, "1");
        assert_eq!(m[1].operator, "B");
        let r = ratings(&parse("item,avaliador,resposta,padrão\n1,Ana,ok,ok\n")).unwrap();
        assert_eq!(r[0].standard.as_deref(), Some("ok"));
        let p = reference_pairs(&parse("referência;valor\n2;2,01\n")).unwrap();
        assert_eq!(p, vec![(2.0, 2.01)]);
    }
}
