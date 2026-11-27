//! The package of environmental missions "Fábrica verde"
//! (`ensino/pacotes/fabrica_verde`, stage AM12 of the UTSim Ambiental plan):
//! it loads, every mission passes the checker (the exhaustive search confirms the reference),
//! the glossary links point to manual chapters and the texts follow the writing rules.

use std::path::{Path, PathBuf};

use utsim_ensino_core::conferir::{conferir, descrever};
use utsim_ensino_core::pacote::{Arquivos, PacoteCarregado, carregar};

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pasta() -> PathBuf {
    raiz().join("ensino/pacotes/fabrica_verde")
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
fn o_pacote_tem_as_quatro_missoes() {
    let p = pacote();
    assert_eq!(p.missoes.len(), 4, "{} missões", p.missoes.len());
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
fn os_fatores_sao_didaticos() {
    let a = arquivos();
    for (path, bytes) in &a {
        if !path.starts_with("modelos/") {
            continue;
        }
        let m = utsim_model::Model::parse(std::str::from_utf8(bytes).unwrap()).unwrap();
        let cat = m.environment.as_deref().expect("catálogo no modelo");
        for f in &cat.factors {
            assert!(
                f.didactic,
                "{path}: fator de {} sem o selo didático",
                f.flow
            );
        }
    }
}

/// Os valores de cada combinação, para calibrar as metas: `cargo test -p utsim-ensino-core
/// --test fabrica_verde valores -- --ignored --nocapture`.
#[test]
#[ignore]
fn valores() {
    let p = pacote();
    for m in &p.missoes {
        let mut lista = vec![m.missao.decisoes_iniciais()];
        lista.extend(m.missao.combinacoes());
        let cen = utsim_ensino_core::motor::rodar(m, "", &lista, &|_| true).unwrap();
        println!("== {}", m.missao.id);
        for c in &cen {
            let v: Vec<String> = m
                .missao
                .metricas
                .iter()
                .map(|mt| {
                    let x = utsim_ensino_core::motor::valor_metrica(&m.missao, mt, c).unwrap();
                    format!("{}={:.4}", mt.nome, x.iter().sum::<f64>() / x.len() as f64)
                })
                .collect();
            println!("{:?} custo={} {}", c.decisoes, c.custo, v.join(" "));
        }
        for pr in &m.missao.previsao {
            println!("previsao {:?}", pr);
        }
    }
}
