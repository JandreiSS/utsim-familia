//! The program folder (`docs/plano_lean_seis_sigma.md`, 8.9): one more folder with the
//! journal of S1, whose main entity is the `programa`, next to the portfolio and project
//! folders.
//!
//! ```text
//! Programa LSS da empresa.lss/
//!   projeto.json                 manifest (main entity: programa)
//!   ops/ presenca/               as in a project
//!   mentoria/<belt>.lss/         coaching log of each belt (restricted: MBB and the belt)
//!   formacao/questoes.lss/       question bank with the keys (restricted: MBB, instructors)
//!   formacao/notas.lss/          grades (restricted: MBB, instructors)
//!   formacao/turmas/<turma>.lss/ papers without the key and the trainees' answers
//!   carteiras/<unidade>/         optional: portfolio and project folders of a unit
//! ```
//!
//! The portfolio and project folders are looked for next to the program folder, in the
//! folders beside it (one per unit, for example) and under `carteiras/`; the projects of a
//! folder belong to the portfolio of the same folder. The program reads only the summary
//! of each one ([`utsim_lss_core::programa::record`]).
//!
//! Each restricted folder has its own journal, so sharing a folder on the Drive with only
//! the people who may read it works without exposing its operations in the program's
//! journal. [`redact_person`] and [`redact_coaching`] rewrite the history of personal
//! fields after an anonymisation (decision MB8): it is the one case where an operation
//! file is changed in place, keeping its id.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use utsim_lss_core::programa::{self, IdeaRecord, ProjectRecord, Unit};
use utsim_lss_core::{Date, Id, Kind, Op, State, designacao, format, privacidade};

use crate::project::{self, ProjectFolder, SNAPSHOTS};
use crate::user::Identity;
use crate::{Result, fsio};

/// Subfolder of the coaching logs.
pub const COACHING: &str = "mentoria";
/// Optional subfolder of the units' portfolios.
pub const PORTFOLIOS: &str = "carteiras";
/// Subfolder of the training (stage L7).
pub const TRAINING: &str = "formacao";

/// The restricted folders of the training (`docs/plano_lean_seis_sigma.md`, 8.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrainingFolder {
    /// The question bank with the answer keys.
    Questions,
    /// The grades.
    Grades,
    /// A class: the papers without the key and the trainees' answers.
    Class(Id),
}

impl TrainingFolder {
    /// The key written in the folder's main entity.
    pub fn key(self) -> &'static str {
        match self {
            TrainingFolder::Questions => "questoes",
            TrainingFolder::Grades => "notas",
            TrainingFolder::Class(_) => "turma",
        }
    }

    pub fn title(self) -> String {
        match self {
            TrainingFolder::Questions => "Banco de questões".into(),
            TrainingFolder::Grades => "Notas".into(),
            TrainingFolder::Class(c) => format!("Turma {}", c.short()),
        }
    }
}

/// Where a training folder lives.
pub fn training_path(program_root: &Path, f: TrainingFolder) -> PathBuf {
    let base = program_root.join(TRAINING);
    match f {
        TrainingFolder::Questions => base.join(format!("questoes.{}", project::EXTENSION)),
        TrainingFolder::Grades => base.join(format!("notas.{}", project::EXTENSION)),
        TrainingFolder::Class(c) => base
            .join("turmas")
            .join(format!("{c}.{}", project::EXTENSION)),
    }
}

/// Subfolder of the certificates in PDF.
pub const CERTIFICATES: &str = "certificacoes";

/// Writes a certificate's PDF into `certificacoes/` (made again at any time).
pub fn write_certificate(program_root: &Path, number: u32, bytes: &[u8]) -> Result<PathBuf> {
    let dir = program_root.join(CERTIFICATES);
    fs::create_dir_all(&dir).map_err(|e| crate::Error::io(&dir, e))?;
    let target = dir.join(format!("certificado-{number:04}.pdf"));
    fsio::write_replace(&target, bytes)?;
    Ok(target)
}

/// Where the coaching log of a belt lives.
pub fn coaching_path(program_root: &Path, belt: Id) -> PathBuf {
    program_root
        .join(COACHING)
        .join(format!("{belt}.{}", project::EXTENSION))
}

/// A portfolio folder found by the scan.
#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioFound {
    pub path: PathBuf,
    pub id: Id,
    pub name: String,
    /// The portfolio's own unit (in its journal).
    pub local_unit: Option<Id>,
    /// The program unit it belongs to.
    pub unit: Option<Id>,
}

/// What the program found around it.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub portfolios: Vec<PortfolioFound>,
    pub projects: Vec<(PathBuf, ProjectRecord)>,
    pub ideas: Vec<IdeaRecord>,
    /// Folders that could not be opened (still syncing, or no access).
    pub unreadable: Vec<PathBuf>,
}

impl Scan {
    pub fn records(&self) -> Vec<ProjectRecord> {
        self.projects.iter().map(|(_, r)| r.clone()).collect()
    }

    pub fn path_of(&self, project: Id) -> Option<&Path> {
        self.projects
            .iter()
            .find(|(_, r)| r.id == project)
            .map(|(p, _)| p.as_path())
    }

    pub fn portfolio_path(&self, portfolio: Id) -> Option<&Path> {
        self.portfolios
            .iter()
            .find(|p| p.id == portfolio)
            .map(|p| p.path.as_path())
    }
}

/// Folders holding `.lss` folders: `dir` itself and its subfolders up to `depth`, never
/// descending into a project folder.
fn folder_groups(dir: &Path, depth: usize, skip: &Path, out: &mut Vec<(PathBuf, Vec<PathBuf>)>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut subdirs: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    subdirs.sort();
    let (folders, plain): (Vec<PathBuf>, Vec<PathBuf>) =
        subdirs.into_iter().partition(|p| project::is_project(p));
    let folders: Vec<PathBuf> = folders.into_iter().filter(|p| p != skip).collect();
    if !folders.is_empty() {
        out.push((dir.to_path_buf(), folders));
    }
    if depth > 0 {
        for d in plain {
            folder_groups(&d, depth - 1, skip, out);
        }
    }
}

/// Looks for the portfolios and projects of the program and reads their summaries.
pub fn scan(
    program_root: &Path,
    program_state: &State,
    program: Id,
    me: &Identity,
    cache: Option<&Path>,
    offset_minutes: i32,
) -> Scan {
    let units: Vec<Unit> = programa::units(program_state, program);
    let mut groups = Vec::new();
    if let Some(parent) = program_root.parent() {
        folder_groups(parent, 2, program_root, &mut groups);
    }
    folder_groups(&program_root.join(PORTFOLIOS), 2, program_root, &mut groups);
    let mut out = Scan::default();
    for (_, folders) in groups {
        let mut opened: Vec<(PathBuf, ProjectFolder)> = Vec::new();
        for f in folders {
            match ProjectFolder::open(&f, me, cache) {
                Ok(pf) => opened.push((f, pf)),
                Err(_) => out.unreadable.push(f),
            }
        }
        // The portfolio of this folder (the first one, by name).
        let portfolio = opened
            .iter()
            .find(|(_, pf)| pf.is_portfolio())
            .map(|(p, pf)| {
                let id = pf.project_id();
                let local = utsim_lss_core::portfolio::read(pf.state(), id).and_then(|x| x.unit);
                PortfolioFound {
                    path: p.clone(),
                    id,
                    name: pf.title(),
                    local_unit: local,
                    unit: programa::unit_of_portfolio(&units, id, local),
                }
            });
        if let Some(pfound) = &portfolio {
            if let Some((_, pf)) = opened.iter().find(|(p, _)| *p == pfound.path) {
                out.ideas.extend(programa::idea_records(
                    pf.state(),
                    pfound.id,
                    pfound.unit,
                    offset_minutes,
                ));
            }
        }
        for (path, pf) in &opened {
            if pf.main_kind() != Some(Kind::Project) {
                continue;
            }
            let folder = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let created = Some(Date::from_unix_ms(pf.manifest().created_ms, offset_minutes));
            if let Some(r) = programa::record(
                pf.state(),
                pf.project_id(),
                &folder,
                created,
                portfolio.as_ref().map(|p| p.id),
                portfolio.as_ref().and_then(|p| p.unit),
            ) {
                out.projects.push((path.clone(), r));
            }
        }
        if let Some(p) = portfolio {
            out.portfolios.push(p);
        }
    }
    out
}

/// Rewrites the operation files of `root` for which `new_value` gives a value (keeping
/// the operation's id, author and clock), and removes the snapshots, which hold the old
/// values. Returns how many files changed.
pub fn rewrite_ops(root: &Path, new_value: impl Fn(&Op) -> Option<Value>) -> Result<usize> {
    let mut n = 0;
    for f in fsio::json_files_deep(&root.join(project::OPS), 2) {
        let Ok(text) = fsio::read_text(&f) else {
            continue;
        };
        let Ok(mut op) = format::read_op(&text) else {
            continue;
        };
        if let Some(v) = new_value(&op) {
            if v != op.value {
                op.value = v;
                fsio::write_replace(&f, format::write(&op).as_bytes())?;
                n += 1;
            }
        }
    }
    for s in fsio::json_files(&root.join(SNAPSHOTS)) {
        let _ = fs::remove_file(s);
    }
    Ok(n)
}

/// Rewrites the history of the personal fields of `person` in the program folder (and the
/// reasons of their absences).
pub fn redact_person(program_root: &Path, state: &State, person: Id) -> Result<usize> {
    let absences: Vec<Id> = designacao::absences(state)
        .into_iter()
        .filter(|a| a.person == Some(person))
        .map(|a| a.id)
        .collect();
    rewrite_ops(program_root, |op| {
        if op.entity.id == person
            && op.entity.kind == Kind::Person
            && privacidade::PERSONAL_FIELDS.contains(&op.field.as_str())
        {
            Some(privacidade::redacted_value(&op.field, person))
        } else if absences.contains(&op.entity.id) && op.field == designacao::fields::REASON {
            Some(Value::from(""))
        } else {
            None
        }
    })
}

/// Rewrites the history of the free texts of a coaching log and of the personal fields of
/// `person` registered in it.
pub fn redact_coaching(log_root: &Path, person: Id) -> Result<usize> {
    rewrite_ops(log_root, |op| {
        if matches!(op.entity.kind, Kind::CoachingSession | Kind::Commitment)
            && privacidade::COACHING_TEXT_FIELDS.contains(&op.field.as_str())
        {
            Some(Value::from(""))
        } else if op.entity.kind == Kind::Person
            && op.entity.id == person
            && privacidade::PERSONAL_FIELDS.contains(&op.field.as_str())
        {
            Some(privacidade::redacted_value(&op.field, person))
        } else {
            None
        }
    })
}

/// Why an entry from UTSim Ensino was not recorded.
#[derive(Debug, thiserror::Error)]
pub enum TeachingError {
    #[error("a pasta não é de um programa Lean Seis Sigma")]
    NotAProgram,
    #[error(
        "{0} não está no cadastro deste programa: abra o programa uma vez no UTSim Seis Sigma ou peça ao master black belt para incluir você"
    )]
    NotAMember(String),
    #[error("o UTSim Ensino só registra os níveis \"estudou\" e \"praticou\"")]
    Level,
    #[error("ferramenta desconhecida no mapa de competências: {0}")]
    Tool(String),
    #[error(transparent)]
    Store(#[from] crate::Error),
    #[error("{0}")]
    Denied(String),
}

/// Whether `me` can record from UTSim Ensino in the program at `program_root`: the folder
/// opens, is a program, and `me` is in its people list. Writes nothing.
pub fn teaching_check(
    program_root: &Path,
    me: &Identity,
) -> std::result::Result<(), TeachingError> {
    let pf = ProjectFolder::open(program_root, me, None)?;
    if !pf.is_program() {
        return Err(TeachingError::NotAProgram);
    }
    if !programa::members(pf.state())
        .iter()
        .any(|m| m.id == me.person)
    {
        return Err(TeachingError::NotAMember(me.name.clone()));
    }
    Ok(())
}

/// Records in the program at `program_root`, as `me`, the level of a tool reached in the
/// final case of UTSim Ensino (origin `ensino`, by the person themselves). The person must
/// be in the program's people list; the level is `estudou` or `praticou`; the tool is a key
/// of [`utsim_lss_core::mentoria::all_tools`]. Returns the id of the entry; when the same
/// entry is already there (same person, tool, level, origin `ensino` and evidence),
/// nothing is written and its id is returned.
pub fn record_teaching(
    program_root: &Path,
    me: &Identity,
    clock: &dyn crate::Clock,
    tool: &str,
    level: utsim_lss_core::mentoria::Level,
    evidence: &str,
) -> std::result::Result<Id, TeachingError> {
    use utsim_lss_core::mentoria;
    if !mentoria::teaching_level(level) {
        return Err(TeachingError::Level);
    }
    if !mentoria::all_tools().iter().any(|t| t == tool) {
        return Err(TeachingError::Tool(tool.to_string()));
    }
    let mut pf = ProjectFolder::open(program_root, me, None)?;
    if !pf.is_program() {
        return Err(TeachingError::NotAProgram);
    }
    if !programa::members(pf.state())
        .iter()
        .any(|m| m.id == me.person)
    {
        return Err(TeachingError::NotAMember(me.name.clone()));
    }
    if let Some(old) = mentoria::entries(pf.state()).into_iter().find(|e| {
        e.person == Some(me.person)
            && e.tool == tool
            && e.level == level
            && e.source == mentoria::Source::Teaching
            && e.evidence.trim() == evidence.trim()
    }) {
        return Ok(old.id);
    }
    let id = clock.new_id();
    let today = Date::from_unix_ms(clock.now_ms(), clock.utc_offset_minutes());
    let edits =
        mentoria::record_teaching(pf.project_id(), id, me.person, tool, level, evidence, today)
            .ok_or(TeachingError::Level)?;
    let founder = pf.manifest().created_by;
    utsim_lss_core::perfis::check_edits(pf.state(), founder, me.person, &edits)
        .map_err(|d| TeachingError::Denied(d.to_string()))?;
    pf.commit(edits, clock)?;
    Ok(id)
}
