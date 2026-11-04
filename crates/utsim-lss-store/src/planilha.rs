//! Spreadsheets (`docs/plano_lean_seis_sigma.md`, 4.6): one sheet per table of a report
//! (actions, risks, FMEA, control plan, savings, data), numbers as numbers, written with
//! `rust_xlsxwriter` (the same pinned version as the dashboard's export).

use rust_xlsxwriter::{Format, Workbook};

use crate::{Error, Result};

/// A sheet name Excel accepts: at most 31 characters, none of `[]:*?/\`, unique.
pub fn sheet_name(name: &str, taken: &[String]) -> String {
    let clean: String = name
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '-' } else { c })
        .collect();
    let clean = clean.trim().trim_matches('\'');
    let base: String = if clean.is_empty() {
        "Tabela".into()
    } else {
        clean.chars().take(31).collect()
    };
    let mut cand = base.clone();
    let mut i = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&cand)) {
        let suffix = format!(" ({i})");
        let keep = 31usize.saturating_sub(suffix.chars().count());
        cand = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
        i += 1;
    }
    cand
}

/// A number written in Portuguese or plain (`1.234,5`, `12,5`, `0.25`), else `None`.
fn number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() || t.len() > 40 {
        return None;
    }
    let t = match (t.rfind(','), t.rfind('.')) {
        (Some(c), Some(p)) if c > p => t.replace('.', "").replace(',', "."),
        (Some(_), None) => t.replace(',', "."),
        _ => t.replace(',', ""),
    };
    // Leading zeros (codes like 007) stay text.
    if t.len() > 1 && t.starts_with('0') && !t.starts_with("0.") {
        return None;
    }
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

/// The workbook of `tables` (`(name, header, rows)`) as `.xlsx` bytes.
pub fn xlsx(tables: &[utsim_lss_core::relatorios::TableRef<'_>]) -> Result<Vec<u8>> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let mut names: Vec<String> = Vec::new();
    for (name, header, rows) in tables {
        let n = sheet_name(name, &names);
        names.push(n.clone());
        let ws = wb.add_worksheet();
        ws.set_name(&n).map_err(|e| Error::Invalid(e.to_string()))?;
        for (c, h) in header.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &bold)
                .map_err(|e| Error::Invalid(e.to_string()))?;
        }
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                let (rr, cc) = (r as u32 + 1, c as u16);
                let res = match number(v) {
                    Some(x) => ws.write_number(rr, cc, x).map(|_| ()),
                    None => ws.write_string(rr, cc, v).map(|_| ()),
                };
                res.map_err(|e| Error::Invalid(e.to_string()))?;
            }
        }
    }
    if names.is_empty() {
        wb.add_worksheet();
    }
    wb.save_to_buffer()
        .map_err(|e| Error::Invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_numbers_and_bytes() {
        let taken = vec!["Riscos".to_string()];
        assert_eq!(sheet_name("Riscos", &taken), "Riscos (2)");
        assert_eq!(sheet_name("a/b:c", &[]), "a-b-c");
        assert_eq!(sheet_name(&"x".repeat(40), &[]).chars().count(), 31);
        assert_eq!(number("1.234,5"), Some(1234.5));
        assert_eq!(number("12,5"), Some(12.5));
        assert_eq!(number("007"), None);
        assert_eq!(number("abc"), None);
        let h = vec!["Modo".to_string(), "RPN".to_string()];
        let rows = vec![vec!["trinca".to_string(), "120".to_string()]];
        let b = xlsx(&[("FMEA", &h, &rows), ("FMEA", &h, &rows)]).unwrap();
        // A zip file (the .xlsx container).
        assert_eq!(&b[..2], b"PK");
    }
}
