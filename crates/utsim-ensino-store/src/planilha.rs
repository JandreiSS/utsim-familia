//! The class grades as `.xlsx`: one sheet per table, numbers as numbers.

use rust_xlsxwriter::{Format, Workbook};

use crate::{Erro, Result};

/// A table: sheet name, header and rows.
pub type Tabela = (String, Vec<String>, Vec<Vec<String>>);

/// A number written with the decimal comma (`12,5`) or point, else `None`.
fn numero(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty()
        || t.len() > 30
        || (t.len() > 1 && t.starts_with('0') && !t.starts_with("0,") && !t.starts_with("0."))
    {
        return None;
    }
    t.replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
}

/// A sheet name Excel accepts (31 characters, none of `[]:*?/\`).
fn nome_aba(nome: &str) -> String {
    let n: String = nome
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '-' } else { c })
        .take(31)
        .collect();
    if n.trim().is_empty() {
        "Tabela".into()
    } else {
        n
    }
}

/// The workbook of `tabelas` as bytes.
pub fn xlsx(tabelas: &[Tabela]) -> Result<Vec<u8>> {
    let e = |x: rust_xlsxwriter::XlsxError| Erro::Invalido(x.to_string());
    let mut wb = Workbook::new();
    let negrito = Format::new().set_bold();
    for (nome, cab, linhas) in tabelas {
        let ws = wb.add_worksheet();
        ws.set_name(nome_aba(nome)).map_err(e)?;
        for (c, h) in cab.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &negrito)
                .map_err(e)?;
        }
        for (r, linha) in linhas.iter().enumerate() {
            for (c, v) in linha.iter().enumerate() {
                let (rr, cc) = (r as u32 + 1, c as u16);
                match numero(v) {
                    Some(x) => ws.write_number(rr, cc, x).map(|_| ()).map_err(e)?,
                    None => ws.write_string(rr, cc, v).map(|_| ()).map_err(e)?,
                }
            }
        }
    }
    if tabelas.is_empty() {
        wb.add_worksheet();
    }
    wb.save_to_buffer().map_err(e)
}
