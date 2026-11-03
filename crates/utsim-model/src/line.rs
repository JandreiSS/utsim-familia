//! Fields that tie a model to a specialised line (`docs/plano_linhas_especializadas.md`):
//! `Model.line`, and per object `preset`, `area` and `symbol`. The simulation never reads
//! them; they are checked here so a typo is caught when the model is loaded.

use crate::Model;

/// Identifiers accepted by `ObjectDef.symbol`: the symbols of the process flowchart
/// (`utsim_diagram::Symbol::id`).
pub const SYMBOL_IDS: [&str; 7] = [
    "inicio",
    "fim",
    "operacao",
    "transporte",
    "inspecao",
    "espera",
    "armazenagem",
];

/// A line or preset identifier: lowercase ASCII letters, digits and `_`, with one `/`
/// between the line and the preset (`lean_office/aprovacao`).
fn valid_id(id: &str, slash: bool) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    match id.split_once('/') {
        Some((line, preset)) if slash => part(line) && part(preset),
        Some(_) => false,
        None => !slash && part(id),
    }
}

impl Model {
    pub(crate) fn validate_line(&self, errors: &mut Vec<String>) {
        if let Some(line) = &self.line {
            if !valid_id(line, false) {
                errors.push(format!(
                    "line '{line}': use letras minúsculas, dígitos e '_' (por exemplo \"lean_office\")"
                ));
            }
        }
        for obj in &self.objects {
            if let Some(p) = &obj.preset {
                if !valid_id(p, true) {
                    errors.push(format!(
                        "{}: preset '{p}' deve ser \"linha/predefinição\" (por exemplo \"lean_office/aprovacao\")",
                        obj.name
                    ));
                }
            }
            if obj.area.as_ref().is_some_and(|a| a.trim().is_empty()) {
                errors.push(format!("{}: area não pode ser vazia", obj.name));
            }
            if let Some(s) = &obj.symbol {
                if !SYMBOL_IDS.contains(&s.as_str()) {
                    errors.push(format!(
                        "{}: symbol '{s}' desconhecido (use {})",
                        obj.name,
                        SYMBOL_IDS.join(", ")
                    ));
                }
            }
        }
    }

    /// Line the preset of `obj` belongs to (`"lean_office"` for `"lean_office/aprovacao"`).
    pub fn preset_line(obj: &crate::ObjectDef) -> Option<&str> {
        obj.preset.as_deref().and_then(|p| p.split_once('/')).map(|(l, _)| l)
    }
}

#[cfg(test)]
mod tests {
    use crate::Model;

    const BASE: &str = r#"Model(name: "m", run: (stop_time: 10.0), objects: [
        (name: "a", kind: Source(interarrival: Const(1.0)) OBJ),
        (name: "b", kind: Sink),
    ], connections: [("a", "b")] MODEL)"#;

    fn model(obj: &str, model: &str) -> Model {
        Model::parse(&BASE.replace("OBJ", obj).replace("MODEL", model)).unwrap()
    }

    #[test]
    fn fields_are_optional_and_round_trip() {
        let plain = model("", "");
        assert_eq!(plain.line, None);
        assert_eq!(plain.objects[0].preset, None);
        plain.validate().unwrap();
        let tagged = model(
            r#", preset: "lean_office/entrada", area: "Compras", symbol: "inicio""#,
            r#", line: "lean_office""#,
        );
        tagged.validate().unwrap();
        assert_eq!(tagged.line.as_deref(), Some("lean_office"));
        assert_eq!(Model::preset_line(&tagged.objects[0]), Some("lean_office"));
        let config = ron::ser::PrettyConfig::new()
            .struct_names(true)
            .extensions(ron::extensions::Extensions::IMPLICIT_SOME);
        let text = ron::ser::to_string_pretty(&tagged, config.clone()).unwrap();
        assert_eq!(Model::parse(&text).unwrap(), tagged);
        // Absent fields are not written.
        let text = ron::ser::to_string_pretty(&plain, config).unwrap();
        for key in ["line:", "preset:", "area:", "symbol:"] {
            assert!(!text.contains(key), "{key}");
        }
    }

    #[test]
    fn bad_values_are_reported() {
        let m = model(
            r#", preset: "aprovacao", area: " ", symbol: "losango""#,
            r#", line: "Lean Office""#,
        );
        let err = m.validate().unwrap_err().to_string();
        assert!(err.contains("line 'Lean Office'"), "{err}");
        assert!(err.contains("preset 'aprovacao'"), "{err}");
        assert!(err.contains("area não pode ser vazia"), "{err}");
        assert!(err.contains("symbol 'losango'"), "{err}");
    }
}
