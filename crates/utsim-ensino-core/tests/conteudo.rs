//! The content of the built-in campaign (`docs/plano_ensino_e_jogo.md`, 6.4): every mission
//! passes the checker (exhaustive search of the reference), the stored numbers of the
//! reference (golden) still hold, the glossary links point to manual chapters and the
//! texts follow the writing rules.
//!
//! An intended change of the engine or of a mission regenerates the golden with
//! `UTSIM_ENSINO_GOLDEN=1 cargo test -p utsim-ensino-core --test conteudo`, with review.

use std::path::PathBuf;

use utsim_ensino_core::campanha::{arquivos, carregar_campanha};
use utsim_ensino_core::conferir::{Golden, conferir, descrever, golden};

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn todas_as_missoes_passam_no_conferidor() {
    let p = carregar_campanha().unwrap_or_else(|e| panic!("{}", e.join("\n")));
    assert!(p.missoes.len() >= 14, "{} missões", p.missoes.len());
    let mut falhas = Vec::new();
    for m in &p.missoes {
        let c = conferir(m).unwrap();
        for a in &c.avisos {
            eprintln!("{}: aviso: {a}", m.missao.id);
        }
        if !c.passou() {
            falhas.push(format!(
                "{}: {} (melhor: {})",
                m.missao.id,
                c.erros.join("; "),
                descrever(m, &c.melhor)
            ));
        }
    }
    assert!(falhas.is_empty(), "\n{}", falhas.join("\n"));
}

#[test]
fn golden_da_referencia() {
    let p = carregar_campanha().unwrap();
    let atual: Vec<Golden> = p.missoes.iter().map(|m| golden(m).unwrap()).collect();
    let path = raiz().join("crates/utsim-ensino-core/tests/golden.ron");
    if std::env::var_os("UTSIM_ENSINO_GOLDEN").is_some() {
        let txt = ron::ser::to_string_pretty(&atual, ron::ser::PrettyConfig::default()).unwrap();
        std::fs::write(&path, format!("// Gerado por UTSIM_ENSINO_GOLDEN=1 cargo test -p utsim-ensino-core --test conteudo\n{txt}\n")).unwrap();
        return;
    }
    let guardado: Vec<Golden> =
        ron::from_str(&std::fs::read_to_string(&path).expect("golden.ron")).expect("golden.ron");
    assert_eq!(
        guardado.len(),
        atual.len(),
        "missões novas ou removidas: regenere o golden"
    );
    for (g, a) in guardado.iter().zip(&atual) {
        assert_eq!(g.missao, a.missao);
        assert_eq!(g.referencia, a.referencia, "{}: referência mudou", g.missao);
        for ((n1, v1), (n2, v2)) in g
            .base
            .iter()
            .chain(&g.ref_valores)
            .zip(a.base.iter().chain(&a.ref_valores))
        {
            assert_eq!(n1, n2);
            let tol = 1e-9 * v1.abs().max(1.0);
            assert!(
                (v1 - v2).abs() <= tol,
                "{}: {n1} era {v1}, agora {v2}",
                g.missao
            );
        }
    }
}

#[test]
fn glossario_aponta_capitulos_do_manual() {
    let p = carregar_campanha().unwrap();
    let manual = raiz().join("docs/manual/src");
    for t in &p.glossario.termos {
        if let Some(m) = &t.manual {
            assert!(manual.join(m).exists(), "{}: capítulo {m} não existe", t.id);
        }
    }
    // Every concept of a mission is in the glossary, and every term is used.
    for t in &p.glossario.termos {
        assert!(
            p.missoes.iter().any(|m| m.missao.conceitos.contains(&t.id)),
            "termo '{}' não é usado por nenhuma missão",
            t.id
        );
    }
}

#[test]
fn textos_seguem_as_regras_de_escrita() {
    for (path, bytes) in arquivos() {
        if !(path.ends_with(".md") || path.ends_with(".ron")) {
            continue;
        }
        let txt = String::from_utf8(bytes).unwrap();
        for (i, l) in txt.lines().enumerate() {
            assert!(!l.contains('\u{2014}'), "{path}:{}: travessão", i + 1);
            if path.ends_with(".md") {
                assert!(!l.contains(';'), "{path}:{}: ponto e vírgula", i + 1);
            }
        }
    }
}
