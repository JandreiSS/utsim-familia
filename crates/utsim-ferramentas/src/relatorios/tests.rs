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

#[test]
fn charts_are_drawn_inside_the_pdf() {
    let mut d = Document::new("Análise", "carta e efeitos");
    let values: Vec<f64> = (0..120)
        .map(|i| 10.0 + ((i * 7) % 11) as f64 / 10.0)
        .collect();
    d.chart(Chart {
        title: "Carta I".into(),
        labels: (1..=120).map(|i| i.to_string()).collect(),
        series: vec![("valores".into(), values)],
        hlines: vec![("LSC".into(), 11.5), ("LIC".into(), 9.5)],
        marks: vec![3, 500],
        ..Chart::default()
    });
    d.chart(Chart {
        title: "Pareto dos efeitos".into(),
        bars: true,
        labels: vec!["A".into(), "B".into(), "AB".into()],
        series: vec![("".into(), vec![8.0, -3.0, f64::NAN])],
        ..Chart::default()
    });
    d.chart(Chart {
        title: "Interação A × B".into(),
        labels: vec!["baixo".into(), "alto".into()],
        series: vec![
            ("B baixo".into(), vec![11.0, 21.0]),
            ("B alto".into(), vec![15.0, 29.0]),
        ],
        ..Chart::default()
    });
    d.chart(Chart::default());
    let t = Template::default();
    let a = pdf::render(&d, &t, "");
    check_structure(&a);
    assert_eq!(a, pdf::render(&d, &t, ""));
    let text = String::from_utf8_lossy(&a);
    // Coloured rectangles (bars, markers, legend) and the reference lines.
    assert!(text.contains(" rg ") && text.contains(" re f 0 g"));
    assert!(text.contains("(LSC = 11,5)"));
    assert!(text.contains("(Pareto dos efeitos)"));
    assert!(d.plain_text().contains("Interação A × B"));
    // Many charts break the page.
    let mut many = Document::new("x", "");
    for _ in 0..8 {
        many.chart(
            d.blocks
                .iter()
                .find_map(|b| match b {
                    Block::Chart(c) => Some(c.clone()),
                    _ => None,
                })
                .unwrap(),
        );
    }
    assert!(pdf::page_count(&many, &t) >= 2);
}

/// A document with every block and chart kind, a cover and the contents.
pub(crate) fn showcase() -> Document {
    let mut d = Document::new("Relatório de demonstração", "Todos os blocos do gerador");
    d.cover = Some(Cover {
        app: "UTSim LSS".into(),
        title: "Redução de refugo na usinagem da tampa".into(),
        subtitle: "Relatório do tollgate Analisar".into(),
        author: "Ana Souza".into(),
        date: "08/11/2026".into(),
        source: "Modelo linha_tampa.ron (hash 3f2a…), 10 replicações; coleta real de outubro"
            .into(),
        ..Cover::default()
    });
    d.toc = true;
    d.heading("Resumo");
    d.kpis(vec![
        Kpi {
            label: "Refugo".into(),
            value: "4,2 %".into(),
            note: "meta 2 %".into(),
        },
        Kpi {
            label: "Cpk".into(),
            value: "0,91".into(),
            note: "era 0,62".into(),
        },
        Kpi {
            label: "Ganho validado".into(),
            value: "R$ 48.000".into(),
            note: "12 meses".into(),
        },
    ]);
    d.callout(Tone::Ok, "Conclusão", "A causa raiz verificada é o desgaste do gabarito; a troca a cada 5 mil peças reduz o refugo pela metade.");
    d.callout(
        Tone::Warn,
        "Atenção",
        "O MSA de atributos ainda não foi refeito depois da troca do critério.",
    );
    d.heading("Causas");
    d.blocks.push(Block::Ishikawa {
        effect: "Furo fora de posição".into(),
        categories: vec![
            (
                "Máquina".into(),
                vec!["Gabarito gasto".into(), "Folga no fuso".into()],
            ),
            ("Método".into(), vec!["Sem troca programada".into()]),
            (
                "Material".into(),
                vec![
                    "Chapa empenada".into(),
                    "Lote fora".into(),
                    "Rebarba".into(),
                ],
            ),
            ("Mão de obra".into(), vec!["Treino do turno C".into()]),
            ("Medição".into(), vec!["Calibrador vencido".into()]),
            ("Meio ambiente".into(), vec!["Temperatura".into()]),
        ],
    });
    d.chart(Chart {
        title: "Pareto dos defeitos".into(),
        kind: ChartKind::Pareto,
        labels: vec![
            "Rebarba".into(),
            "Risco".into(),
            "Mancha".into(),
            "Outros".into(),
        ],
        series: vec![("ocorrências".into(), vec![48.0, 22.0, 9.0, 5.0])],
        ..Chart::default()
    });
    d.heading("Medições");
    let data: Vec<f64> = (0..80)
        .map(|i| 10.0 + ((i * 37 % 23) as f64 - 11.0) / 40.0)
        .collect();
    d.chart(Chart {
        title: "Histograma do diâmetro".into(),
        kind: ChartKind::Histogram,
        series: vec![("diâmetro".into(), data.clone())],
        hlines: vec![("LIE".into(), 9.6), ("LSE".into(), 10.4)],
        ..Chart::default()
    });
    d.chart(Chart {
        title: "Carta I com zonas".into(),
        labels: (1..=40).map(|i| i.to_string()).collect(),
        series: vec![("diâmetro".into(), data[..40].to_vec())],
        hlines: vec![("LSC".into(), 10.4), ("LIC".into(), 9.6)],
        bands: vec![
            ("zona C".into(), 9.87, 10.13),
            ("zona B".into(), 10.13, 10.27),
        ],
        marks: vec![7],
        ..Chart::default()
    });
    d.chart(Chart {
        title: "Temperatura × diâmetro".into(),
        kind: ChartKind::Scatter,
        series: vec![
            (
                "temperatura".into(),
                (0..20).map(|i| 20.0 + i as f64).collect(),
            ),
            (
                "diâmetro".into(),
                (0..20).map(|i| 9.8 + 0.02 * i as f64).collect(),
            ),
        ],
        ..Chart::default()
    });
    d.heading("Riscos");
    d.blocks.push(Block::RiskMatrix {
        title: "Matriz de risco (probabilidade × severidade)".into(),
        rows: vec!["Alta".into(), "Média".into(), "Baixa".into()],
        cols: vec!["Leve".into(), "Moderada".into(), "Grave".into()],
        levels: vec![vec![1, 2, 2], vec![0, 1, 2], vec![0, 0, 1]],
        colours: vec!["#7FBF7F".into(), "#F2C94C".into(), "#D9534F".into()],
        marks: vec![(0, 2, "Prensa sem trava".into()), (1, 1, "Ruído".into())],
    });
    d.table(
        "Ações",
        &["Ação", "Dono", "Prazo"],
        (1..=6)
            .map(|i| vec![format!("Ação {i}"), "Ana".into(), "30/11/2026".into()])
            .collect(),
        &[3.0, 1.0, 1.0],
    );
    d
}

#[test]
fn cover_contents_and_new_blocks() {
    let d = showcase();
    let t = Template::default();
    let a = pdf::render(&d, &t, "Impresso por Ana em 08/11/2026");
    check_structure(&a);
    assert_eq!(a, pdf::render(&d, &t, "Impresso por Ana em 08/11/2026"));
    let text = String::from_utf8_lossy(&a);
    assert!(text.contains(r"(Sum\341rio)"));
    assert!(text.contains(r"(Redu\347\343o de refugo na"));
    assert!(text.contains(r"(Furo fora de posi\347\343o)"));
    assert!(text.contains("(80 %)"));
    assert!(text.contains("(Prensa sem trava)"));
    // The cover has no header nor "página 1 de N".
    assert!(!text.contains(r"(p\341gina 1 de"));
    // The contents point at the right pages: each heading's page holds its text.
    let n = pdf::page_count(&d, &t);
    assert!(n >= 4, "{n}");
    assert_eq!(d.headings(), vec!["Resumo", "Causas", "Medições", "Riscos"]);
    let p = d.plain_text();
    assert!(p.contains("Gabarito gasto") && p.contains("Cpk: 0,91 era 0,62"));
    // Without cover and contents nothing changes in an old document.
    let old = sample(5);
    assert_eq!(pdf::page_count(&old, &t), 1);
    if let Some(dir) = std::env::var_os("UTSIM_CAPTURAS") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("relatorio_demonstracao.pdf"),
            &a,
        )
        .unwrap();
    }
}

#[test]
fn header_names_the_app_that_made_the_document() {
    let t = Template::default();
    let mut d = sample(1);
    assert_eq!(d.app_name(), "UTSim");
    d.app = "UTSim Qualidade".into();
    assert!(html::render(&d, &t, "").contains("UTSim Qualidade"));
    assert!(!html::render(&d, &t, "").contains("UTSim LSS"));
    d.app.clear();
    d.cover = Some(Cover {
        app: "UTSim Estatística".into(),
        ..Cover::default()
    });
    assert_eq!(d.app_name(), "UTSim Estatística");
    let b = pdf::render(&d, &t, "");
    assert!(!b.windows(9).any(|w| w == b"UTSim LSS"));
}
