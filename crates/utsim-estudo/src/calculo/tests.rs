use super::*;
use crate::exemplo::estudo_demonstracao;
use crate::relatorio::{self, Contexto, Identidade};

#[test]
fn every_tool_of_the_demonstration_has_a_result_without_warnings() {
    let e = estudo_demonstracao();
    assert_eq!(e.aplicacoes.len(), Ferramenta::TODAS.len());
    for a in &e.aplicacoes {
        let r = calcular(a, &e.assunto.problema);
        assert!(!r.vazio(), "{:?}: {:?}", a.ferramenta, r.avisos);
        assert!(r.avisos.is_empty(), "{:?}: {:?}", a.ferramenta, r.avisos);
    }
}

#[test]
fn the_results_say_what_the_data_show() {
    let e = estudo_demonstracao();
    let r = |f: Ferramenta| {
        let a = e.aplicacoes.iter().find(|a| a.ferramenta == f).unwrap();
        calcular(a, &e.assunto.problema)
    };
    // Pareto: rebarba first; the cost puts the displaced hole first.
    let p = r(Ferramenta::Pareto);
    assert_eq!(p.graficos[0].labels[0], "Rebarba");
    assert_eq!(p.graficos[1].labels[0], "Furo deslocado");
    assert!(
        p.achados
            .iter()
            .any(|a| a.titulo == "O custo muda a prioridade")
    );
    // The stratification finds shift C.
    let s = r(Ferramenta::Estratificacao);
    assert!(
        s.achados.iter().any(|a| a.texto.starts_with('C')),
        "{:?}",
        s.achados
    );
    // The check sheet flags Friday.
    let f = r(Ferramenta::FolhaVerificacao);
    assert!(
        f.achados.iter().any(|a| a.texto.starts_with("Sex")),
        "{:?}",
        f.achados
    );
    // The control chart sees the shift near the end.
    let c = r(Ferramenta::CartaControle);
    assert!(c.achados.iter().any(|a| a.tom == Tone::Danger));
    assert_eq!(c.graficos.len(), 2);
    // The FMEA: the 9-severity row without action is flagged.
    let m = r(Ferramenta::Fmea);
    assert!(m.achados.iter().any(|a| a.texto.contains("Diâmetro acima")));
    // The scatter: strong positive correlation.
    let d = r(Ferramenta::Dispersao);
    assert!(
        d.achados[0].texto.contains("forte e positiva"),
        "{:?}",
        d.achados
    );
    // MSA: an acceptable or conditional gauge, with the tolerance column.
    let g = r(Ferramenta::Msa);
    assert!(g.quadros[0].cabecalho.iter().any(|c| c == "%Tolerância"));
}

#[test]
fn empty_and_bad_cells_are_warnings_not_panics() {
    for f in Ferramenta::TODAS {
        let a = Aplicacao::nova("a1", f);
        let r = calcular(&a, "");
        assert!(!r.avisos.is_empty(), "{f:?}");
        let mut a = Aplicacao::nova("a1", f);
        if f.usa_tabela() {
            let n = a.tabela.colunas.len();
            a.tabela.linhas = vec![
                vec!["x".into(); n],
                vec!["1".into(); n],
                vec![String::new(); n],
            ];
            let _ = calcular(&a, "");
        }
    }
    let mut a = Aplicacao::nova("a1", Ferramenta::Histograma);
    a.tabela.linhas = vec![vec!["1".into()], vec!["abc".into()], vec!["2".into()]];
    let r = calcular(&a, "");
    assert!(r.avisos[0].contains("linha 2"), "{:?}", r.avisos);
}

fn salvar(nome: &str, bytes: &[u8]) {
    if let Some(d) = std::env::var_os("UTSIM_CAPTURAS") {
        let d = std::path::PathBuf::from(d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(nome), bytes).unwrap();
    }
}

#[test]
fn reports_per_tool_and_consolidated_in_pdf_and_html() {
    let e = estudo_demonstracao();
    let id = Identidade {
        app: "UTSim Qualidade".into(),
        cor: "#1F6FB2".into(),
        icone_png: Vec::new(),
    };
    let ctx = Contexto {
        identidade: &id,
        autor: "Ana Souza",
        data: "09/11/2026",
        nome_do_posto: None,
    };
    let t = relatorio::modelo_do_app(&id);
    for a in &e.aplicacoes {
        let d = relatorio::documento_ferramenta(&e, a, &ctx);
        assert!(d.cover.is_some());
        let b = relatorio::pdf(&d, &t, "Impresso por Ana Souza em 09/11/2026");
        assert!(b.starts_with(b"%PDF-1.4"));
        let h = relatorio::html(&d, &t, "Impresso por Ana Souza em 09/11/2026");
        assert!(h.contains("UTSim Qualidade"));
        salvar(&format!("estudo_{}.pdf", a.id), &b);
        salvar(&format!("estudo_{}.html", a.id), h.as_bytes());
    }
    let d = relatorio::documento_estudo(&e, &[], &ctx);
    assert!(d.toc);
    assert_eq!(d.headings().len(), e.aplicacoes.len() + 2);
    let b = relatorio::pdf(&d, &t, "Impresso por Ana Souza em 09/11/2026");
    salvar("estudo_consolidado.pdf", &b);
    salvar(
        "estudo_consolidado.html",
        relatorio::html(&d, &t, "Impresso por Ana Souza em 09/11/2026").as_bytes(),
    );
    // Only the chosen applications.
    let d = relatorio::documento_estudo(&e, &["a3".into(), "a9".into()], &ctx);
    assert_eq!(d.headings().len(), 4);
}

#[test]
fn pareto_without_the_cost_column() {
    let e = estudo_demonstracao();
    let mut a = e
        .aplicacoes
        .iter()
        .find(|a| a.ferramenta == Ferramenta::Pareto)
        .unwrap()
        .clone();
    a.parametros.colunas = vec![0, 1, usize::MAX];
    let r = calcular(&a, "");
    assert_eq!(r.graficos.len(), 1);
    assert!(r.avisos.is_empty());
}
