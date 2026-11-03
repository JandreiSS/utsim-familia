//! Real data in any distribution field ("valores reais"): the model's `data: [..]` list.
//!
//! Every time field of every object (process, setup, interarrival, breakdowns, repair,
//! load and unload times, label values, schedule periods...) holds a [`Dist`]. A field
//! can instead be fed by observed values: an entry of `data` names the field, says where
//! the values are and how to choose, and the distribution is fitted automatically
//! ([`utsim_fit::auto`]) when the model is loaded:
//!
//! ```ron
//! data: [
//!     (
//!         field: "Torno.process",
//!         source: "dados/tempos.csv",
//!         column: "processo",
//!         choose: Best,          // Best | Family("Lognormal") | Empirical
//!         criterion: Aic,        // Aic | Bic | Ks | Ad
//!     ),
//!     (field: "Chegada.interarrival", values: [2.1, 0.7, 3.9, 1.2, 5.0]),
//! ],
//! ```
//!
//! The field itself keeps an ordinary distribution (the last fitted one), so every tool
//! that reads the model, older versions included, keeps working. After the fit the entry
//! records the result (`fitted`: family, distribution, hash of the values and settings),
//! which is saved with the model. On the next load, when the hash and the settings are
//! the same, the recorded distribution is used without refitting (same result, no
//! cost); when the values changed, the field is refitted and a warning says so; when the
//! file cannot be read, the recorded distribution is used, with a warning.
//!
//! Fields are addressed like scenario keys: `<object>.<field>` (`Torno.process`,
//! `Torno.setup`, `Torno.downtime.mttr`, `Chegada.labels.peso`,
//! `Chegada.schedule.periods.0.arrivals`), or `run.<field>`; see [`dist_fields`] for the
//! list of a model. Models without `data` are not touched at all.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utsim_core::Dist;
use utsim_fit::auto::{AutoConfig, AutoFit, Choice, Criterion, IndexEntry};
use utsim_fit::data::DataOptions;
use utsim_fit::report::FitConfig;

use crate::{Model, ModelError};

/// The automatic fit API, re-exported for the editors.
pub use utsim_fit::auto;

/// One field fed by observed values. Two entries are equal when everything that is
/// saved is equal (the load [`DataStatus`] does not count).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataInput {
    /// The distribution field, e.g. `"Torno.process"`.
    pub field: String,
    /// CSV/TXT file with the values, relative to the model file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Column of `source`: header name or position from 1 (automatic when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    /// The column holds increasing instants: fit the intervals between them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub timestamps: bool,
    /// The values themselves, for small samples written in the model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<f64>,
    #[serde(default)]
    pub choose: Choice,
    #[serde(default)]
    pub criterion: Criterion,
    /// Significance level of the goodness-of-fit tests (0.05 when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    /// Parametric bootstrap replicates for the KS and AD p-values (300 when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<usize>,
    /// Seed of the bootstrap (1 when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Result of the last fit, written back when the model is saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fitted: Option<FittedRecord>,
    /// What happened on the last load (not saved).
    #[serde(skip)]
    pub status: DataStatus,
}

impl PartialEq for DataInput {
    fn eq(&self, o: &Self) -> bool {
        self.field == o.field
            && self.source == o.source
            && self.column == o.column
            && self.timestamps == o.timestamps
            && self.values == o.values
            && self.choose == o.choose
            && self.criterion == o.criterion
            && self.alpha == o.alpha
            && self.bootstrap == o.bootstrap
            && self.seed == o.seed
            && self.fitted == o.fitted
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The recorded outcome of a fit, for reproducibility and change detection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FittedRecord {
    /// Chosen family ("Lognormal", "Gama"... or "Empírica").
    pub family: String,
    pub dist: Dist,
    /// [`auto::data_hash`] of the values.
    pub hash: String,
    /// Number of values.
    pub n: usize,
    /// The settings the fit was made with ([`DataInput::settings_key`]).
    pub settings: String,
    /// Problems found in the data, shown as warnings of the model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// How the field got its distribution on the last load.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum DataStatus {
    /// Not resolved yet (a new or edited entry).
    #[default]
    Pending,
    /// First fit of these values.
    Fitted,
    /// Same values and settings as the record: the recorded distribution was used.
    Reused,
    /// The values changed since the record and were refitted.
    DataChanged {
        old_hash: String,
        old_family: String,
    },
    /// The settings changed since the record and the values were refitted.
    SettingsChanged,
    /// The values could not be read: the recorded distribution was used.
    RecordOnly { reason: String },
}

/// Version of the selection procedure, part of the settings key: a change of the
/// procedure invalidates the records.
const PROCEDURE: &str = "auto1";

impl DataInput {
    /// A new entry fed by values written in the model.
    pub fn with_values(field: &str, values: Vec<f64>) -> Self {
        DataInput {
            field: field.to_string(),
            source: None,
            column: None,
            timestamps: false,
            values,
            choose: Choice::Best,
            criterion: Criterion::Aic,
            alpha: None,
            bootstrap: None,
            seed: None,
            fitted: None,
            status: DataStatus::Pending,
        }
    }

    /// A new entry fed by a column of a data file.
    pub fn with_source(field: &str, source: &str, column: Option<&str>) -> Self {
        DataInput {
            source: Some(source.to_string()),
            column: column.map(str::to_string),
            ..DataInput::with_values(field, Vec::new())
        }
    }

    pub fn config(&self) -> AutoConfig {
        let mut fit = FitConfig::default();
        if let Some(a) = self.alpha {
            fit.alpha = a;
        }
        if let Some(b) = self.bootstrap {
            fit.gof.bootstrap = b;
        }
        if let Some(s) = self.seed {
            fit.gof.seed = s;
        }
        AutoConfig {
            choose: self.choose.clone(),
            criterion: self.criterion,
            fit,
        }
    }

    /// Everything besides the values that changes the result, as a short text.
    pub fn settings_key(&self) -> String {
        let c = self.config();
        let choose = match &c.choose {
            Choice::Best => "Best".to_string(),
            Choice::Family(f) => format!("Family({f})"),
            Choice::Empirical => "Empirical".to_string(),
        };
        format!(
            "{PROCEDURE} {choose} {:?} alpha={} bootstrap={} seed={}",
            c.criterion, c.fit.alpha, c.fit.gof.bootstrap, c.fit.gof.seed
        )
    }

    /// Where the values come from, for people.
    pub fn source_label(&self) -> String {
        match &self.source {
            Some(s) => {
                let col = self
                    .column
                    .as_ref()
                    .map_or(String::new(), |c| format!(", coluna {c}"));
                let ts = if self.timestamps {
                    ", intervalos entre instantes"
                } else {
                    ""
                };
                format!("{s}{col}{ts}")
            }
            None => format!("{} valores no modelo", self.values.len()),
        }
    }

    /// Reads the values (file relative to `base`, or the embedded ones).
    pub fn read_values(&self, base: Option<&Path>) -> Result<Vec<f64>, String> {
        match &self.source {
            None if self.values.is_empty() => Err(format!(
                "{}: informe `source` (arquivo de dados) ou `values` (os valores)",
                self.field
            )),
            None => Ok(self.values.clone()),
            Some(_) if !self.values.is_empty() => Err(format!(
                "{}: use `source` ou `values`, não os dois",
                self.field
            )),
            Some(source) => {
                let path = resolve_path(base, source);
                let options = DataOptions {
                    column: self.column.clone(),
                    timestamps: self.timestamps,
                };
                utsim_fit::data::load(&path, &options)
                    .map(|l| l.values)
                    .map_err(|e| format!("{}: {e}", self.field))
            }
        }
    }

    /// Fits the values (full analysis, with the report).
    pub fn analyze(&self, base: Option<&Path>) -> Result<AutoFit, String> {
        let values = self.read_values(base)?;
        auto::auto_fit(&self.field, &values, &self.config())
    }
}

fn resolve_path(base: Option<&Path>, source: &str) -> PathBuf {
    let p = Path::new(source);
    match base {
        Some(b) if p.is_relative() => b.join(p),
        _ => p.to_path_buf(),
    }
}

fn record_of(input: &DataInput, fit: &AutoFit) -> FittedRecord {
    FittedRecord {
        family: fit.family.clone(),
        dist: fit.dist.clone(),
        hash: fit.hash.clone(),
        n: fit.support.n,
        settings: input.settings_key(),
        warnings: fit.warnings.clone(),
    }
}

/// Resolves one entry: reads, reuses or refits, and returns the distribution.
fn resolve_one(input: &mut DataInput, base: Option<&Path>) -> Result<Dist, String> {
    let values = match input.read_values(base) {
        Ok(v) => v,
        Err(reason) => {
            // Only a file that cannot be read falls back on the record; a malformed
            // entry is always an error.
            if input.source.is_some()
                && input.values.is_empty()
                && let Some(rec) = &input.fitted
            {
                input.status = DataStatus::RecordOnly { reason };
                return Ok(rec.dist.clone());
            }
            return Err(reason);
        }
    };
    let hash = auto::data_hash(&values);
    let settings = input.settings_key();
    if let Some(rec) = &input.fitted
        && rec.hash == hash
        && rec.settings == settings
    {
        input.status = DataStatus::Reused;
        return Ok(rec.dist.clone());
    }
    let fit = auto::auto_fit(&input.field, &values, &input.config())?;
    input.status = match &input.fitted {
        None => DataStatus::Fitted,
        Some(rec) if rec.hash != hash => DataStatus::DataChanged {
            old_hash: rec.hash.clone(),
            old_family: rec.family.clone(),
        },
        Some(_) => DataStatus::SettingsChanged,
    };
    input.fitted = Some(record_of(input, &fit));
    Ok(fit.dist)
}

/// Resolves every entry of `model.data` (or only the pending ones) and writes the
/// distributions into their fields. Errors of every entry are collected.
fn resolve_entries(
    model: &mut Model,
    base: Option<&Path>,
    only_pending: bool,
) -> Result<(), ModelError> {
    if model.data.is_empty() {
        return Ok(());
    }
    let mut errors = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut inputs = std::mem::take(&mut model.data);
    for input in &mut inputs {
        if !seen.insert(input.field.clone()) {
            errors.push(format!(
                "data: o campo '{}' aparece mais de uma vez",
                input.field
            ));
            continue;
        }
        if only_pending && input.status != DataStatus::Pending {
            continue;
        }
        match resolve_one(input, base).and_then(|d| set_dist(model, &input.field, &d)) {
            Ok(()) => {}
            Err(e) => errors.push(format!("data: {e}")),
        }
    }
    model.data = inputs;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ModelError::Invalid(errors))
    }
}

/// Resolves every entry (called when the model is loaded). `base` is the folder of the
/// model file, against which `source` paths are taken.
pub fn resolve(model: &mut Model, base: Option<&Path>) -> Result<(), ModelError> {
    resolve_entries(model, base, false)
}

/// Resolves only the entries added or changed since the last resolution (the editor).
pub fn resolve_pending(model: &mut Model, base: Option<&Path>) -> Result<(), ModelError> {
    resolve_entries(model, base, true)
}

/// Warnings about the fields fed by data, for [`Model::warnings`].
pub fn warnings(model: &Model) -> Vec<String> {
    let mut out = Vec::new();
    for input in &model.data {
        let f = &input.field;
        match &input.status {
            DataStatus::DataChanged {
                old_hash,
                old_family,
            } => {
                let now = input.fitted.as_ref();
                out.push(format!(
                    "{f}: os dados mudaram desde o último ajuste (hash {old_hash} -> {}); o campo \
                     foi reajustado ({old_family} -> {}). Salve o modelo para registrar o novo ajuste.",
                    now.map_or("?", |r| r.hash.as_str()),
                    now.map_or("?", |r| r.family.as_str()),
                ));
            }
            DataStatus::RecordOnly { reason } => out.push(format!(
                "{f}: os dados não puderam ser lidos ({reason}); foi usado o último ajuste registrado."
            )),
            _ => {}
        }
        if let Some(rec) = &input.fitted {
            for w in &rec.warnings {
                out.push(format!("{f} (dados): {w}"));
            }
        }
    }
    out
}

/// Renames the object part of the fields that start with `from.`.
pub fn rename_object(model: &mut Model, from: &str, to: &str) {
    let prefix = format!("{from}.");
    for input in &mut model.data {
        if let Some(rest) = input.field.strip_prefix(&prefix) {
            input.field = format!("{to}.{rest}");
        }
    }
}

/// Drops the entries of the fields of a removed object.
pub fn remove_object(model: &mut Model, name: &str) {
    let prefix = format!("{name}.");
    model.data.retain(|i| !i.field.starts_with(&prefix));
}

// --- Addressing fields ---------------------------------------------------------------

/// Top-level parts of the model that can hold distributions besides the objects.
const TOP_LEVEL: &[&str] = &["run", "flows", "fluids", "network", "calendars", "spans"];

fn to_json(model: &Model) -> Result<Value, String> {
    serde_json::to_value(model).map_err(|e| format!("o modelo não pôde ser percorrido: {e}"))
}

fn escape(seg: &str) -> String {
    seg.replace('~', "~0").replace('/', "~1")
}

fn is_variant(key: &str) -> bool {
    key.chars().next().is_some_and(char::is_uppercase)
}

/// JSON pointer of `field` in the serialized model.
fn locate(root: &Value, field: &str) -> Result<String, String> {
    let mut segs = field.split('.');
    let first = segs.next().unwrap_or_default();
    let objects = root["objects"].as_array().cloned().unwrap_or_default();
    let (mut pointer, mut node) = if let Some(i) = objects
        .iter()
        .position(|o| o["name"].as_str() == Some(first))
    {
        (format!("/objects/{i}"), &root["objects"][i])
    } else if TOP_LEVEL.contains(&first) && root.get(first).is_some() {
        (format!("/{first}"), &root[first])
    } else {
        let names: Vec<&str> = objects.iter().filter_map(|o| o["name"].as_str()).collect();
        return Err(format!(
            "'{field}': objeto '{first}' não existe; objetos: {}",
            names.join(", ")
        ));
    };
    let mut walked = first.to_string();
    for seg in segs {
        loop {
            match node {
                Value::Object(map) if map.contains_key(seg) => {
                    pointer = format!("{pointer}/{}", escape(seg));
                    node = &map[seg];
                    break;
                }
                Value::Array(items) => {
                    let index = seg
                        .parse::<usize>()
                        .ok()
                        .or_else(|| items.iter().position(|it| it["name"].as_str() == Some(seg)));
                    match index.filter(|&i| i < items.len()) {
                        Some(i) => {
                            pointer = format!("{pointer}/{i}");
                            node = &items[i];
                            break;
                        }
                        None => {
                            return Err(format!(
                                "'{field}': '{walked}' tem {} elementos; '{seg}' não é um deles",
                                items.len()
                            ));
                        }
                    }
                }
                // `kind` and enum variants are crossed without being named.
                Value::Object(map) if map.contains_key("kind") => {
                    pointer = format!("{pointer}/kind");
                    node = &map["kind"];
                }
                Value::Object(map) if map.len() == 1 && is_variant(map.keys().next().unwrap()) => {
                    let k = map.keys().next().unwrap();
                    pointer = format!("{pointer}/{}", escape(k));
                    node = &map[k];
                }
                Value::Null => {
                    return Err(format!(
                        "'{field}': '{walked}' não está definido no modelo; defina-o antes"
                    ));
                }
                Value::Object(map) => {
                    let keys: Vec<&str> = map.keys().map(String::as_str).collect();
                    return Err(format!(
                        "'{field}': '{walked}' não tem o campo '{seg}'; campos: {}",
                        keys.join(", ")
                    ));
                }
                _ => {
                    return Err(format!(
                        "'{field}': '{walked}' não tem campos (não há '{seg}')"
                    ));
                }
            }
        }
        walked = format!("{walked}.{seg}");
    }
    Ok(pointer)
}

fn as_dist(v: &Value) -> Option<Dist> {
    if v.is_object() {
        serde_json::from_value(v.clone()).ok()
    } else {
        None
    }
}

/// The distribution in `field` (`None` when it is an optional field left empty).
pub fn get_dist(model: &Model, field: &str) -> Result<Option<Dist>, String> {
    let root = to_json(model)?;
    let pointer = locate(&root, field)?;
    let node = &root.pointer(&pointer).expect("located");
    if node.is_null() {
        return Ok(None);
    }
    as_dist(node)
        .map(Some)
        .ok_or_else(|| format!("'{field}' não é um campo de distribuição"))
}

/// Writes `dist` into `field`. The field must hold a distribution (or be an empty
/// optional distribution, like a processor's `setup`).
pub fn set_dist(model: &mut Model, field: &str, dist: &Dist) -> Result<(), String> {
    let mut root = to_json(model)?;
    let pointer = locate(&root, field)?;
    let node = root.pointer_mut(&pointer).expect("located");
    if !node.is_null() && as_dist(node).is_none() {
        return Err(format!("'{field}' não é um campo de distribuição"));
    }
    *node = serde_json::to_value(dist).map_err(|e| e.to_string())?;
    let data = std::mem::take(&mut model.data);
    let patched: Result<Model, _> = serde_json::from_value(root);
    match patched {
        Ok(mut m) => {
            m.data = data;
            *model = m;
            Ok(())
        }
        Err(e) => {
            model.data = data;
            Err(format!("'{field}' não aceita uma distribuição aqui: {e}"))
        }
    }
}

/// Every distribution field of the model, by the key [`DataInput::field`] uses, in file
/// order. Optional distributions left empty (`setup: None`) are listed too.
pub fn dist_fields(model: &Model) -> Vec<String> {
    let Ok(root) = to_json(model) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(objects) = root["objects"].as_array() {
        for o in objects {
            let name = o["name"].as_str().unwrap_or_default();
            walk(o, name, &mut out);
        }
    }
    for key in TOP_LEVEL {
        if let Some(node) = root.get(*key) {
            walk(node, key, &mut out);
        }
    }
    // Keep only the keys that resolve back to themselves.
    out.retain(|f| locate(&root, f).is_ok());
    out
}

/// Field names that may hold an optional distribution while empty.
const OPTIONAL_DISTS: &[&str] = &["setup", "dwell"];

fn walk(node: &Value, path: &str, out: &mut Vec<String>) {
    match node {
        Value::Object(map) => {
            for (k, v) in map {
                if k == "name" {
                    continue;
                }
                // Variants and `kind` are crossed without being named.
                let child = if k == "kind" || (map.len() == 1 && is_variant(k)) {
                    path.to_string()
                } else {
                    format!("{path}.{k}")
                };
                if child != path
                    && (as_dist(v).is_some()
                        || (v.is_null() && OPTIONAL_DISTS.contains(&k.as_str())))
                {
                    out.push(child);
                } else {
                    walk(v, &child, out);
                }
            }
        }
        Value::Array(items) => {
            for (i, it) in items.iter().enumerate() {
                let label = it["name"].as_str().map_or(i.to_string(), str::to_string);
                walk(it, &format!("{path}.{label}"), out);
            }
        }
        _ => {}
    }
}

// --- Reports ---------------------------------------------------------------------------

/// The full analysis of every entry, in order (an error when the values cannot be read).
pub fn analyze_all(
    model: &Model,
    base: Option<&Path>,
) -> Vec<(DataInput, Result<AutoFit, String>)> {
    model
        .data
        .iter()
        .map(|i| (i.clone(), i.analyze(base)))
        .collect()
}

/// File name of the report of a field (letters, digits, `-`, `_`, `.`).
pub fn report_name(field: &str) -> String {
    field
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Writes one HTML and one CSV report per field plus `index.html` into `dir`, and
/// returns the paths written (index first) and the number of fields that failed.
pub fn write_reports(
    model: &Model,
    base: Option<&Path>,
    dir: &Path,
) -> std::io::Result<(Vec<PathBuf>, usize)> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    let mut entries = Vec::new();
    let mut failed = 0;
    for (input, result) in analyze_all(model, base) {
        let name = report_name(&input.field);
        let entry = match result {
            Ok(fit) => {
                let html = dir.join(format!("{name}.html"));
                let csv = dir.join(format!("{name}.csv"));
                std::fs::write(&html, fit.to_html("utsim dados"))?;
                std::fs::write(&csv, fit.to_csv())?;
                written.push(html);
                written.push(csv);
                IndexEntry {
                    field: input.field.clone(),
                    source: input.source_label(),
                    result: Ok((fit.family.clone(), fit.ron())),
                    warnings: fit.warnings.len(),
                    href: Some(format!("{name}.html")),
                }
            }
            Err(e) => {
                failed += 1;
                IndexEntry {
                    field: input.field.clone(),
                    source: input.source_label(),
                    result: Err(e),
                    warnings: 0,
                    href: None,
                }
            }
        };
        entries.push(entry);
    }
    let index = dir.join("index.html");
    std::fs::write(&index, auto::index_html(&model.name, &entries))?;
    written.insert(0, index);
    Ok((written, failed))
}
