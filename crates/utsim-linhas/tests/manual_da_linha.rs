//! The manual chapters of the Lean Office line (`docs/manual/src/linhas/lean-office/`):
//! written with the line's vocabulary (no general UTSim term left in the prose, outside
//! code), every link leads to a file and a heading that exist, and every chapter is in the
//! table of contents.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

const CHAPTERS: &str = "docs/manual/src/linhas/lean-office";

/// General UTSim terms the line replaces: the plain-word keys of the vocabulary (item,
/// operador...) and the Portuguese names of the core objects it renames.
fn general_terms() -> BTreeSet<String> {
    let text = std::fs::read_to_string(repo("linhas/lean_office/vocabulario.ron")).unwrap();
    let mut terms: BTreeSet<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix('"'))
        .filter_map(|l| l.split_once('"').map(|(k, _)| k.to_string()))
        .filter(|k| k.chars().all(|c| c.is_lowercase()))
        .collect();
    for t in [
        "processador",
        "processadores",
        "transportador",
        "transportadores",
        "despachante",
        "despachantes",
        "fila",
        "filas",
        "fonte",
        "fontes",
    ] {
        terms.insert(t.to_string());
    }
    terms
}

/// The prose of a chapter: without fenced blocks, inline code and link targets.
fn prose(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (n, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut clean = String::new();
        let mut code = false;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '`' {
                code = !code;
                continue;
            }
            if code {
                continue;
            }
            if c == ']' && chars.peek() == Some(&'(') {
                // Skip the link target.
                for d in chars.by_ref() {
                    if d == ')' {
                        break;
                    }
                }
                continue;
            }
            clean.push(c);
        }
        out.push((n + 1, clean));
    }
    out
}

fn chapters() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(repo(CHAPTERS))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();
    files
}

#[test]
fn chapters_use_the_line_vocabulary() {
    let terms = general_terms();
    assert!(
        terms.contains("item") && terms.contains("operador"),
        "{terms:?}"
    );
    let mut found = Vec::new();
    for file in chapters() {
        let text = std::fs::read_to_string(&file).unwrap();
        for (n, line) in prose(&text) {
            for word in line
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
            {
                if terms.contains(&word.to_lowercase()) {
                    found.push(format!("{}:{n}: {word}", file.display()));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "termos gerais no texto da linha:\n{}",
        found.join("\n")
    );
}

/// The anchor mdBook gives a heading.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn headings(file: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(file).unwrap();
    let mut fenced = false;
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced && line.starts_with('#') {
            out.insert(slug(line.trim_start_matches('#')));
        }
    }
    out
}

/// Every `[text](target)` of `file` outside code blocks.
fn links(file: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(file).unwrap();
    let mut out = Vec::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("](") {
            let after = &rest[at + 2..];
            let Some(end) = after.find(')') else { break };
            out.push(after[..end].to_string());
            rest = &after[end..];
        }
    }
    out
}

#[test]
fn links_lead_to_files_and_headings_that_exist() {
    let mut files = chapters();
    files.push(repo("docs/manual/src/SUMMARY.md"));
    files.push(repo("docs/manual/src/referencia-ron.md"));
    files.push(repo("docs/manual/src/referencia-cli.md"));
    let mut broken = Vec::new();
    for file in &files {
        let from_line = file.starts_with(repo(CHAPTERS));
        for target in links(file) {
            if target.starts_with("http") {
                continue;
            }
            let (path, anchor) = match target.split_once('#') {
                Some((p, a)) => (p, Some(a)),
                None => (target.as_str(), None),
            };
            // Outside the line's chapters, only the links into them are checked.
            if !from_line && !path.contains("lean-office") {
                continue;
            }
            let dest = if path.is_empty() {
                file.clone()
            } else {
                file.parent().unwrap().join(path)
            };
            if !dest.exists() {
                broken.push(format!("{}: {target} (arquivo)", file.display()));
            } else if let Some(a) = anchor {
                if !headings(&dest).contains(a) {
                    broken.push(format!("{}: {target} (seção)", file.display()));
                }
            }
        }
    }
    assert!(broken.is_empty(), "links quebrados:\n{}", broken.join("\n"));
}

#[test]
fn every_chapter_is_in_the_table_of_contents() {
    let summary = std::fs::read_to_string(repo("docs/manual/src/SUMMARY.md")).unwrap();
    for file in chapters() {
        let name = file.file_name().unwrap().to_string_lossy();
        let entry = format!("(linhas/lean-office/{name})");
        assert!(summary.contains(&entry), "{entry} fora do SUMMARY.md");
    }
}
