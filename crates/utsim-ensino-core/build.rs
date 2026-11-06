//! Embeds the campaign package (`ensino/campanha`) in the library: every file, by path.

use std::path::{Path, PathBuf};

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("ensino/campanha")
        .map(|e| e.expect("entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, base, out);
        } else {
            let rel = p
                .strip_prefix(base)
                .expect("inside")
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            out.push((rel, p));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let dir = manifest.join("../../ensino/campanha");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files = Vec::new();
    walk(&dir, &dir, &mut files);
    let mut code = String::from("pub const ARQUIVOS: &[(&str, &[u8])] = &[\n");
    for (rel, p) in &files {
        println!("cargo:rerun-if-changed={}", p.display());
        let abs = std::fs::canonicalize(p).expect("canonical");
        code.push_str(&format!("    ({rel:?}, include_bytes!({:?})),\n", abs));
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("campanha.rs");
    std::fs::write(out, code).expect("write");
}
