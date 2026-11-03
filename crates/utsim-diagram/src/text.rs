//! Small text helpers: number format, accent folding and escaping.

/// Lower case without accents, to compare names and keywords.
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Number with at most two decimals and no trailing zeros, dot as separator.
pub fn num_dot(x: f64) -> String {
    if !x.is_finite() {
        return String::new();
    }
    let s = if x.abs() >= 1000.0 {
        format!("{x:.0}")
    } else if x.abs() >= 100.0 {
        format!("{x:.1}")
    } else {
        format!("{x:.2}")
    };
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Number for screens and documents: decimal comma.
pub fn num(x: f64) -> String {
    num_dot(x).replace('.', ",")
}

/// Percentage of a fraction, e.g. `0.632` gives "63,2%".
pub fn pct(fraction: f64) -> String {
    format!("{}%", num((1000.0 * fraction).round() / 10.0))
}

/// Escapes text for HTML and SVG.
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Rough width of a text drawn with the given font size.
pub fn text_width(s: &str, font: f64) -> f64 {
    s.chars().count() as f64 * font * 0.56
}
