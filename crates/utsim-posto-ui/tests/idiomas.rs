//! As telas comuns nas cinco línguas: toda chave que o código pede existe em todas elas, e os
//! nomes de tela mudam com a língua.

use utsim_idiomas::Idioma;
use utsim_posto_ui::texto::{TEXTOS, tx};

/// As chaves escritas no código (`tx("...")`, `fx("...")` e as chaves guardadas em tabelas,
/// como `"carta-imr"` e `"papel-valor"`).
fn chaves_do_codigo() -> Vec<String> {
    let mut pastas = vec![std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src"
    ))];
    let mut arquivos = Vec::new();
    // As subpastas também (ferramentas/desenho.rs).
    while let Some(p) = pastas.pop() {
        for e in std::fs::read_dir(p).unwrap() {
            let c = e.unwrap().path();
            if c.is_dir() {
                pastas.push(c);
            } else {
                arquivos.push(c);
            }
        }
    }
    let mut chaves = Vec::new();
    for arquivo in arquivos {
        let texto = std::fs::read_to_string(arquivo).unwrap();
        for parte in texto.split('"').skip(1).step_by(2) {
            let prefixos = [
                "abrir-",
                "ferramentas-",
                "inicial-",
                "modelo-",
                "postos-",
                "sessao-",
                "trabalho-",
                "util-",
                "papel-",
                "carta-",
                "regras-",
            ];
            if prefixos.iter().any(|p| parte.starts_with(p))
                && parte
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_".contains(c))
            {
                chaves.push(parte.to_string());
            }
        }
    }
    chaves.sort();
    chaves.dedup();
    chaves
}

#[test]
fn as_telas_comuns_nas_cinco_linguas() {
    let chaves = chaves_do_codigo();
    assert!(chaves.len() > 250, "{}", chaves.len());
    for k in &chaves {
        assert!(TEXTOS.tem(k), "falta em pt-BR: {k}");
        assert_eq!(TEXTOS.em_todas(k).len(), 5, "falta em alguma língua: {k}");
    }
    let mut vistos = Vec::new();
    for i in Idioma::TODOS {
        utsim_idiomas::definir_nesta_thread(Some(i));
        vistos.push(tx("postos-postos_de_trabalho"));
    }
    vistos.dedup();
    assert_eq!(vistos.len(), Idioma::TODOS.len(), "{vistos:?}");
    utsim_idiomas::definir_nesta_thread(None);
    assert!(TEXTOS.faltas().is_empty(), "{:?}", TEXTOS.faltas());
}
