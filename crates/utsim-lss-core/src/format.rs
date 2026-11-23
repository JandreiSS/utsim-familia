//! The versioned project format (`formato: N`), in the style of `docs/versionamento.md`.
//!
//! Every JSON file of a project folder (`projeto.json`, each operation, each snapshot)
//! carries `"formato": N`. Absent means 1. A file newer than [`FORMAT`] is refused with a
//! clear message; an older one goes through [`MIGRATIONS`], in order, before being read.
//! The app always writes the current format. Optional new fields (`#[serde(default)]`) do
//! not need a new format.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::Id;
use crate::journal::Snapshot;
use crate::op::{Op, OpError};

/// Newest format this version reads and the one it writes.
pub const FORMAT: u32 = 1;

/// Name of the format key in every file.
pub const FORMAT_KEY: &str = "formato";

/// One migration step: rewrites a JSON document from one format to the next (and sets the
/// new `formato`). JSON values keep everything, so steps work on [`Value`].
pub type Migration = fn(&mut Value) -> Result<(), FormatError>;

/// Migrations to the current format, as `(source version, step)` in ascending order. To add
/// format N + 1: bump [`FORMAT`], push `(N, vN_to_vN1)` here, keep a file in the old format
/// in the tests and document the step in `docs/versionamento.md`.
pub const MIGRATIONS: &[(u32, Migration)] = &[];

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("{what} no formato {found}; esta versão do UTSim LSS lê até {supported}")]
    TooNew {
        what: &'static str,
        found: u32,
        supported: u32,
    },
    #[error("{what} com JSON inválido: {source}")]
    Json {
        what: &'static str,
        source: serde_json::Error,
    },
    #[error("{what}: campo \"formato\" inválido")]
    BadFormatField { what: &'static str },
    #[error("{0}")]
    Invalid(String),
    #[error("operação inválida: {0}")]
    Op(#[from] OpError),
}

/// The format a document declares (absent: 1).
pub fn declared_format(value: &Value, what: &'static str) -> Result<u32, FormatError> {
    match value.get(FORMAT_KEY) {
        None => Ok(1),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n >= 1)
            .ok_or(FormatError::BadFormatField { what }),
    }
}

/// Applies every step whose source version is `>= from`, in order.
pub fn migrate(
    value: &mut Value,
    from: u32,
    steps: &[(u32, Migration)],
) -> Result<(), FormatError> {
    for (_, step) in steps.iter().filter(|(source, _)| *source >= from) {
        step(value)?;
    }
    Ok(())
}

/// Reads a versioned document: checks the format, migrates and deserializes.
pub fn read<T: DeserializeOwned>(text: &str, what: &'static str) -> Result<T, FormatError> {
    read_with(text, what, FORMAT, MIGRATIONS)
}

fn read_with<T: DeserializeOwned>(
    text: &str,
    what: &'static str,
    supported: u32,
    steps: &[(u32, Migration)],
) -> Result<T, FormatError> {
    let mut value: Value =
        serde_json::from_str(text).map_err(|source| FormatError::Json { what, source })?;
    let found = declared_format(&value, what)?;
    if found > supported {
        return Err(FormatError::TooNew {
            what,
            found,
            supported,
        });
    }
    if found < supported {
        migrate(&mut value, found, steps)?;
    }
    if let Some(obj) = value.as_object_mut() {
        obj.remove(FORMAT_KEY);
    }
    serde_json::from_value(value).map_err(|source| FormatError::Json { what, source })
}

/// Writes a document with `"formato"` first, pretty printed.
pub fn write<T: Serialize>(doc: &T) -> String {
    let mut body = serde_json::to_value(doc).expect("project types serialize to JSON");
    if !body.is_object() {
        body = serde_json::json!({ "valor": body });
    }
    if let Some(obj) = body.as_object_mut() {
        obj.remove(FORMAT_KEY);
    }
    let pretty = serde_json::to_string_pretty(&body).expect("JSON value");
    // The map may sort its keys: put the format first by hand.
    let rest = pretty
        .strip_prefix('{')
        .unwrap_or(&pretty)
        .trim_start_matches('\n');
    let sep = if rest.trim_start().starts_with('}') {
        "\n"
    } else {
        ",\n"
    };
    format!("{{\n  \"{FORMAT_KEY}\": {FORMAT}{sep}{rest}\n")
}

/// Reads an operation file and validates it.
pub fn read_op(text: &str) -> Result<Op, FormatError> {
    let op: Op = read(text, "operação")?;
    op.validate()?;
    Ok(op)
}

pub fn read_snapshot(text: &str) -> Result<Snapshot, FormatError> {
    let mut snap: Snapshot = read(text, "instantâneo")?;
    snap.included.sort();
    snap.included.dedup();
    if snap.included.len() as u64 != snap.state.count {
        return Err(FormatError::Invalid(format!(
            "instantâneo incoerente: {} operações listadas, {} aplicadas",
            snap.included.len(),
            snap.state.count
        )));
    }
    Ok(snap)
}

/// `projeto.json`: the manifest of a project folder. Written once, when the project is
/// created; everything that changes lives in the journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Id of the project entity.
    pub id: Id,
    /// Title at creation (the current one is in the journal).
    #[serde(rename = "titulo")]
    pub title: String,
    #[serde(rename = "criado_em")]
    pub created_ms: i64,
    #[serde(rename = "criado_por")]
    pub created_by: Id,
    /// Program that created it (`utsim-seis-sigma 0.1.0`).
    #[serde(rename = "programa", default)]
    pub program: String,
}

pub fn read_manifest(text: &str) -> Result<Manifest, FormatError> {
    read(text, "projeto")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::{EntityRef, Kind};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Doc {
        nome: String,
    }

    fn rename_v1_to_v2(v: &mut Value) -> Result<(), FormatError> {
        let obj = v
            .as_object_mut()
            .ok_or_else(|| FormatError::Invalid("não é objeto".into()))?;
        let old = obj.remove("titulo").unwrap_or(Value::Null);
        obj.insert("nome".into(), old);
        obj.insert(FORMAT_KEY.into(), Value::from(2));
        Ok(())
    }

    #[test]
    fn write_puts_format_first_and_reads_back() {
        let text = write(&Doc { nome: "x".into() });
        assert!(text.trim_start().starts_with("{\n  \"formato\": 1"));
        assert_eq!(read::<Doc>(&text, "doc").unwrap(), Doc { nome: "x".into() });
        // Absent format means 1.
        assert_eq!(read::<Doc>(r#"{"nome":"y"}"#, "doc").unwrap().nome, "y");
    }

    #[test]
    fn newer_format_is_refused_with_clear_message() {
        let err = read::<Doc>(r#"{"formato": 9, "nome": "y"}"#, "projeto").unwrap_err();
        assert_eq!(
            err.to_string(),
            "projeto no formato 9; esta versão do UTSim LSS lê até 1"
        );
        assert!(matches!(
            read::<Doc>(r#"{"formato": "um"}"#, "projeto"),
            Err(FormatError::BadFormatField { .. })
        ));
        assert!(matches!(
            read::<Doc>(r#"{"formato": 0}"#, "projeto"),
            Err(FormatError::BadFormatField { .. })
        ));
    }

    #[test]
    fn migration_hook_runs_old_steps_only() {
        let steps: &[(u32, Migration)] = &[(1, rename_v1_to_v2)];
        let doc: Doc = read_with(r#"{"formato":1,"titulo":"antigo"}"#, "doc", 2, steps).unwrap();
        assert_eq!(doc.nome, "antigo");
        let doc: Doc = read_with(r#"{"formato":2,"nome":"novo"}"#, "doc", 2, steps).unwrap();
        assert_eq!(doc.nome, "novo");
        assert!(MIGRATIONS.is_empty(), "format 1 has no migrations yet");
    }

    #[test]
    fn op_files_validate() {
        let op = Op {
            id: Id::v7(1, [1; 10]),
            author: Id::v7(1, [2; 10]),
            at_ms: 0,
            lamport: 1,
            entity: EntityRef::new(Kind::Project, Id::v7(1, [3; 10])),
            field: "titulo".into(),
            value: Value::from("t"),
            base: None,
        };
        let text = write(&op);
        assert_eq!(read_op(&text).unwrap(), op);
        let broken = text.replace("\"relogio_lamport\": 1", "\"relogio_lamport\": 0");
        assert!(matches!(read_op(&broken), Err(FormatError::Op(_))));
        assert!(matches!(
            read_op(&text[..text.len() / 2]),
            Err(FormatError::Json { .. })
        ));
    }
}
