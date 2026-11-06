//! The pack of a line: `biblioteca.ron` (presets and ready models) and `vocabulario.ron`
//! (the words of the domain), read and checked here.
//!
//! ```ron
//! Library(
//!     line: "lean_office",
//!     name: "Lean Office",
//!     description: "...",
//!     models: [(file: "examples/aprovacao_de_compras.ron", name: "Aprovação de compras")],
//!     presets: [
//!         (
//!             id: "posto",
//!             name: "Posto de trabalho",
//!             icon: "mesa",
//!             role: Work,
//!             name_base: "posto",
//!             fields: [(key: "tempo", label: "Tempo de trabalho", default: "Exp(mean: 30.0)")],
//!             objects: [r#"(name: "${name}", kind: Processor(process: ${tempo}))"#],
//!         ),
//!     ],
//! )
//! ```
//!
//! Each preset is the RON text of one or more ordinary objects with `${key}` markers:
//! `${name}` is the name chosen for the object, the others are its `fields`. Placing a
//! preset creates ordinary objects (the model stays readable without the line), tagged
//! with `preset: "<line>/<id>"`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use utsim_edit::Document;
use utsim_edit::preset::{Fragment, add_command, fill, markers, parse_object};

/// Library file name inside a pack directory.
pub const LIBRARY_FILE: &str = "biblioteca.ron";
/// Vocabulary file name inside a pack directory.
pub const VOCABULARY_FILE: &str = "vocabulario.ron";

/// What a preset is in the domain: the line's reports read it (waits by cause, work time,
/// people).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Role {
    /// Where cases enter (a source).
    Entry,
    /// Inbox: cases wait for the next step.
    Inbox,
    /// Work done on the case.
    Work,
    /// Approval, possibly sending the case back.
    Approval,
    /// Check that finds errors of earlier steps.
    Check,
    /// Handover between departments (mail, e-mail, system): elapsed time, no work.
    Transit,
    /// Waiting for information from outside.
    WaitInfo,
    /// Grouping in batches.
    Batch,
    /// Meeting or committee on a schedule.
    Meeting,
    /// Team of people.
    Team,
    /// A person.
    Person,
    /// Where cases end.
    Done,
}

impl Role {
    pub const ALL: [Role; 12] = [
        Role::Entry,
        Role::Inbox,
        Role::Work,
        Role::Approval,
        Role::Check,
        Role::Transit,
        Role::WaitInfo,
        Role::Batch,
        Role::Meeting,
        Role::Team,
        Role::Person,
        Role::Done,
    ];

    /// Portuguese name.
    pub fn nome(self) -> &'static str {
        match self {
            Role::Entry => "entrada",
            Role::Inbox => "caixa de entrada",
            Role::Work => "trabalho",
            Role::Approval => "aprovação",
            Role::Check => "verificação",
            Role::Transit => "tramitação",
            Role::WaitInfo => "espera por informação",
            Role::Batch => "lote",
            Role::Meeting => "reunião",
            Role::Team => "equipe",
            Role::Person => "pessoa",
            Role::Done => "conclusão",
        }
    }

    /// Whether time spent in an object of this role is work on the case (process time).
    pub fn is_work(self) -> bool {
        matches!(
            self,
            Role::Work | Role::Approval | Role::Check | Role::Meeting
        )
    }
}

/// One field shown when the preset is placed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    /// Marker key (`${key}` in the templates).
    pub key: String,
    /// Portuguese label.
    pub label: String,
    /// Default value, as RON text.
    pub default: String,
    /// Portuguese help text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
}

/// One entry of the line's library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Identifier inside the line (`aprovacao`); the objects get `preset: "<line>/<id>"`.
    pub id: String,
    /// Name shown (Portuguese).
    pub name: String,
    /// Icon name (2D plan and library).
    pub icon: String,
    pub role: Role,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Base of the names of new objects.
    pub name_base: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    /// RON text of each object; the first is the main one and is called `${name}`.
    pub objects: Vec<String>,
    /// Connections between the objects of the preset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<(String, String)>,
}

/// A ready model of the line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelRef {
    /// Path relative to the repository (or installation) root.
    pub file: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// `biblioteca.ron`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename = "Library")]
pub struct Library {
    /// Line identifier (`lean_office`), the value of `Model.line`.
    pub line: String,
    /// Name shown (`Lean Office`).
    pub name: String,
    /// Shown after the name (`processos administrativos`); may be empty.
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub models: Vec<ModelRef>,
    pub presets: Vec<Preset>,
}

impl Library {
    /// The name with the subtitle: `Lean Office (processos administrativos)`.
    pub fn title(&self) -> String {
        if self.subtitle.trim().is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.subtitle.trim())
        }
    }
}

/// `vocabulario.ron`: core term → domain term.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "Vocabulary")]
pub struct Vocabulary {
    pub line: String,
    pub terms: BTreeMap<String, String>,
}

impl Vocabulary {
    /// The domain word for `key`, or `key` itself when the line has none.
    pub fn term<'a>(&'a self, key: &'a str) -> &'a str {
        self.terms.get(key).map_or(key, String::as_str)
    }
}

/// A line: library and vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    pub library: Library,
    pub vocabulary: Vocabulary,
}

fn ron_options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl Pack {
    /// Reads a pack from the text of its two files and checks it.
    pub fn parse(library: &str, vocabulary: &str) -> Result<Pack, Vec<String>> {
        let library: Library = ron_options()
            .from_str(library)
            .map_err(|e| vec![format!("{LIBRARY_FILE}: {e}")])?;
        let vocabulary: Vocabulary = ron_options()
            .from_str(vocabulary)
            .map_err(|e| vec![format!("{VOCABULARY_FILE}: {e}")])?;
        let pack = Pack {
            library,
            vocabulary,
        };
        let errors = pack.validate();
        if errors.is_empty() {
            Ok(pack)
        } else {
            Err(errors)
        }
    }

    /// Reads the pack in `dir` (`biblioteca.ron` and `vocabulario.ron`).
    pub fn load_dir(dir: &Path) -> Result<Pack, Vec<String>> {
        let read = |f: &str| {
            std::fs::read_to_string(dir.join(f))
                .map_err(|e| vec![format!("{}: {e}", dir.join(f).display())])
        };
        Pack::parse(&read(LIBRARY_FILE)?, &read(VOCABULARY_FILE)?)
    }

    pub fn id(&self) -> &str {
        &self.library.line
    }

    pub fn preset(&self, id: &str) -> Option<&Preset> {
        self.library.presets.iter().find(|p| p.id == id)
    }

    /// The preset an object was made from, when it belongs to this line.
    pub fn preset_of(&self, obj: &utsim_model::ObjectDef) -> Option<&Preset> {
        let (line, id) = obj.preset.as_deref()?.split_once('/')?;
        (line == self.id()).then(|| self.preset(id)).flatten()
    }

    /// Every problem of the pack (empty: valid).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let lib = &self.library;
        if !valid_id(&lib.line) {
            errors.push(format!(
                "{LIBRARY_FILE}: line '{}' deve ter só letras minúsculas, dígitos e '_'",
                lib.line
            ));
        }
        if lib.name.trim().is_empty() {
            errors.push(format!("{LIBRARY_FILE}: name vazio"));
        }
        if self.vocabulary.line != lib.line {
            errors.push(format!(
                "{VOCABULARY_FILE}: line '{}' diferente da biblioteca ('{}')",
                self.vocabulary.line, lib.line
            ));
        }
        for (k, v) in &self.vocabulary.terms {
            if k.trim().is_empty() || v.trim().is_empty() {
                errors.push(format!("{VOCABULARY_FILE}: termo vazio ('{k}' -> '{v}')"));
            }
        }
        let mut ids = BTreeSet::new();
        for p in &lib.presets {
            if !valid_id(&p.id) {
                errors.push(format!(
                    "predefinição '{}': id deve ter só letras minúsculas, dígitos e '_'",
                    p.id
                ));
            }
            if !ids.insert(p.id.as_str()) {
                errors.push(format!("predefinição '{}' repetida", p.id));
            }
            errors.extend(
                validate_preset(&lib.line, p)
                    .into_iter()
                    .map(|e| format!("predefinição '{}': {e}", p.id)),
            );
        }
        errors
    }
}

/// Problems of one preset: markers and fields, and the objects made with the defaults.
fn validate_preset(line: &str, p: &Preset) -> Vec<String> {
    let mut errors = Vec::new();
    if p.name.trim().is_empty() || p.icon.trim().is_empty() {
        errors.push("name e icon não podem ser vazios".to_string());
    }
    if !valid_id(&p.name_base) {
        errors.push(format!(
            "name_base '{}' deve ter só letras minúsculas, dígitos e '_'",
            p.name_base
        ));
    }
    if p.objects.is_empty() {
        errors.push("objects está vazio".to_string());
    }
    let mut keys = BTreeSet::new();
    for f in &p.fields {
        if f.key == "name" {
            errors.push("o campo 'name' é reservado ao nome do objeto".to_string());
        }
        if !keys.insert(f.key.as_str()) {
            errors.push(format!("campo '{}' repetido", f.key));
        }
        if f.label.trim().is_empty() {
            errors.push(format!("campo '{}' sem label", f.key));
        }
    }
    let texts = p
        .objects
        .iter()
        .chain(p.connections.iter().flat_map(|(a, b)| [a, b]));
    let mut used = BTreeSet::new();
    for t in texts {
        match markers(t) {
            Ok(ks) => used.extend(ks),
            Err(e) => errors.push(e),
        }
    }
    for k in &used {
        if k != "name" && !keys.contains(k.as_str()) {
            errors.push(format!("marcador ${{{k}}} sem campo"));
        }
    }
    for k in &keys {
        if !used.contains(*k) {
            errors.push(format!("campo '{k}' não é usado em nenhum objeto"));
        }
    }
    if !errors.is_empty() {
        return errors;
    }
    match p.instantiate(line, &p.name_base, &BTreeMap::new(), [0.0; 3], None) {
        Ok(f) => {
            if f.objects.first().map(|o| o.name.as_str()) != Some(p.name_base.as_str()) {
                errors.push("o primeiro objeto deve se chamar \"${name}\"".to_string());
            }
            for o in f.objects.iter().skip(1) {
                if !o.name.starts_with(&p.name_base) {
                    errors.push(format!(
                        "o objeto '{}' deve ter o nome começando por \"${{name}}\"",
                        o.name
                    ));
                }
            }
            for (a, b) in &f.connections {
                for end in [a, b] {
                    if !f.objects.iter().any(|o| o.name == *end) {
                        errors.push(format!(
                            "ligação ({a} -> {b}): '{end}' fora da predefinição"
                        ));
                    }
                }
            }
        }
        Err(e) => errors.push(e),
    }
    errors
}

impl Preset {
    /// Values of the fields: the defaults, replaced by `values`, plus `name`. An unknown
    /// key is an error.
    pub fn values(
        &self,
        name: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut out: BTreeMap<String, String> = self
            .fields
            .iter()
            .map(|f| (f.key.clone(), f.default.clone()))
            .collect();
        for (k, v) in values {
            if !out.contains_key(k) {
                let known: Vec<&str> = self.fields.iter().map(|f| f.key.as_str()).collect();
                return Err(format!(
                    "'{}' não tem o campo '{k}' (campos: {})",
                    self.id,
                    if known.is_empty() {
                        "nenhum".to_string()
                    } else {
                        known.join(", ")
                    }
                ));
            }
            out.insert(k.clone(), v.clone());
        }
        out.insert("name".to_string(), name.to_string());
        Ok(out)
    }

    /// The objects of this preset called `name`, moved by `position` (the templates'
    /// positions are relative), tagged `preset: "<line>/<id>"` (unless the template sets
    /// another) and, with `department`, in that department.
    pub fn instantiate(
        &self,
        line: &str,
        name: &str,
        values: &BTreeMap<String, String>,
        position: [f64; 3],
        department: Option<&str>,
    ) -> Result<Fragment, String> {
        let v = self.values(name, values)?;
        let mut objects = Vec::new();
        for t in &self.objects {
            let text = fill(t, &v)?;
            let mut o = parse_object(&text).map_err(|e| format!("objeto inválido: {e}"))?;
            for (p, d) in o.position.iter_mut().zip(position) {
                *p += d;
            }
            if o.preset.is_none() {
                o.preset = Some(format!("{line}/{}", self.id));
            }
            if let Some(a) = department {
                o.department = Some(a.to_string());
            }
            objects.push(o);
        }
        let connections = self
            .connections
            .iter()
            .map(|(a, b)| Ok((fill(a, &v)?, fill(b, &v)?)))
            .collect::<Result<_, String>>()?;
        Ok(Fragment {
            objects,
            connections,
        })
    }
}

/// What to place: preset, name, field values, position and department.
pub struct Placement<'a> {
    pub preset: &'a str,
    /// Absent: a free name from the preset's `name_base`.
    pub name: Option<&'a str>,
    pub values: BTreeMap<String, String>,
    pub position: [f64; 3],
    pub department: Option<&'a str>,
}

impl Pack {
    /// Adds a preset to `doc` as one undo step; returns the names of the new objects.
    pub fn apply(&self, doc: &mut Document, at: &Placement) -> Result<Vec<String>, String> {
        let p = self.preset(at.preset).ok_or_else(|| {
            format!(
                "a linha '{}' não tem a predefinição '{}'",
                self.id(),
                at.preset
            )
        })?;
        let name = match at.name {
            Some(n) => n.to_string(),
            None => doc.unique_name(&p.name_base),
        };
        let fragment = p.instantiate(self.id(), &name, &at.values, at.position, at.department)?;
        let names = fragment.objects.iter().map(|o| o.name.clone()).collect();
        let command = add_command(doc, &fragment)?;
        doc.apply(command)?;
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIB: &str = r##"Library(
        line: "teste",
        name: "Teste",
        presets: [
            (id: "chegada", name: "Chegada", icon: "bandeja", role: Entry, name_base: "chegada",
             fields: [(key: "intervalo", label: "Intervalo", default: "Exp(mean: 2.0)")],
             objects: [r#"(name: "${name}", kind: Source(interarrival: ${intervalo}))"#]),
            (id: "posto", name: "Posto", icon: "mesa", role: Work, name_base: "posto",
             fields: [(key: "tempo", label: "Tempo", default: "Exp(mean: 1.0)")],
             objects: [
                r#"(name: "${name}", kind: Processor(process: ${tempo}))"#,
                r#"(name: "${name}_caixa", kind: Queue(), position: (-1.0, 0.0, 0.0))"#,
             ],
             connections: [("${name}_caixa", "${name}")]),
            (id: "fim", name: "Fim", icon: "pasta", role: Done, name_base: "fim",
             objects: [r#"(name: "${name}", kind: Sink)"#]),
        ],
    )"##;

    /// The "posto" preset without its first object and without the field in use.
    const BAD: &str = r##"Library(
        line: "teste",
        name: "Teste",
        presets: [
            (id: "posto", name: "Posto", icon: "mesa", role: Work, name_base: "posto",
             fields: [(key: "tempo", label: "Tempo", default: "Exp(mean: 1.0)")],
             objects: [r#"(name: "${name}_caixa", kind: Queue())"#]),
            (id: "fim", name: "Fim", icon: "pasta", role: Done, name_base: "fim",
             objects: [r#"(name: "x${name}", kind: Sink)"#]),
        ],
    )"##;

    const VOC: &str = r#"Vocabulary(line: "teste", terms: {"item": "caso"})"#;

    #[test]
    fn bad_packs_are_reported() {
        let err = Pack::parse(BAD, VOC).unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.contains("'posto'") && e.contains("campo 'tempo'")),
            "{err:?}"
        );
        assert!(
            err.iter()
                .any(|e| e.contains("'fim'") && e.contains("primeiro objeto")),
            "{err:?}"
        );
        let other = VOC.replace("\"teste\"", "\"outra\"");
        let err = Pack::parse(LIB, &other).unwrap_err();
        assert_eq!(err.len(), 1, "{err:?}");
        assert!(err[0].starts_with(VOCABULARY_FILE));
        assert!(Pack::parse("Library(", VOC).is_err());
    }

    #[test]
    fn presets_build_a_model_that_runs() {
        let pack = Pack::parse(LIB, VOC).unwrap();
        assert_eq!(pack.vocabulary.term("item"), "caso");
        assert_eq!(pack.vocabulary.term("Sink"), "Sink");
        let mut doc = Document::new("m");
        let place = |preset, values: &[(&str, &str)], x| Placement {
            preset,
            name: None,
            values: values
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            position: [x, 0.0, 0.0],
            department: Some("Compras"),
        };
        let a = pack.apply(&mut doc, &place("chegada", &[], 0.0)).unwrap();
        let b = pack
            .apply(&mut doc, &place("posto", &[("tempo", "Const(0.5)")], 5.0))
            .unwrap();
        assert_eq!(b, ["posto", "posto_caixa"]);
        let c = pack.apply(&mut doc, &place("fim", &[], 9.0)).unwrap();
        for (from, to) in [(&a[0], &b[1]), (&b[0], &c[0])] {
            doc.apply(utsim_edit::Command::Connect {
                from: from.clone(),
                to: to.clone(),
            })
            .unwrap();
        }
        let m = doc.model();
        assert_eq!(m.objects[2].position, [4.0, 0.0, 0.0]);
        assert_eq!(m.objects[1].preset.as_deref(), Some("teste/posto"));
        assert_eq!(m.objects[1].department.as_deref(), Some("Compras"));
        assert_eq!(pack.preset_of(&m.objects[1]).unwrap().role, Role::Work);
        m.validate().unwrap();
        assert!(m.run_replications().is_ok());
        // Saved and read back without loss.
        let again = utsim_model::Model::parse(&doc.to_text()).unwrap();
        assert_eq!(&again, m);
        // A second "posto" gets a free name; an unknown field or preset is refused.
        let names = pack.apply(&mut doc, &place("posto", &[], 5.0)).unwrap();
        assert_eq!(names[0], "posto_2");
        assert!(
            pack.apply(&mut doc, &place("posto", &[("x", "1")], 5.0))
                .is_err()
        );
        assert!(pack.apply(&mut doc, &place("nada", &[], 5.0)).is_err());
    }
}
