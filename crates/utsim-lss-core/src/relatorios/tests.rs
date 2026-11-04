use proptest::prelude::*;

use super::pdf::{self, encode, width, wrap};
use super::*;

fn sample(rows: usize) -> Document {
    let mut d = Document::new("Termo de abertura", "Redução do refugo na prensa 3");
    d.heading("Problema")
        .para("O refugo da prensa subiu de 2,1 % para 4,8 % em seis meses; a meta é voltar a 2 %.")
        .kv(vec![
            ("Patrocinador".into(), "João Ávila".into()),
            ("Prazo".into(), "30/04/2027".into()),
        ])
        .table(
            "Riscos",
            &["Risco", "Probabilidade", "Impacto"],
            (0..rows)
                .map(|i| {
                    vec![
                        format!("Risco nº {i}: atraso na peça"),
                        "3".into(),
                        "4".into(),
                    ]
                })
                .collect(),
            &[3.0, 1.0, 1.0],
        )
        .bullets(vec!["Ação ① com ç e ã".into()])
        .signatures(vec!["Patrocinador".into()]);
    d
}

/// The byte offsets of the xref table point to each object (on the raw bytes: the
/// header and the WinAnsi text are not UTF-8).
fn check_structure(bytes: &[u8]) {
    let find = |needle: &[u8], from: usize| -> Option<usize> {
        bytes[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + from)
    };
    let rfind = |needle: &[u8]| -> usize {
        bytes
            .windows(needle.len())
            .rposition(|w| w == needle)
            .unwrap()
    };
    let num_at = |at: usize| -> usize {
        let s: String = bytes[at..]
            .iter()
            .skip_while(|b| b.is_ascii_whitespace())
            .take_while(|b| b.is_ascii_digit())
            .map(|b| *b as char)
            .collect();
        s.parse().unwrap()
    };
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert!(bytes.trim_ascii_end().ends_with(b"%%EOF"));
    let xref = num_at(rfind(b"startxref") + 9);
    assert!(bytes[xref..].starts_with(b"xref"));
    let count = num_at(xref + 7);
    let entries = find(
        b"65535 f 
",
        xref,
    )
    .unwrap()
        + 9;
    for i in 1..count {
        let line = &bytes[entries + (i - 1) * 20..entries + (i - 1) * 20 + 10];
        let off: usize = std::str::from_utf8(line).unwrap().parse().unwrap();
        let head = format!("{i} 0 obj");
        assert_eq!(&bytes[off..off + head.len()], head.as_bytes(), "objeto {i}");
    }
    // Every stream's /Length is its real length.
    let mut at = 0;
    while let Some(p) = find(b"/Length ", at) {
        let len = num_at(p + 8);
        let s = find(
            b"stream
", p,
        )
        .unwrap()
            + 7;
        assert_eq!(&bytes[s + len..s + len + 9], b"endstream");
        at = s + len;
    }
}

#[test]
fn pdf_is_well_formed_and_deterministic() {
    let d = sample(5);
    let t = Template::default();
    let a = pdf::render(&d, &t, "Impresso por Ana em 03/11/2026");
    check_structure(&a);
    assert_eq!(a, pdf::render(&d, &t, "Impresso por Ana em 03/11/2026"));
    let text = String::from_utf8_lossy(&a);
    assert!(text.contains("/BaseFont /Helvetica /Encoding /WinAnsiEncoding"));
    assert!(text.contains("p\\341gina 1 de 1"));
}

#[test]
fn long_tables_break_pages_and_repeat_the_header() {
    let d = sample(200);
    let t = Template::default();
    let n = pdf::page_count(&d, &t);
    assert!(n >= 4, "{n}");
    let bytes = pdf::render(&d, &t, "");
    check_structure(&bytes);
    let text = String::from_utf8_lossy(&bytes);
    // The header of the table on every page with rows.
    assert!(text.matches("(Probabilidade)").count() >= n - 1);
    assert!(text.contains(&format!("de {n})")));
    let mut a3 = d.clone();
    a3.size = PageSize::A3Landscape;
    assert!(pdf::page_count(&a3, &t) <= n);
}

#[test]
fn portuguese_letters_in_winansi() {
    assert_eq!(encode("ação"), vec![b'a', 0xE7, 0xE3, b'o']);
    assert_eq!(encode("“€”—•"), vec![0x93, 0x80, 0x94, 0x97, 0x95]);
    assert_eq!(encode("a → b ≥ c"), b"a -> b >= c".to_vec());
    assert_eq!(encode("日"), b"?".to_vec());
    assert!((width("a", 10.0, false) - 5.56).abs() < 1e-9);
    assert!(width("ã", 10.0, false) == width("a", 10.0, false));
}

#[test]
fn template_colour_and_json() {
    let t = Template {
        company: "Metalúrgica Exemplo".into(),
        colour: "#FF8000".into(),
        ..Template::default()
    };
    assert_eq!(t.rgb(), (1.0, 128.0 / 255.0, 0.0));
    assert_eq!(Template::from_json(&t.to_json()).unwrap(), t);
    let bad = Template {
        colour: "azul".into(),
        ..Template::default()
    };
    assert_eq!(bad.rgb(), Template::default().rgb());
    let d = sample(1);
    let text = String::from_utf8_lossy(&pdf::render(&d, &t, "")).into_owned();
    assert!(text.contains("(Metal\\372rgica Exemplo)"));
    assert!(Template::from_json("{}").unwrap() == Template::default());
}

#[test]
fn csv_of_the_tables() {
    let mut d = sample(2);
    d.table(
        "FMEA",
        &["Modo", "RPN"],
        vec![vec!["trinca; rebarba".into(), "120".into()]],
        &[],
    );
    let c = csv(&d);
    assert!(c.starts_with("Riscos\nRisco;Probabilidade;Impacto\n"));
    assert!(c.contains("\"trinca; rebarba\";120"));
    assert!(d.plain_text().contains("João Ávila"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// No wrapped line is wider than the limit, unless it is a single character, and the
    /// words come out in order.
    #[test]
    fn wrap_fits(s in "[a-zçãé ]{0,200}", max in 20.0f64..300.0, size in 6.0f64..14.0) {
        let lines = wrap(&s, max, size, false);
        for l in &lines {
            prop_assert!(width(l, size, false) <= max + 1e-9 || l.chars().count() == 1, "{l:?}");
        }
        let joined: String = lines.concat().split_whitespace().collect();
        let orig: String = s.split_whitespace().collect();
        prop_assert_eq!(joined, orig);
    }
}
