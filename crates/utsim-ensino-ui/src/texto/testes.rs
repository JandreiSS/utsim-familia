//! The catalogue is complete in the three languages, and no screen writes a literal text.

use super::*;

/// An entry: name, Portuguese, English, Spanish.
type Entrada = (&'static str, &'static str, &'static str, &'static str);

/// Every catalogue of the crate.
fn catalogos() -> Vec<(&'static str, &'static [Entrada])> {
    vec![
        ("geral", geral::TODOS),
        ("missao", cat_missao::TODOS),
        ("turma", cat_turma::TODOS),
        ("vista", cat_vista::TODOS),
    ]
}

fn marcas(s: &str) -> (usize, Vec<String>) {
    let mut seq = 0;
    let mut pos = Vec::new();
    let mut resto = s;
    while let Some(i) = resto.find('{') {
        let depois = &resto[i + 1..];
        match depois.find('}') {
            Some(j) if depois[..j].chars().all(|c| c.is_ascii_digit()) => {
                if j == 0 {
                    seq += 1;
                } else {
                    pos.push(depois[..j].to_string());
                }
                resto = &depois[j + 1..];
            }
            _ => resto = depois,
        }
    }
    pos.sort();
    (seq, pos)
}

#[test]
fn catalogo_completo_nas_tres_linguas() {
    let mut nomes = std::collections::BTreeSet::new();
    let mut erros = Vec::new();
    for (cat, todos) in catalogos() {
        for &(nome, pt, en, es) in todos {
            if !nomes.insert(nome) {
                erros.push(format!("{cat}::{nome}: nome repetido"));
            }
            for (l, t) in [("pt", pt), ("en", en), ("es", es)] {
                if t.trim().is_empty() {
                    erros.push(format!("{cat}::{nome}: {l} vazio"));
                }
                if t.contains('—') {
                    erros.push(format!("{cat}::{nome}: {l} com travessão"));
                }
            }
            let m = marcas(pt);
            for (l, t) in [("en", en), ("es", es)] {
                if marcas(t) != m {
                    erros.push(format!("{cat}::{nome}: {l} com valores diferentes do pt"));
                }
            }
        }
    }
    assert!(erros.is_empty(), "{}", erros.join("\n"));
}

#[test]
fn preenche_por_ordem_e_por_posicao() {
    assert_eq!(f("a {} b {}", &[&1, &"x"]), "a 1 b x");
    assert_eq!(f("{1} de {0}", &[&"ato", &3]), "3 de ato");
    assert_eq!(f("{x} {}", &[&7]), "{x} 7");
    assert_eq!(Idioma::de_codigo("en-US"), Idioma::En);
    assert_eq!(Idioma::de_codigo("es"), Idioma::Es);
    assert_eq!(Idioma::de_codigo("xx"), Idioma::Pt);
}

/// The string literals of a Rust source, with their line, outside comments, char literals
/// and `#[cfg(test)]` modules.
fn literais(src: &str) -> Vec<(usize, String)> {
    let src = match src.find("#[cfg(test)]") {
        Some(i) => &src[..i],
        None => src,
    };
    let c: Vec<char> = src.chars().collect();
    let mut v = Vec::new();
    let (mut i, mut linha) = (0, 1);
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            linha += 1;
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                if c[i] == '\n' {
                    linha += 1;
                }
                i += 1;
            }
            i += 2;
        } else if ch == '\'' {
            // A char literal ('"', '\n', 'x'); otherwise a lifetime.
            if c.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < c.len() && c[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if c.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if ch == 'r'
            && (c.get(i + 1) == Some(&'"') || c.get(i + 1) == Some(&'#'))
            && (i == 0 || !(c[i - 1].is_alphanumeric() || c[i - 1] == '_'))
        {
            let mut j = i + 1;
            let mut hashes = 0;
            while c.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if c.get(j) != Some(&'"') {
                i += 1;
                continue;
            }
            let inicio = linha;
            j += 1;
            let mut s = String::new();
            loop {
                if j >= c.len() {
                    break;
                }
                if c[j] == '"' && (0..hashes).all(|k| c.get(j + 1 + k) == Some(&'#')) {
                    j += 1 + hashes;
                    break;
                }
                if c[j] == '\n' {
                    linha += 1;
                }
                s.push(c[j]);
                j += 1;
            }
            v.push((inicio, s));
            i = j;
        } else if ch == '"' {
            let inicio = linha;
            let mut j = i + 1;
            let mut s = String::new();
            while j < c.len() && c[j] != '"' {
                if c[j] == '\\' {
                    s.push(c[j]);
                    j += 1;
                }
                if j < c.len() {
                    if c[j] == '\n' {
                        linha += 1;
                    }
                    s.push(c[j]);
                }
                j += 1;
            }
            v.push((inicio, s));
            i = j + 1;
        } else {
            i += 1;
        }
    }
    v
}

/// A literal that is a text for people: it has letters and is not an identifier, a key, a
/// path or a command line flag (lower case, digits, `_ . / : -` and no spaces).
fn e_texto(s: &str) -> bool {
    // Escapes are not letters (the literal keeps them as written in the source).
    let s = &s
        .replace("\\\\", "/")
        .replace("\\n", " ")
        .replace("\\t", " ")
        .replace("\\\"", "\"");
    if !s.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    let ident = s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_./:-{}<>|=".contains(c));
    if ident {
        return false;
    }
    // Formats with no words (`{:.1}`, `{e}`, `{:?}`).
    let sem_marcas = {
        let mut t = String::new();
        let mut dentro = false;
        for ch in s.chars() {
            match ch {
                '{' => dentro = true,
                '}' => dentro = false,
                _ if !dentro => t.push(ch),
                _ => {}
            }
        }
        t
    };
    sem_marcas.chars().any(|c| c.is_alphabetic())
}

/// Literals that are not texts for people although they look like one.
const NAO_SAO_TEXTO: &[&str] = &[
    "UNC",
    "UTSIM_ENSINO_PERFIL",
    "UTSim Ensino",
    "CARGO_PKG_VERSION",
    "Ok",
    "-V",
    "utsim-ensino {}",
    "{}º",
    "{}º =",
    "Dispatcher",
    "Transporter",
    "Processor",
    "Queue",
    "Conveyor",
    "Source",
    "Sink",
];

#[test]
fn nenhum_texto_fora_do_catalogo() {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut arquivos: Vec<std::path::PathBuf> = std::fs::read_dir(raiz.join("src"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter(|p| p.file_name().is_some_and(|n| n != "texto.rs"))
        .collect();
    arquivos.push(raiz.join("../utsim-ensino-app/src/main.rs"));
    arquivos.push(raiz.join("../utsim-godot/src/ensino_class.rs"));
    arquivos.sort();
    let mut achados = Vec::new();
    for a in &arquivos {
        let src = std::fs::read_to_string(a).unwrap();
        for (linha, s) in literais(&src) {
            if e_texto(&s) && !NAO_SAO_TEXTO.contains(&s.as_str()) {
                let nome = a.file_name().unwrap().to_string_lossy();
                achados.push(format!("{nome}:{linha}: \"{s}\""));
            }
        }
    }
    assert!(
        achados.is_empty(),
        "{} texto(s) fora do catálogo:\n{}",
        achados.len(),
        achados.join("\n")
    );
}
