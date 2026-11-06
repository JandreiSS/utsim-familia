//! Integration with the UTSim model (`docs/plano_lean_seis_sigma.md`, 3.1 to 3.4 and 3.6):
//! the project's **link to a model** with the hash of its content, the maps generated from
//! it (flowchart, process chart and SIPOC, through `utsim-diagram`), the **simulated data
//! sets** (KPIs per replication, always labelled), the **studies** (scenarios with common
//! random numbers compared to the baseline by paired difference, with the interval and the
//! decision "significant and relevant", saved as `estudo.json`), the bottleneck and the
//! shifting bottleneck and the **observed critical path** (`utsim-pert`).
//!
//! Pure: the caller reads the model's text and runs the replications (on a worker thread);
//! these functions build the edits and the records from the results. When the model file
//! changes, [`stale`] lists what depends on the old version.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use utsim_model::Model;
use utsim_model::analysis::ObjectStats;
use utsim_model::experiment::{Scenario, ScenarioResult};

use crate::dados::{self, DataOrigin, DatasetInput};
use crate::date::Date;
use crate::domain::enum_value;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};
use crate::tools::{self, Deliverable, Tool, sipoc};

pub mod fields {
    pub const PROJECT: &str = "projeto";
    pub const NAME: &str = "nome";
    // Model link.
    /// Path of the `.ron`, relative to the project folder (`modelos/x.ron` when copied).
    pub const PATH: &str = "caminho";
    pub const HASH: &str = "hash";
    pub const FORMAT: &str = "formato_modelo";
    pub const LINKED_ON: &str = "ligado_em";
    pub const COPIED: &str = "copiado";
    // What depends on a model version (data sets, studies, analyses, generated SIPOC).
    pub const MODEL_HASH: &str = "modelo_hash";
    /// A deliverable generated from the model: `modelo`.
    pub const GENERATED: &str = "gerado_de";
    // Study and analysis.
    pub const FILE: &str = "arquivo";
    pub const KIND: &str = "tipo";
    pub const RESULT: &str = "resultado";
    pub const REPLICATIONS: &str = "replicacoes";
    pub const DATE: &str = "data";
    pub const CONCLUSION: &str = "conclusao";
}

/// Version of the `estudo.json` schema.
pub const STUDY_SCHEMA: u32 = 1;
/// Confidence level of the paired differences.
pub const LEVEL: f64 = 0.95;

/// Hex SHA-256 of a model's text.
pub fn hash_text(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// --- The link ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelLink {
    pub id: Id,
    pub path: String,
    pub hash: String,
    pub name: String,
    pub format: u32,
    pub linked_on: Option<Date>,
    pub copied: bool,
}

impl ModelLink {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            path: e.text(fields::PATH),
            hash: e.text(fields::HASH),
            name: e.text(fields::NAME),
            format: e.integer(fields::FORMAT).map_or(1, |n| n.max(1) as u32),
            linked_on: e.opt_text(fields::LINKED_ON).and_then(|s| s.parse().ok()),
            copied: e.boolean(fields::COPIED),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::ModelLink, id)
    }
}

/// The project's model link (the most recent one).
pub fn link(state: &State, project: Id) -> Option<ModelLink> {
    let key = project.to_string();
    state
        .of_kind(Kind::ModelLink)
        .filter(|(_, e)| !e.is_deleted() && e.text(fields::PROJECT) == key)
        .map(|(id, e)| ModelLink::read(id, e))
        .max_by_key(|l| (l.linked_on, l.id))
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SimError {
    #[error("modelo inválido: {0}")]
    Model(String),
    #[error("o modelo não tem o indicador {0}")]
    NoMetric(String),
    #[error("dê um nome")]
    NoName,
    #[error("o estudo precisa de ao menos um cenário além da linha de base")]
    NoScenario,
}

/// Reads a model's text (with its folder, for the relative data files).
pub fn parse(text: &str, base: Option<&std::path::Path>) -> Result<Model, SimError> {
    let m = Model::parse_in(text, base).map_err(|e| SimError::Model(e.to_string()))?;
    m.validate().map_err(|e| SimError::Model(e.to_string()))?;
    Ok(m)
}

/// Edits of the link (or of a new version of it: the same entity, the new hash).
pub fn link_edits(
    project: Id,
    id: Id,
    path: &str,
    text: &str,
    model: &Model,
    copied: bool,
    date: Date,
) -> Vec<Edit> {
    let r = ModelLink::entity_ref(id);
    vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::PATH, path),
        Edit::new(r, fields::HASH, hash_text(text)),
        Edit::new(r, fields::NAME, model.name.as_str()),
        Edit::new(r, fields::FORMAT, model.format.unwrap_or(1)),
        Edit::new(r, fields::COPIED, copied),
        Edit::new(r, fields::LINKED_ON, date.to_string()),
    ]
}

/// Something that depends on a version of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Stale {
    pub kind: Kind,
    pub id: Id,
    pub name: String,
    pub hash: String,
}

/// What was made from another version of the model than `current_hash` (the hash of the
/// file now): data sets, studies, analyses and generated deliverables.
pub fn stale(state: &State, project: Id, current_hash: &str) -> Vec<Stale> {
    let key = project.to_string();
    let mut v = Vec::new();
    for kind in [
        Kind::Dataset,
        Kind::Study,
        Kind::Analysis,
        Kind::Deliverable,
    ] {
        for (id, e) in state.of_kind(kind) {
            if e.is_deleted() || e.text(fields::PROJECT) != key {
                continue;
            }
            let h = e.text(fields::MODEL_HASH);
            if !h.is_empty() && h != current_hash {
                v.push(Stale {
                    kind,
                    id,
                    name: e.text(fields::NAME),
                    hash: h,
                });
            }
        }
    }
    v.sort_by_key(|s| (s.kind, s.id));
    v
}

// --- Maps ----------------------------------------------------------------------------------

/// The process chart of the model's main path.
pub fn carta(model: &Model) -> utsim_diagram::Carta {
    let g = utsim_diagram::flowchart(model, &utsim_diagram::Options::default());
    utsim_diagram::carta::build(&g)
}

/// A diagram of the model as a self-contained HTML page (flowchart, process chart or
/// SIPOC), to save next to the project.
pub fn diagram_html(model: &Model, kind: utsim_diagram::TipoDiagrama) -> Result<String, SimError> {
    utsim_diagram::gerar(kind, model, &utsim_diagram::Options::default())
        .map(|s| s.html)
        .map_err(SimError::Model)
}

/// The value stream map of the model (one run with the trace): its page and the processes
/// of the main path, to fill a VSM deliverable (P4).
pub fn vsm_from_model(
    model: &Model,
) -> Result<(String, Vec<crate::tools::vsm::ModelProcess>), SimError> {
    let s = utsim_diagram::gerar(
        utsim_diagram::TipoDiagrama::Vsm,
        model,
        &utsim_diagram::Options::default(),
    )
    .map_err(SimError::Model)?;
    let rows = crate::tools::vsm::from_diagram_csv(s.csv.as_deref().unwrap_or(""));
    Ok((s.html, rows))
}

/// One line of the distances of a spaghetti diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Travel {
    #[serde(rename = "recurso")]
    pub resource: String,
    #[serde(rename = "viagens")]
    pub trips: u64,
    #[serde(rename = "distancia")]
    pub distance: f64,
}

/// The spaghetti diagram of the model over the plant (one run with the trace): its page
/// and the trips and distance of each mobile resource and of the items (the last line).
pub fn spaghetti(model: &Model) -> Result<(String, Vec<Travel>), SimError> {
    let s = utsim_diagram::gerar(
        utsim_diagram::TipoDiagrama::Espaguete,
        model,
        &utsim_diagram::Options::default(),
    )
    .map_err(SimError::Model)?;
    let rows = s
        .csv
        .as_deref()
        .unwrap_or("")
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            if c.len() < 3 {
                return None;
            }
            Some(Travel {
                resource: c[0].trim_matches('"').to_string(),
                trips: c[1].trim().parse().ok()?,
                distance: c[2].trim().parse().ok()?,
            })
        })
        .collect();
    Ok((s.html, rows))
}

/// The five SIPOC columns generated from the model (suppliers, inputs, process, outputs,
/// customers), in [`sipoc::COLUMNS`] order.
pub fn sipoc_columns(model: &Model) -> Result<[Vec<String>; 5], SimError> {
    let s = utsim_diagram::gerar(
        utsim_diagram::TipoDiagrama::Sipoc,
        model,
        &utsim_diagram::Options::default(),
    )
    .map_err(SimError::Model)?;
    Ok(sipoc_from_csv(s.csv.as_deref().unwrap_or("")))
}

/// Parses the SIPOC CSV of `utsim-diagram` (header and one value per column per row).
pub fn sipoc_from_csv(csv: &str) -> [Vec<String>; 5] {
    let mut cols: [Vec<String>; 5] = Default::default();
    for line in csv.lines().skip(1) {
        for (i, cell) in split_csv(line).into_iter().enumerate().take(5) {
            let c = cell.trim();
            if !c.is_empty() {
                cols[i].push(c.to_string());
            }
        }
    }
    cols
}

/// One CSV line with `"` quoting (the format of `utsim_model::csv`).
fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Edits of a SIPOC deliverable generated from the model: the belt edits it afterwards
/// and it stays marked "gerado do modelo, editado" with the model's hash.
pub fn sipoc_edits(
    state: &State,
    project: Id,
    deliverable: Id,
    model_hash: &str,
    columns: &[Vec<String>; 5],
    mut ids: impl FnMut() -> Id,
) -> Vec<Edit> {
    let phase = tools::default_phase(state, project, Tool::Sipoc);
    let mut e = Deliverable::create(
        project,
        phase,
        Tool::Sipoc,
        deliverable,
        "SIPOC gerado do modelo",
    );
    let r = Deliverable::entity_ref(deliverable);
    e.push(Edit::new(r, fields::GENERATED, "modelo"));
    e.push(Edit::new(r, fields::MODEL_HASH, model_hash));
    for (ci, (kind, _)) in sipoc::COLUMNS.iter().enumerate() {
        for (order, text) in columns[ci].iter().enumerate() {
            let ir = tools::item_ref(ids());
            use tools::fields::item as f;
            e.push(Edit::new(ir, f::PROJECT, project.to_string()));
            e.push(Edit::new(ir, f::DELIVERABLE, deliverable.to_string()));
            e.push(Edit::new(ir, f::ITEM_KIND, *kind));
            e.push(Edit::new(ir, f::ORDER, order as f64 + 1.0));
            e.push(Edit::new(ir, sipoc::TEXT, text.as_str()));
        }
    }
    e
}

// --- Simulated data sets ---------------------------------------------------------------------

/// The KPIs of every replication as CSV (`replicacao,<kpi>...`), the content of a
/// simulated data set.
pub fn replication_csv(per_rep: &BTreeMap<String, Vec<f64>>, metrics: &[String]) -> String {
    let keep: Vec<&String> = if metrics.is_empty() {
        per_rep.keys().collect()
    } else {
        metrics
            .iter()
            .filter(|m| per_rep.contains_key(*m))
            .collect()
    };
    let n = keep.iter().map(|m| per_rep[*m].len()).max().unwrap_or(0);
    let mut out = String::from("replicacao");
    for m in &keep {
        out.push(',');
        out.push_str(&quote(m));
    }
    out.push('\n');
    for r in 0..n {
        out.push_str(&(r + 1).to_string());
        for m in &keep {
            out.push(',');
            if let Some(x) = per_rep[*m].get(r).filter(|x| x.is_finite()) {
                out.push_str(&x.to_string());
            }
        }
        out.push('\n');
    }
    out
}

fn quote(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// What a simulated data set records: always the origin `Simulado`, the model's hash, the
/// seed and the replications.
#[allow(clippy::too_many_arguments)]
pub fn simulated_dataset(
    name: &str,
    model: &Model,
    model_hash: &str,
    scenario: &str,
    file: &str,
    file_hash: &str,
    csv: &str,
    date: Date,
) -> DatasetInput {
    let (columns, rows) = dados::shape(csv);
    DatasetInput {
        name: name.trim().to_string(),
        origin: DataOrigin::Simulated,
        file: file.into(),
        hash: file_hash.into(),
        columns,
        rows,
        model_hash: model_hash.into(),
        model_name: model.name.clone(),
        scenario: scenario.into(),
        replications: Some(model.run.replications),
        seed: Some(model.run.seed),
        date: Some(date),
        note: "KPIs por replicação de um modelo; dado simulado, não é coleta".into(),
    }
}

// --- Studies -------------------------------------------------------------------------------------

/// Mean and half-width of a metric in a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    #[serde(rename = "cenario")]
    pub scenario: String,
    #[serde(rename = "indicador")]
    pub metric: String,
    #[serde(rename = "media")]
    pub mean: f64,
    #[serde(rename = "meia_largura")]
    pub half_width: f64,
    pub n: usize,
}

/// The paired difference of a scenario to the baseline for one metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Difference {
    #[serde(rename = "cenario")]
    pub scenario: String,
    #[serde(rename = "indicador")]
    pub metric: String,
    /// Mean of scenario − baseline over the replications (same seeds).
    #[serde(rename = "diferenca")]
    pub mean: f64,
    #[serde(rename = "meia_largura")]
    pub half_width: f64,
    /// The interval does not contain zero.
    #[serde(rename = "significativa")]
    pub significant: bool,
    /// The minimum difference of interest given by the belt (0: none).
    #[serde(rename = "minima_de_interesse")]
    pub minimum: f64,
    /// Significant and at least the minimum of interest.
    #[serde(rename = "relevante")]
    pub relevant: bool,
}

impl Difference {
    pub fn lo(&self) -> f64 {
        self.mean - self.half_width
    }

    pub fn hi(&self) -> f64 {
        self.mean + self.half_width
    }
}

/// Decides a difference: significant when the interval excludes zero; relevant when it is
/// significant and the whole interval is at least `minimum` away from zero (a minimum of
/// zero needs only significance).
pub fn decide(mean: f64, half_width: f64, minimum: f64) -> (bool, bool) {
    if !mean.is_finite() || !half_width.is_finite() {
        return (false, false);
    }
    let lo = mean - half_width;
    let hi = mean + half_width;
    let significant = lo > 0.0 || hi < 0.0;
    let near = lo.abs().min(hi.abs());
    let relevant = significant && near + 1e-12 >= minimum.abs();
    (significant, relevant)
}

/// The study package (`estudo.json`): enough to reproduce every number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudyFile {
    #[serde(rename = "esquema")]
    pub schema: u32,
    #[serde(rename = "nome")]
    pub name: String,
    #[serde(rename = "modelo")]
    pub model: String,
    #[serde(rename = "modelo_hash")]
    pub model_hash: String,
    #[serde(rename = "versao")]
    pub version: String,
    #[serde(rename = "replicacoes")]
    pub replications: u32,
    #[serde(rename = "semente")]
    pub seed: u64,
    #[serde(rename = "nivel")]
    pub level: f64,
    /// The first one is the baseline.
    #[serde(rename = "cenarios")]
    pub scenarios: Vec<Scenario>,
    #[serde(rename = "indicadores")]
    pub measures: Vec<String>,
    #[serde(rename = "resultados")]
    pub results: Vec<Estimate>,
    #[serde(rename = "por_replicacao")]
    pub per_replication: BTreeMap<String, BTreeMap<String, Vec<f64>>>,
    #[serde(rename = "diferencas")]
    pub differences: Vec<Difference>,
}

impl StudyFile {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("study")
    }

    pub fn from_json(text: &str) -> Result<StudyFile, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    /// The differences of one scenario.
    pub fn of(&self, scenario: &str) -> Vec<&Difference> {
        self.differences
            .iter()
            .filter(|d| d.scenario == scenario)
            .collect()
    }
}

/// Builds the package from the results of [`utsim_model::experiment::run_scenarios`]: the
/// first scenario is the baseline; every other one is compared with it by the paired
/// difference of each measure.
#[allow(clippy::too_many_arguments)]
pub fn build_study(
    name: &str,
    model: &Model,
    model_hash: &str,
    scenarios: &[Scenario],
    measures: &[String],
    results: &[ScenarioResult],
    minimums: &BTreeMap<String, f64>,
) -> Result<StudyFile, SimError> {
    if name.trim().is_empty() {
        return Err(SimError::NoName);
    }
    if results.len() < 2 {
        return Err(SimError::NoScenario);
    }
    let names: Vec<String> = if measures.is_empty() {
        let mut v: Vec<String> = results
            .iter()
            .flat_map(|r| r.summary.keys().cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    } else {
        measures.to_vec()
    };
    let mut est = Vec::new();
    let mut per = BTreeMap::new();
    for r in results {
        for m in &names {
            if let Some(s) = r.summary.get(m) {
                est.push(Estimate {
                    scenario: r.name.clone(),
                    metric: m.clone(),
                    mean: s.mean,
                    half_width: s.half_width,
                    n: s.n,
                });
            }
        }
        per.insert(
            r.name.clone(),
            r.per_replication
                .iter()
                .filter(|(k, _)| names.contains(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
    }
    let base = &results[0];
    let mut diffs = Vec::new();
    for r in &results[1..] {
        for m in &names {
            let (Some(a), Some(b)) = (r.per_replication.get(m), base.per_replication.get(m)) else {
                continue;
            };
            let s = utsim_model::analysis::paired_difference_at(a, b, LEVEL);
            let minimum = minimums.get(m).copied().unwrap_or(0.0);
            let (significant, relevant) = decide(s.mean, s.half_width, minimum);
            diffs.push(Difference {
                scenario: r.name.clone(),
                metric: m.clone(),
                mean: s.mean,
                half_width: s.half_width,
                significant,
                minimum,
                relevant,
            });
        }
    }
    Ok(StudyFile {
        schema: STUDY_SCHEMA,
        name: name.trim().to_string(),
        model: model.name.clone(),
        model_hash: model_hash.to_string(),
        version: format!("utsim-lss-core {}", env!("CARGO_PKG_VERSION")),
        replications: model.run.replications,
        seed: model.run.seed,
        level: LEVEL,
        scenarios: scenarios.to_vec(),
        measures: names,
        results: est,
        per_replication: per,
        differences: diffs,
    })
}

/// A study of the project journal (the package is the file in `dados/`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Study {
    pub id: Id,
    pub name: String,
    pub file: String,
    pub hash: String,
    pub model_hash: String,
    pub date: Option<Date>,
    pub conclusion: String,
}

impl Study {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            name: e.text(fields::NAME),
            file: e.text(fields::FILE),
            hash: e.text(fields::HASH),
            model_hash: e.text(fields::MODEL_HASH),
            date: e.opt_text(fields::DATE).and_then(|s| s.parse().ok()),
            conclusion: e.text(fields::CONCLUSION),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Study, id)
    }
}

pub fn studies(state: &State, project: Id) -> Vec<Study> {
    let key = project.to_string();
    let mut v: Vec<Study> = state
        .of_kind(Kind::Study)
        .filter(|(_, e)| !e.is_deleted() && e.text(fields::PROJECT) == key)
        .map(|(id, e)| Study::read(id, e))
        .collect();
    v.sort_by_key(|s| (s.date, s.id));
    v
}

/// Edits of a study saved as `file` (in `dados/`) with the hash of its text.
pub fn record_study(
    project: Id,
    id: Id,
    study: &StudyFile,
    file: &str,
    file_hash: &str,
    date: Date,
) -> Vec<Edit> {
    let r = Study::entity_ref(id);
    let relevant: Vec<String> = study
        .differences
        .iter()
        .filter(|d| d.relevant)
        .map(|d| format!("{} em {}", d.scenario, d.metric))
        .collect();
    vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::NAME, study.name.as_str()),
        Edit::new(r, fields::FILE, file),
        Edit::new(r, fields::HASH, file_hash),
        Edit::new(r, fields::MODEL_HASH, study.model_hash.as_str()),
        Edit::new(r, fields::DATE, date.to_string()),
        Edit::new(
            r,
            fields::CONCLUSION,
            if relevant.is_empty() {
                "nenhuma diferença significativa e relevante".to_string()
            } else {
                format!("significativa e relevante: {}", relevant.join("; "))
            },
        ),
    ]
}

// --- Estimated gain from a study ---------------------------------------------------------------

/// How the belt turns a difference of a study into money (section 1.10: the saving starts
/// estimated, with source "simulação", and never becomes validated without real entries).
#[derive(Debug, Clone, PartialEq)]
pub struct GainInput {
    pub description: String,
    pub category: crate::domain::SavingCategory,
    /// Value of one unit of the measure, in the company's currency.
    pub value_per_unit: f64,
    /// How many times a year the simulated horizon happens (a run of one week: 52).
    pub periods_per_year: f64,
    /// An increase of the measure is the improvement (throughput); otherwise a decrease is
    /// (lead time, queue).
    pub higher_is_better: bool,
    /// Uses the end of the interval nearest to zero instead of the mean.
    pub conservative: bool,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GainError {
    #[error("dê uma descrição ao ganho")]
    NoDescription,
    #[error("o valor por unidade e os períodos por ano devem ser positivos")]
    BadFactor,
    #[error("a diferença não é significativa: o intervalo contém zero")]
    NotSignificant,
    #[error("o cenário piora o indicador no sentido escolhido")]
    Worse,
}

/// The yearly value and the rationale (formula, assumptions and source) of a saving
/// estimated from one difference of a saved study.
pub fn gain_value(
    study: &Study,
    file: &StudyFile,
    d: &Difference,
    g: &GainInput,
) -> Result<(f64, String), GainError> {
    if g.description.trim().is_empty() {
        return Err(GainError::NoDescription);
    }
    let ok = |x: f64| x.is_finite() && x > 0.0;
    if !ok(g.value_per_unit) || !ok(g.periods_per_year) {
        return Err(GainError::BadFactor);
    }
    if !d.significant {
        return Err(GainError::NotSignificant);
    }
    let sign = if g.higher_is_better { 1.0 } else { -1.0 };
    let basis = if g.conservative {
        if d.lo().abs() < d.hi().abs() {
            d.lo()
        } else {
            d.hi()
        }
    } else {
        d.mean
    };
    let improvement = basis * sign;
    if improvement <= 0.0 {
        return Err(GainError::Worse);
    }
    let value = improvement * g.value_per_unit * g.periods_per_year;
    let n = num;
    let rationale = format!(
        "Estimado por simulação no estudo \"{}\" (arquivo {}, hash {}; modelo {}, hash {}; {} replicações, semente {}). \
         Cenário \"{}\" contra a linha de base, indicador {}: diferença pareada {} [{}; {}] a {} %{}. \
         Base usada: {} ({}). Conta: {} × {} por unidade × {} períodos por ano = {} por ano. \
         {} Um ganho estimado por simulação só é validado com lançamentos reais.",
        file.name,
        study.file,
        short(&study.hash),
        file.model,
        short(&file.model_hash),
        file.replications,
        file.seed,
        d.scenario,
        d.metric,
        n(d.mean),
        n(d.lo()),
        n(d.hi()),
        (file.level * 100.0).round(),
        if d.relevant {
            ", significativa e relevante"
        } else {
            ", significativa"
        },
        n(basis),
        if g.conservative {
            "ponta do intervalo mais próxima de zero"
        } else {
            "média"
        },
        n(improvement),
        n(g.value_per_unit),
        n(g.periods_per_year),
        n(value),
        if g.higher_is_better {
            "Aumento do indicador é melhora."
        } else {
            "Redução do indicador é melhora."
        },
    );
    Ok((value, rationale))
}

fn short(h: &str) -> &str {
    &h[..h.len().min(12)]
}

/// Edits of a saving estimated from a difference of a saved study: estimated, source
/// "simulação", with the rationale and the study it came from.
pub fn gain_edits(
    project: Id,
    id: Id,
    study: &Study,
    file: &StudyFile,
    d: &Difference,
    g: &GainInput,
) -> Result<Vec<Edit>, GainError> {
    let (value, rationale) = gain_value(study, file, d, g)?;
    let mut e = crate::savings::new_saving(
        project,
        id,
        &g.description,
        g.category,
        value,
        crate::savings::Source::Simulation,
    );
    let r = EntityRef::new(Kind::Saving, id);
    e.push(Edit::new(
        r,
        crate::domain::fields::saving::RATIONALE,
        rationale,
    ));
    e.push(Edit::new(
        r,
        crate::savings::fields::STUDY,
        study.id.to_string(),
    ));
    e.push(Edit::new(
        r,
        crate::savings::fields::STUDY_DIFFERENCE,
        format!("{} em {}", d.scenario, d.metric),
    ));
    Ok(e)
}

/// A number with a decimal comma and up to four decimals.
fn num(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

// --- Analyses on the model ---------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum AnalysisKind {
    #[default]
    #[serde(rename = "gargalo")]
    Bottleneck,
    #[serde(rename = "gargalo_movel")]
    ShiftingBottleneck,
    #[serde(rename = "pert_observado")]
    ObservedPert,
    /// Distances travelled by the mobile resources and the items (spaghetti diagram).
    #[serde(rename = "espaguete")]
    Spaghetti,
}

/// One row of the bottleneck table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceRow {
    #[serde(rename = "nome")]
    pub name: String,
    #[serde(rename = "tipo")]
    pub kind: String,
    #[serde(rename = "utilizacao")]
    pub utilization: f64,
    #[serde(rename = "meia_largura")]
    pub half_width: f64,
    #[serde(rename = "utilizacao_no_turno")]
    pub in_shift: Option<f64>,
}

/// The bottleneck: the resources by utilization, with the one of the highest utilization
/// and the one of the highest utilization on shift.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bottleneck {
    #[serde(rename = "recursos")]
    pub rows: Vec<ResourceRow>,
    #[serde(rename = "gargalo")]
    pub bottleneck: Option<String>,
    #[serde(rename = "gargalo_no_turno")]
    pub in_shift: Option<String>,
}

pub fn bottleneck(stats: &[ObjectStats]) -> Bottleneck {
    use utsim_model::analysis as a;
    let mut rows: Vec<ResourceRow> = stats
        .iter()
        .filter_map(|s| {
            let u = s.utilization.as_ref()?;
            Some(ResourceRow {
                name: s.name.clone(),
                kind: s.kind.clone(),
                utilization: u.mean,
                half_width: u.half_width,
                in_shift: s.utilization_in_shift.as_ref().map(|x| x.mean),
            })
        })
        .collect();
    rows.sort_by(|x, y| {
        y.utilization
            .total_cmp(&x.utilization)
            .then(x.name.cmp(&y.name))
    });
    Bottleneck {
        rows,
        bottleneck: a::bottleneck(stats).map(|i| stats[i].name.clone()),
        in_shift: a::bottleneck_in_shift(stats).map(|i| stats[i].name.clone()),
    }
}

/// The share of the time each resource was the bottleneck (sole and shifting).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Share {
    #[serde(rename = "nome")]
    pub name: String,
    #[serde(rename = "unico")]
    pub sole: f64,
    #[serde(rename = "compartilhado")]
    pub shifting: f64,
}

pub fn shares(v: &[utsim_model::analysis::BottleneckShare]) -> Vec<Share> {
    v.iter()
        .map(|b| Share {
            name: b.name.clone(),
            sole: b.sole.mean,
            shifting: b.shifting.mean,
        })
        .collect()
}

/// The observed critical path of a stretch of the process, as kept in the journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observed {
    #[serde(rename = "de")]
    pub from: String,
    #[serde(rename = "ate")]
    pub to: String,
    #[serde(rename = "unidades")]
    pub units: usize,
    #[serde(rename = "total")]
    pub total: f64,
    #[serde(rename = "meia_largura")]
    pub half_width: f64,
    /// `(activity, criticality, wait, work, blocked)` by criticality.
    #[serde(rename = "atividades")]
    pub activities: Vec<(String, f64, f64, f64, f64)>,
    /// The most frequent chains and their share.
    #[serde(rename = "caminhos")]
    pub paths: Vec<(Vec<String>, f64)>,
    #[serde(rename = "avisos")]
    pub warnings: Vec<String>,
}

/// Runs the observed critical path of `from`..`to` (the network of the objects between
/// them, the model's replications).
pub fn observed_pert(
    model: &Model,
    from: &str,
    to: &str,
    replications: Option<u32>,
) -> Result<Observed, SimError> {
    use utsim_pert::network::{self, DeriveOptions, Selection};
    let sel = Selection::Between {
        from: from.into(),
        to: to.into(),
    };
    let net = network::derive(model, &sel, &DeriveOptions::default())
        .map_err(|e| SimError::Model(e.to_string()))?;
    let opts = utsim_pert::ObservedOptions {
        replications,
        ..utsim_pert::ObservedOptions::default()
    };
    let r = utsim_pert::observed::run(model, &net, &opts)
        .map_err(|e| SimError::Model(e.to_string()))?;
    let mut acts: Vec<(String, f64, f64, f64, f64)> = r
        .activities
        .iter()
        .map(|a| {
            (
                a.name.clone(),
                a.criticality.mean,
                a.wait,
                a.work,
                a.blocked,
            )
        })
        .collect();
    acts.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(Observed {
        from: from.into(),
        to: to.into(),
        units: r.units,
        total: r.total.mean,
        half_width: r.total.half_width,
        activities: acts,
        paths: r.paths.into_iter().take(5).collect(),
        warnings: r.warnings,
    })
}

/// An analysis of the model kept in the journal (its result as JSON).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelAnalysis {
    pub id: Id,
    pub kind: AnalysisKind,
    pub name: String,
    pub model_hash: String,
    pub result: Value,
    pub replications: Option<u32>,
    pub date: Option<Date>,
}

impl ModelAnalysis {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            kind: e.typed(fields::KIND).unwrap_or_default(),
            name: e.text(fields::NAME),
            model_hash: e.text(fields::MODEL_HASH),
            result: e.get(fields::RESULT).cloned().unwrap_or(Value::Null),
            replications: e.integer(fields::REPLICATIONS).map(|n| n.max(0) as u32),
            date: e.opt_text(fields::DATE).and_then(|s| s.parse().ok()),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Analysis, id)
    }
}

/// The analyses of the model (bottleneck, shifting bottleneck, observed PERT) of a project.
pub fn model_analyses(state: &State, project: Id) -> Vec<ModelAnalysis> {
    let key = project.to_string();
    let mut v: Vec<ModelAnalysis> = state
        .of_kind(Kind::Analysis)
        .filter(|(_, e)| !e.is_deleted() && e.text(fields::PROJECT) == key)
        .filter(|(_, e)| e.typed::<AnalysisKind>(fields::KIND).is_some())
        .map(|(id, e)| ModelAnalysis::read(id, e))
        .collect();
    v.sort_by_key(|a| (a.date, a.id));
    v
}

#[allow(clippy::too_many_arguments)]
pub fn record_analysis(
    project: Id,
    id: Id,
    kind: AnalysisKind,
    name: &str,
    model_hash: &str,
    result: Value,
    replications: u32,
    date: Date,
) -> Vec<Edit> {
    let r = ModelAnalysis::entity_ref(id);
    vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::KIND, enum_value(kind)),
        Edit::new(r, fields::NAME, name),
        Edit::new(r, fields::MODEL_HASH, model_hash),
        Edit::new(r, fields::RESULT, result),
        Edit::new(r, fields::REPLICATIONS, replications),
        Edit::new(r, fields::DATE, date.to_string()),
    ]
}

/// The names of the model's objects (for the "from" and "to" of the observed PERT).
pub fn object_names(model: &Model) -> Vec<String> {
    model.objects.iter().map(|o| o.name.clone()).collect()
}

#[cfg(test)]
mod tests;
