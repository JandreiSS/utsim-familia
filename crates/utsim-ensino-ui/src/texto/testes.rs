//! The catalogue matches the files of the five languages, and no screen writes a literal
//! text. That every language has the keys and values of pt-BR is the test of
//! `utsim-idiomas` (`idiomas/`).

use super::*;

/// Every entry of the crate: plain ones (no values) and the ones with values.
fn entradas() -> Vec<(&'static str, &'static [&'static str])> {
    let mut v: Vec<(&str, &[&str])> = Vec::new();
    for nomes in [
        cat_abertura::NOMES,
        cat_editor::NOMES,
        geral::NOMES,
        cat_missao::NOMES,
        cat_turma::NOMES,
        cat_vista::NOMES,
    ] {
        v.extend(nomes.iter().map(|n| (*n, &[][..])));
    }
    for modelos in [
        cat_abertura::MODELOS,
        cat_editor::MODELOS,
        geral::MODELOS,
        cat_missao::MODELOS,
        cat_turma::MODELOS,
        cat_vista::MODELOS,
    ] {
        v.extend_from_slice(modelos);
    }
    v
}

fn arquivo(i: Idioma) -> String {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(
        raiz.join("../../idiomas")
            .join(i.codigo())
            .join("ensino.ftl"),
    )
    .unwrap()
}

#[test]
fn catalogo_igual_aos_arquivos() {
    let chaves = utsim_idiomas::conferencia::chaves(&arquivo(Idioma::PtBr)).unwrap();
    let mut erros = Vec::new();
    let mut nomes = std::collections::BTreeSet::new();
    for (nome, valores) in entradas() {
        if !nomes.insert(nome) {
            erros.push(format!("{nome}: nome repetido"));
        }
        match chaves.get(nome) {
            None => erros.push(format!("{nome}: falta em pt-BR/ensino.ftl")),
            Some(args) => {
                let declarados: std::collections::BTreeSet<String> =
                    valores.iter().map(|s| s.to_string()).collect();
                if *args != declarados {
                    erros.push(format!(
                        "{nome}: valores {declarados:?} no código, {args:?} no arquivo"
                    ));
                }
            }
        }
    }
    for k in chaves.keys() {
        if !nomes.contains(k.as_str()) {
            erros.push(format!("{k}: no arquivo, sem uso no código"));
        }
    }
    for i in Idioma::TODOS {
        if arquivo(i).contains('—') {
            erros.push(format!("{}/ensino.ftl com travessão", i.codigo()));
        }
    }
    assert!(erros.is_empty(), "{}", erros.join("\n"));
}

#[test]
fn preenche_pelos_nomes() {
    utsim_idiomas::definir_nesta_thread(Some(Idioma::PtBr));
    let m = ato_titulo();
    assert_eq!(m.nomes.len(), 2);
    assert!(f(m, &[&1, &"Gargalo"]).contains("Gargalo"));
    assert_eq!(idioma_de("en-US"), Idioma::En);
    assert_eq!(idioma_de("fr"), Idioma::Fr);
    assert_eq!(idioma_de("xx"), Idioma::PtBr);
    assert!(TEXTOS.faltas().is_empty());
    utsim_idiomas::definir_nesta_thread(None);
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
    // The window of the app and the Godot bridge also show these texts. They live in other
    // repositories (utsim-ensino, utsim-simulador): checked only when they are next to this
    // crate, as in a workspace with all of them.
    for outro in [
        "../utsim-ensino-app/src/main.rs",
        "../utsim-godot/src/ensino_class.rs",
    ] {
        let p = raiz.join(outro);
        if p.is_file() {
            arquivos.push(p);
        }
    }
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
