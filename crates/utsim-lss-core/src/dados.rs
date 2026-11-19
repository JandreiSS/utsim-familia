//! Data sets of a project (`docs/plano_lean_seis_sigma.md`, 3.2 and 4.4): the content is a
//! CSV file in the project's `dados/` folder, named by its hash; the journal keeps the
//! description, the hash and the **origin**: `Real` (collected), `Gêmeo` (the digital
//! twin's recordings) or `Simulado` (replications of a model, with the model's hash, the
//! seed and the replications). Every analysis shows the origin, and a simulated data set
//! never counts as a real baseline.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::domain::enum_value;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};

pub mod fields {
    pub const PROJECT: &str = "projeto";
    pub const NAME: &str = "nome";
    pub const ORIGIN: &str = "origem";
    /// File name inside `dados/` (`<sha256>.csv`).
    pub const FILE: &str = "arquivo";
    pub const HASH: &str = "hash";
    pub const COLUMNS: &str = "colunas";
    pub const ROWS: &str = "linhas";
    pub const MODEL_HASH: &str = "modelo_hash";
    pub const MODEL_NAME: &str = "modelo";
    pub const SCENARIO: &str = "cenario";
    pub const REPLICATIONS: &str = "replicacoes";
    pub const SEED: &str = "semente";
    pub const CREATED_ON: &str = "criado_em";
    pub const NOTE: &str = "observacao";
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum DataOrigin {
    #[default]
    #[serde(rename = "real")]
    Real,
    #[serde(rename = "gemeo")]
    Twin,
    #[serde(rename = "simulado")]
    Simulated,
}

impl DataOrigin {
    pub const ALL: [DataOrigin; 3] = [DataOrigin::Real, DataOrigin::Twin, DataOrigin::Simulated];

    pub fn label(self) -> &'static str {
        match self {
            DataOrigin::Real => "Real",
            DataOrigin::Twin => "Gêmeo",
            DataOrigin::Simulated => "Simulado",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dataset {
    pub id: Id,
    pub name: String,
    pub origin: DataOrigin,
    pub file: String,
    pub hash: String,
    pub columns: Vec<String>,
    pub rows: usize,
    pub model_hash: String,
    pub model_name: String,
    pub scenario: String,
    pub replications: Option<u32>,
    pub seed: Option<u64>,
    pub created_on: Option<Date>,
    pub note: String,
}

impl Dataset {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            name: e.text(fields::NAME),
            origin: e.typed(fields::ORIGIN).unwrap_or_default(),
            file: e.text(fields::FILE),
            hash: e.text(fields::HASH),
            columns: e
                .get(fields::COLUMNS)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            rows: e.integer(fields::ROWS).map_or(0, |n| n.max(0) as usize),
            model_hash: e.text(fields::MODEL_HASH),
            model_name: e.text(fields::MODEL_NAME),
            scenario: e.text(fields::SCENARIO),
            replications: e.integer(fields::REPLICATIONS).map(|n| n.max(0) as u32),
            seed: e
                .get(fields::SEED)
                .and_then(|v| v.as_str().and_then(|s| s.parse().ok()).or(v.as_u64())),
            created_on: e.opt_text(fields::CREATED_ON).and_then(|s| s.parse().ok()),
            note: e.text(fields::NOTE),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Dataset, id)
    }

    /// "Nome (Simulado)": the origin always goes with the name.
    pub fn labeled(&self) -> String {
        format!("{} ({})", self.name, self.origin.label())
    }
}

/// The edit that deletes a data set (an ordinary operation: another can restore it). The
/// screen deletes it only when no analysis uses it ([`in_use`]).
pub fn delete(id: Id) -> Vec<Edit> {
    vec![Edit::new(
        Dataset::entity_ref(id),
        crate::op::FIELD_DELETED,
        true,
    )]
}

/// Whether a saved analysis uses the data set.
pub fn in_use(analyses: &[crate::analises::Analysis], id: Id) -> bool {
    analyses.iter().any(|a| a.dataset == Some(id))
}

pub fn datasets(state: &State, project: Id) -> Vec<Dataset> {
    let key = project.to_string();
    let mut v: Vec<Dataset> = state
        .of_kind(Kind::Dataset)
        .filter(|(_, e)| !e.is_deleted() && e.text(fields::PROJECT) == key)
        .map(|(id, e)| Dataset::read(id, e))
        .collect();
    v.sort_by_key(|d| (d.created_on, d.id));
    v
}

/// What a new data set records (the file is written by the store first).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DatasetInput {
    pub name: String,
    pub origin: DataOrigin,
    pub file: String,
    pub hash: String,
    pub columns: Vec<String>,
    pub rows: usize,
    pub model_hash: String,
    pub model_name: String,
    pub scenario: String,
    pub replications: Option<u32>,
    pub seed: Option<u64>,
    pub date: Option<Date>,
    pub note: String,
}

pub fn register(project: Id, id: Id, input: &DatasetInput) -> Vec<Edit> {
    let r = Dataset::entity_ref(id);
    let mut e = vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::NAME, input.name.trim()),
        Edit::new(r, fields::ORIGIN, enum_value(input.origin)),
        Edit::new(r, fields::FILE, input.file.as_str()),
        Edit::new(r, fields::HASH, input.hash.as_str()),
        Edit::new(
            r,
            fields::COLUMNS,
            Value::from(
                input
                    .columns
                    .iter()
                    .map(|c| Value::from(c.as_str()))
                    .collect::<Vec<_>>(),
            ),
        ),
        Edit::new(r, fields::ROWS, input.rows as u64),
        Edit::new(r, fields::NOTE, input.note.trim()),
    ];
    if !input.model_hash.is_empty() {
        e.push(Edit::new(r, fields::MODEL_HASH, input.model_hash.as_str()));
        e.push(Edit::new(r, fields::MODEL_NAME, input.model_name.as_str()));
    }
    if !input.scenario.is_empty() {
        e.push(Edit::new(r, fields::SCENARIO, input.scenario.as_str()));
    }
    if let Some(n) = input.replications {
        e.push(Edit::new(r, fields::REPLICATIONS, n));
    }
    if let Some(s) = input.seed {
        e.push(Edit::new(r, fields::SEED, s.to_string()));
    }
    if let Some(d) = input.date {
        e.push(Edit::new(r, fields::CREATED_ON, d.to_string()));
    }
    e
}

/// The header and the number of data rows of a CSV text.
pub fn shape(text: &str) -> (Vec<String>, usize) {
    let t = utsim_quality::data::parse(text);
    let header = t.header.clone().unwrap_or_else(|| {
        (1..=t.rows.first().map_or(0, Vec::len))
            .map(|i| format!("coluna {i}"))
            .collect()
    });
    (header, t.rows.len())
}

/// The numeric values of a column (cells that are not numbers are left out).
pub fn column_values(text: &str, column: usize) -> Vec<f64> {
    let t = utsim_quality::data::parse(text);
    t.rows
        .iter()
        .filter_map(|r| r.get(column))
        .filter_map(|c| utsim_quality::data::number(c, t.decimal_comma))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_read_with_origin() {
        let mut s = State::new();
        let p = Id::v7(1, [1; 10]);
        let id = Id::v7(2, [2; 10]);
        let e = register(
            p,
            id,
            &DatasetInput {
                name: "Vazão".into(),
                origin: DataOrigin::Simulated,
                file: "abc.csv".into(),
                hash: "abc".into(),
                columns: vec!["replicacao".into(), "vazao".into()],
                rows: 10,
                model_hash: "f00".into(),
                model_name: "CD".into(),
                replications: Some(10),
                seed: Some(u64::MAX),
                ..DatasetInput::default()
            },
        );
        let mut n = 10;
        let ops = s.prepare(p, 0, e, || {
            n += 1;
            Id::v7(n, [3; 10])
        });
        for o in &ops {
            s.apply(o).unwrap();
        }
        let d = &datasets(&s, p)[0];
        assert_eq!(d.labeled(), "Vazão (Simulado)");
        assert_eq!(d.seed, Some(u64::MAX));
        assert_eq!(d.columns.len(), 2);
    }

    #[test]
    fn csv_shape_and_values() {
        let text = "a;b\n1,5;x\n2,5;3\n";
        let (h, n) = shape(text);
        assert_eq!(h, vec!["a", "b"]);
        assert_eq!(n, 2);
        assert_eq!(column_values(text, 0), vec![1.5, 2.5]);
        assert_eq!(column_values(text, 1), vec![3.0]);
    }
}
