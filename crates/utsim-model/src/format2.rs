//! Migração do formato 1 para o 2: grava `format: 2` e, se o modelo tem transportadores,
//! `movement: Legacy` em `run` (o comportamento de sempre). No formato 2, `movement` ausente
//! vale `Free`; num modelo sem transportadores o modo não significa nada e fica ausente.
//!
//! O passo reescreve o texto, não um valor RON genérico (ver `Migration`): um pequeno
//! analisador léxico pula comentários, textos e atributos, acha os campos do `Model(...)` e
//! de `run: (...)` e só insere ou troca o necessário; o resto do arquivo fica igual.

use crate::ModelError;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tok {
    Ident,
    Punct(u8),
    /// Text, number or char literal: only its span matters.
    Atom,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Tok,
    start: usize,
    end: usize,
}

fn lex(text: &str) -> Result<Vec<Token>, String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let unterminated = |what: &str| format!("migração do formato 1: {what} sem fim");
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut depth = 1;
            i += 2;
            while depth > 0 {
                if i + 1 >= b.len() {
                    return Err(unterminated("comentário"));
                }
                if b[i] == b'/' && b[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if c == b'"' || c == b'\'' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != c {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            if i >= b.len() {
                return Err(unterminated("texto"));
            }
            i += 1;
            out.push(Token {
                kind: Tok::Atom,
                start,
                end: i,
            });
        } else if (c == b'r' || c == b'b')
            && matches!(b.get(i + 1), Some(b'"' | b'#'))
            && (c == b'r' || b.get(i + 1) == Some(&b'"'))
        {
            // Raw (r"..", r#".."#) or byte string (b"..").
            let start = i;
            i += 1;
            let mut hashes = 0;
            while b.get(i) == Some(&b'#') {
                hashes += 1;
                i += 1;
            }
            if b.get(i) != Some(&b'"') {
                return Err(unterminated("texto"));
            }
            i += 1;
            loop {
                if i >= b.len() {
                    return Err(unterminated("texto"));
                }
                if c == b'b' && b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == b'"'
                    && b[i + 1..]
                        .iter()
                        .take(hashes)
                        .filter(|&&h| h == b'#')
                        .count()
                        == hashes
                {
                    i += 1 + hashes;
                    break;
                }
                i += 1;
            }
            out.push(Token {
                kind: Tok::Atom,
                start,
                end: i,
            });
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Token {
                kind: Tok::Ident,
                start,
                end: i,
            });
        } else if c.is_ascii_digit() || c == b'.' {
            let start = i;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric()
                    || b[i] == b'.'
                    || b[i] == b'_'
                    || (matches!(b[i], b'+' | b'-') && matches!(b[i - 1], b'e' | b'E')))
            {
                i += 1;
            }
            out.push(Token {
                kind: Tok::Atom,
                start,
                end: i,
            });
        } else {
            // Multi-byte UTF-8 outside strings and comments is not valid RON; keep the
            // byte as punctuation and let the parser report it.
            out.push(Token {
                kind: Tok::Punct(c),
                start: i,
                end: i + 1,
            });
            i += 1;
        }
    }
    Ok(out)
}

/// A field `name: value` of a struct: index of the name token and the value's token range.
struct Field {
    name: usize,
    value: (usize, usize),
}

/// Fields of the struct whose `(` is token `open`; returns them and the index of its `)`.
fn fields(toks: &[Token], open: usize) -> (Vec<Field>, usize) {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut k = open + 1;
    let mut expect_field = true;
    let mut current: Option<(usize, usize)> = None;
    while k < toks.len() {
        let t = toks[k];
        match t.kind {
            Tok::Punct(b'(' | b'[' | b'{') => depth += 1,
            Tok::Punct(b')' | b']' | b'}') => {
                if depth == 0 {
                    if let Some((name, from)) = current.take() {
                        out.push(Field {
                            name,
                            value: (from, k),
                        });
                    }
                    return (out, k);
                }
                depth -= 1;
            }
            Tok::Punct(b',') if depth == 0 => {
                if let Some((name, from)) = current.take() {
                    out.push(Field {
                        name,
                        value: (from, k),
                    });
                }
                expect_field = true;
                k += 1;
                continue;
            }
            Tok::Ident
                if depth == 0
                    && expect_field
                    && toks.get(k + 1).is_some_and(|n| n.kind == Tok::Punct(b':')) =>
            {
                current = Some((k, k + 2));
                expect_field = false;
                k += 2;
                continue;
            }
            _ => {}
        }
        expect_field = false;
        k += 1;
    }
    (out, toks.len())
}

fn name<'a>(text: &'a str, toks: &[Token], i: usize) -> &'a str {
    &text[toks[i].start..toks[i].end]
}

/// Passo `(1, v1_to_v2)` de `MIGRATIONS`.
pub(crate) fn v1_to_v2(text: &str) -> Result<String, ModelError> {
    let invalid = |m: String| ModelError::Invalid(vec![m]);
    let toks = lex(text).map_err(invalid)?;
    // Skip inner attributes (`#![enable(...)]`) before the model.
    let mut k = 0;
    while k < toks.len() && toks[k].kind == Tok::Punct(b'#') {
        let mut depth = 0;
        k += 1;
        while k < toks.len() {
            match toks[k].kind {
                Tok::Punct(b'[') => depth += 1,
                Tok::Punct(b']') => {
                    depth -= 1;
                    if depth == 0 {
                        k += 1;
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
    }
    let Some(open) = (k..toks.len()).find(|&i| toks[i].kind == Tok::Punct(b'(')) else {
        // Not a struct: leave it to the parser, which reports the real problem.
        return Ok(text.to_string());
    };
    let (model_fields, _) = fields(&toks, open);
    // (byte position, removed length, inserted text), applied from the end.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    match model_fields
        .iter()
        .find(|f| name(text, &toks, f.name) == "format")
    {
        Some(f) if f.value.0 < f.value.1 => {
            let (a, b) = (toks[f.value.0].start, toks[f.value.1 - 1].end);
            edits.push((a, b - a, "2".to_string()));
        }
        _ => edits.push((toks[open].end, 0, "format: 2, ".to_string())),
    }
    // Without mobile resources the mode means nothing: nothing to keep, so the model
    // stays as a new one would be (and compares equal to it).
    let mobile = toks
        .iter()
        .any(|t| t.kind == Tok::Ident && &text[t.start..t.end] == "Transporter");
    if let Some(run) = model_fields
        .iter()
        .find(|f| mobile && name(text, &toks, f.name) == "run")
    {
        let run_open = (run.value.0..run.value.1).find(|&i| toks[i].kind == Tok::Punct(b'('));
        if let Some(run_open) = run_open {
            let (run_fields, _) = fields(&toks, run_open);
            let has = run_fields
                .iter()
                .any(|f| name(text, &toks, f.name) == "movement");
            if !has {
                edits.push((toks[run_open].end, 0, "movement: Legacy, ".to_string()));
            }
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_string();
    for (at, len, with) in edits {
        out.replace_range(at..at + len, &with);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "(name: \"t\", kind: Transporter(speed: 1.0))";

    #[test]
    fn adds_format_and_legacy_movement() {
        let v1 = format!("Model(name: \"m\", run: (stop_time: 1.0), objects: [{T}])");
        assert_eq!(
            v1_to_v2(&v1).unwrap(),
            format!(
                "Model(format: 2, name: \"m\", run: (movement: Legacy, stop_time: 1.0), objects: [{T}])"
            )
        );
    }

    #[test]
    fn without_transporters_only_the_format_changes() {
        let v1 = "Model(name: \"m\", run: (stop_time: 1.0), objects: [(name: \"Transporter\", kind: Sink)])";
        assert_eq!(
            v1_to_v2(v1).unwrap(),
            "Model(format: 2, name: \"m\", run: (stop_time: 1.0), objects: [(name: \"Transporter\", kind: Sink)])"
        );
    }

    #[test]
    fn replaces_an_explicit_format_and_keeps_comments_and_strings() {
        let v1 = format!(
            "// cabeçalho: run: (\n#![enable(implicit_some)]\nModel(\n    format: Some(1), // formato\n    name: \"run: ( (\",\n    /* run: ( */ run: RunSettings( stop_time: 5.0, ),\n    objects: [{T}],\n)"
        );
        let v2 = v1_to_v2(&v1).unwrap();
        assert!(v2.contains("format: 2, // formato"), "{v2}");
        assert!(v2.contains("name: \"run: ( (\""), "{v2}");
        assert!(
            v2.contains("run: RunSettings(movement: Legacy,  stop_time: 5.0, )"),
            "{v2}"
        );
        assert!(v2.starts_with("// cabeçalho: run: (\n#![enable(implicit_some)]"));
    }

    #[test]
    fn keeps_a_movement_that_is_already_there() {
        let v1 = format!(
            "Model(format: 1, name: \"m\", run: (stop_time: 1.0, movement: Free), objects: [{T}])"
        );
        assert_eq!(
            v1_to_v2(&v1).unwrap(),
            format!(
                "Model(format: 2, name: \"m\", run: (stop_time: 1.0, movement: Free), objects: [{T}])"
            )
        );
    }

    #[test]
    fn nested_fields_named_run_are_not_touched() {
        let v1 = format!(
            "Model(name: \"m\", objects: [(name: \"a\", kind: Sink, run: (x: 1)), {T}], run: (stop_time: 2.0))"
        );
        let v2 = v1_to_v2(&v1).unwrap();
        assert!(v2.contains("run: (x: 1)"), "{v2}");
        assert!(
            v2.contains("run: (movement: Legacy, stop_time: 2.0)"),
            "{v2}"
        );
    }
}
