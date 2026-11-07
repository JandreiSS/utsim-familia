//! The package of quality and occupational safety missions
//! (`ensino/pacotes/qualidade_seguranca`, stage QS18 of `docs/plano_qualidade_e_seguranca.md`):
//! it loads, every mission passes the checker (the exhaustive search confirms the reference),
//! the glossary links point to manual chapters and the texts follow the writing rules.

use std::path::{Path, PathBuf};

use utsim_ensino_core::conferir::{conferir, descrever};
use utsim_ensino_core::pacote::{Arquivos, PacoteCarregado, carregar};

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pasta() -> PathBuf {
    raiz().join("ensino/pacotes/qualidade_seguranca")
}

fn ler(dir: &Path, base: &Path, out: &mut Arquivos) {
    for e in std::fs::read_dir(dir).expect("pasta") {
        let p = e.expect("entrada").path();
        if p.is_dir() {
            ler(&p, base, out);
        } else {
            let rel: Vec<String> = p
                .strip_prefix(base)
                .expect("dentro")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.insert(rel.join("/"), std::fs::read(&p).expect("ler"));
        }
    }
}

fn arquivos() -> Arquivos {
    let mut a = Arquivos::new();
    ler(&pasta(), &pasta(), &mut a);
    a
}

fn pacote() -> PacoteCarregado {
    carregar(&arquivos()).unwrap_or_else(|e| panic!("{}", e.join("\n")))
}

#[test]
fn o_pacote_tem_as_seis_missoes() {
    let p = pacote();
    assert_eq!(p.missoes.len(), 6, "{} missões", p.missoes.len());
    for m in &p.missoes {
        assert!(!m.missao.dicas.is_empty(), "{}: sem dicas", m.missao.id);
        assert!(
            !m.missao.metricas.is_empty(),
            "{}: sem métricas",
            m.missao.id
        );
    }
}

#[test]
fn todas_as_missoes_passam_no_conferidor() {
    let p = pacote();
    let mut falhas = Vec::new();
    for m in &p.missoes {
        let c = conferir(m).unwrap();
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
fn glossario_aponta_capitulos_do_manual() {
    let p = pacote();
    let manual = raiz().join("docs/manual/src");
    for t in &p.glossario.termos {
        if let Some(m) = &t.manual {
            assert!(manual.join(m).exists(), "{}: capítulo {m} não existe", t.id);
        }
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

#[test]
fn os_casos_de_seguranca_trazem_o_aviso() {
    let a = arquivos();
    for nome in ["qs/cruzamento.md", "qs/posto_pesado.md"] {
        let txt = String::from_utf8(a[nome].clone()).unwrap();
        assert!(
            txt.contains("não substitui o profissional habilitado"),
            "{nome}"
        );
        assert!(txt.contains("não certifica conformidade"), "{nome}");
    }
    let cruz = String::from_utf8(a["qs/cruzamento.md"].clone()).unwrap();
    assert!(cruz.contains("não estima a probabilidade de um acidente real"));
}
