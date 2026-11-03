//! Objects made from a template: the presets of a specialised line (`utsim-linhas`).
//!
//! A template is RON text of one or more objects with `${key}` markers. [`fill`] puts
//! the values (RON text too) in the markers, [`parse_object`] reads each object with the
//! same reader as the model, and [`add_command`] turns the result into one undoable
//! [`Command::Batch`] that adds the objects and their connections to a [`Document`].

use std::collections::{BTreeMap, BTreeSet};

use utsim_model::ObjectDef;

use crate::{Command, Document};

/// Objects and connections made from one preset.
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub objects: Vec<ObjectDef>,
    pub connections: Vec<(String, String)>,
}

/// Keys of the `${key}` markers in `template`, in order of first use.
pub fn markers(template: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = template;
    while let Some(at) = rest.find("${") {
        let after = &rest[at + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| format!("marcador sem '}}' em: {}", &rest[at..]))?;
        let key = &after[..end];
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("marcador inválido: ${{{key}}}"));
        }
        if !out.iter().any(|k| k == key) {
            out.push(key.to_string());
        }
        rest = &after[end + 1..];
    }
    Ok(out)
}

/// `template` with every `${key}` replaced by `values[key]`. A marker without a value is
/// an error.
pub fn fill(template: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut text = template.to_string();
    for key in markers(template)? {
        let value = values
            .get(&key)
            .ok_or_else(|| format!("falta o valor de ${{{key}}}"))?;
        text = text.replace(&format!("${{{key}}}"), value);
    }
    Ok(text)
}

/// Reads one object written in RON, as in the `objects` list of a model.
pub fn parse_object(text: &str) -> Result<ObjectDef, String> {
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str::<ObjectDef>(text)
        .map_err(|e| format!("{e}"))
}

/// The commands that add `fragment` to `doc` as one undo step: every object, then every
/// connection. Fails, changing nothing, when a name is already taken or repeated, or a
/// connection names an object that exists neither in the fragment nor in the document.
pub fn add_command(doc: &Document, fragment: &Fragment) -> Result<Command, String> {
    let mut names = BTreeSet::new();
    for o in &fragment.objects {
        if doc.model().objects.iter().any(|x| x.name == o.name) {
            return Err(format!("já existe um objeto chamado '{}'", o.name));
        }
        if !names.insert(o.name.as_str()) {
            return Err(format!("nome repetido na predefinição: '{}'", o.name));
        }
    }
    let known = |n: &str| names.contains(n) || doc.model().objects.iter().any(|x| x.name == n);
    let mut commands: Vec<Command> = fragment
        .objects
        .iter()
        .map(|o| Command::Add(Box::new(o.clone())))
        .collect();
    for (from, to) in &fragment.connections {
        for end in [from, to] {
            if !known(end) {
                return Err(format!("ligação ({from} -> {to}): '{end}' não existe"));
            }
        }
        commands.push(Command::Connect {
            from: from.clone(),
            to: to.clone(),
        });
    }
    Ok(Command::Batch(commands))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn markers_are_found_and_filled() {
        let t = r#"(name: "${name}", kind: Processor(process: ${tempo}), on_entry: "${name}")"#;
        assert_eq!(markers(t).unwrap(), ["name", "tempo"]);
        let text = fill(
            t,
            &values(&[("name", "posto"), ("tempo", "Exp(mean: 3.0)")]),
        )
        .unwrap();
        let o = parse_object(&text).unwrap();
        assert_eq!(o.name, "posto");
        assert_eq!(o.on_entry.as_deref(), Some("posto"));
        assert!(fill(t, &values(&[("name", "x")])).is_err());
        assert!(markers("${a").is_err());
        assert!(markers("${a-b}").is_err());
        // RON maps keep their braces.
        assert!(markers(r#"labels: {"p": Const(1.0)}"#).unwrap().is_empty());
    }

    #[test]
    fn fragment_is_one_undo_step() {
        let mut doc = Document::new("m");
        let fragment = Fragment {
            objects: vec![
                parse_object(r#"(name: "a", kind: Source(interarrival: Const(1.0)))"#).unwrap(),
                parse_object(r#"(name: "b", kind: Sink)"#).unwrap(),
            ],
            connections: vec![("a".into(), "b".into())],
        };
        let cmd = add_command(&doc, &fragment).unwrap();
        doc.apply(cmd).unwrap();
        assert_eq!(doc.model().objects.len(), 2);
        assert_eq!(doc.model().connections.len(), 1);
        assert!(add_command(&doc, &fragment).is_err());
        assert!(doc.undo());
        assert!(doc.model().objects.is_empty());
        let bad = Fragment {
            objects: vec![],
            connections: vec![("a".into(), "z".into())],
        };
        assert!(add_command(&doc, &bad).is_err());
    }
}
