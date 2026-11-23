//! Numbers, money and dates in the language now (the rules of UTSim LSS, P8): a
//! comma as decimal mark except in English, dates day first except in English (which takes
//! `AAAA-MM-DD`, the form no reader takes for month-first or day-first), and money in R$
//! whatever the language.

use crate::{Idioma, atual};

/// A number written with a point as decimal mark, in the language's form.
pub fn marca_decimal(s: &str) -> String {
    match atual().separador_decimal() {
        '.' => s.to_string(),
        m => s.replace('.', &m.to_string()),
    }
}

/// Digits with the language's mark between thousands: `1234567` → `1.234.567`.
pub fn agrupar_milhares(digitos: &str) -> String {
    let marca = atual().separador_milhar();
    let n = digitos.chars().count();
    let mut s = String::with_capacity(digitos.len() + n / 3 * 2);
    for (i, c) in digitos.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            s.push(marca);
        }
        s.push(c);
    }
    s
}

/// An amount of money: `R$ 1.234,56`, `R$ 1,234.56` in English, `R$ 1 234,56` in French. The
/// currency is the company's, not the language's, so the symbol stays.
pub fn dinheiro(v: f64) -> String {
    let neg = v < 0.0;
    let centavos = (v.abs() * 100.0).round() as u64;
    let (inteiro, fracao) = (centavos / 100, centavos % 100);
    let decimal = atual().separador_decimal();
    format!(
        "{}R$ {}{decimal}{fracao:02}",
        if neg { "−" } else { "" },
        agrupar_milhares(&inteiro.to_string())
    )
}

/// A date: `DD/MM/AAAA`, or `AAAA-MM-DD` in English.
pub fn data(ano: i32, mes: u32, dia: u32) -> String {
    match atual() {
        Idioma::En => format!("{ano:04}-{mes:02}-{dia:02}"),
        _ => format!("{dia:02}/{mes:02}/{ano:04}"),
    }
}

/// A month: `MM/AAAA`, or `AAAA-MM` in English.
pub fn mes(ano: i32, mes: u32) -> String {
    match atual() {
        Idioma::En => format!("{ano:04}-{mes:02}"),
        _ => format!("{mes:02}/{ano:04}"),
    }
}

/// A number as typed. With a comma as decimal mark, `1.234,5` (or `1234.5`, with a point);
/// in English `1,234.5` (or `1,5`, a comma not followed by groups of three digits). Spaces
/// (the French mark between thousands) are ignored.
pub fn ler_numero(s: &str) -> Option<f64> {
    let t: String = s
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{202f}')
        .collect();
    if t.is_empty() {
        return None;
    }
    let t = if atual() == Idioma::En {
        let milhares = t.contains('.')
            || t.split(',')
                .skip(1)
                .all(|g| g.len() == 3 && g.bytes().all(|b| b.is_ascii_digit()));
        if milhares {
            t.replace(',', "")
        } else {
            t.replace(',', ".")
        }
    } else if t.contains(',') {
        t.replace('.', "").replace(',', ".")
    } else {
        t
    };
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}
