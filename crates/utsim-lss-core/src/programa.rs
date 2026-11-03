//! The implementation program (`docs/plano_lean_seis_sigma.md`, 8.1; decisions MB1 and
//! MB9): `Programa → UnidadeNegocio → Carteira → Projeto`.
//!
//! The program is a folder with its own journal (the mechanism of S1), next to the
//! portfolio and project folders. Its journal holds the program entity, the business
//! units (each linked to one portfolio), the weighted strategic objectives deployed per
//! unit, the yearly targets per unit, the program's people (level, capacity, consent),
//! the opportunities sent to the selection committee and the committee's decisions.
//!
//! What the program shows of the portfolios and projects comes from [`ProjectRecord`] and
//! [`IdeaRecord`], small summaries read from each folder's state ([`record`],
//! [`idea_records`]); everything here is a pure function over those and the program state,
//! so a synthetic program with known counts tests every number.
//!
//! The official scoreboard counts **only validated savings** (validated monthly entries of
//! section 1.10); forecasts appear in a column of their own and never in the realised
//! total. Training projects (`treino`, section 8.6) never count (LSS-RNF-15).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::domain::{
    Approach, PhaseStatus, ProjectStatus, RoleKind, SavingStatus, enum_value, fields as dfields,
    phases,
};
use crate::id::Id;
use crate::meeting::{self, ItemKind, MeetingKind};
use crate::op::{Edit, EntityRef, Kind};
use crate::portfolio;
use crate::savings::{self, Category, Totals};
use crate::state::{Entity, State};
use crate::tollgate::{self, TollgateStatus};
use crate::tools::{self, DeliverableStatus, Tool};

pub mod fields {
    pub const PROGRAM: &str = "programa";
    pub const NAME: &str = "nome";
    // Business unit.
    pub const CHAMPION: &str = "champion";
    pub const SPONSOR: &str = "patrocinador";
    /// People of the unit, given by the company (for the coverage).
    pub const HEADCOUNT: &str = "pessoas";
    /// The portfolio (manifest id of its folder) of the unit.
    pub const PORTFOLIO: &str = "carteira";
    // Objective: weight, and per unit `peso_unidade_<unit>`.
    pub const WEIGHT: &str = "peso";
    pub const DEPLOY_PREFIX: &str = "peso_unidade_";
    // Target.
    pub const UNIT: &str = "unidade";
    pub const YEAR: &str = "ano";
    pub const METRIC: &str = "indicador";
    pub const LEVEL: &str = "nivel";
    pub const VALUE: &str = "valor";
    // Person of the program.
    pub const EMAIL: &str = "email";
    pub const AREA: &str = "area";
    pub const HOURS_WEEK: &str = "horas_semana";
    pub const PROJECT_LIMIT: &str = "limite_projetos";
    pub const NEEDS_CERTIFICATION: &str = "precisa_certificacao";
    pub const TRAINED_YEAR: &str = "formado_em";
    pub const CERTIFIED_ON: &str = "certificado_em";
    pub const CONSENT: &str = "consentimento";
    pub const PURPOSE: &str = "finalidade";
    pub const CONSENT_ON: &str = "consentimento_em";
    pub const ANONYMISED: &str = "anonimizado";
    // Opportunity sent to the committee.
    pub const IDEA: &str = "ideia";
    pub const SENT_ON: &str = "enviada_em";
    pub const SENT_BY: &str = "enviada_por";
    pub const DECISION: &str = "decisao";
    pub const REASON: &str = "motivo";
    pub const MEETING: &str = "reuniao";
    pub const BELT: &str = "belt";
    pub const TITLE: &str = "titulo";
    pub const SCORE: &str = "pontuacao";
    // Committee decision (besides the above).
    pub const OPPORTUNITY: &str = "oportunidade";
    pub const DATE: &str = "data";
    pub const BY: &str = "por";
    // Program.
    pub const COMMITTEE: &str = "comite";
    pub const CADENCE: &str = "cadencia_comite_dias";
    /// On a project: a training project (section 8.6), out of every indicator.
    pub const TRAINING: &str = "treino";
}

/// Belt level of a person (L7 will set it from the certification; the MBB may set it).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum BeltLevel {
    #[serde(rename = "yellow_belt")]
    Yellow,
    #[default]
    #[serde(rename = "green_belt")]
    Green,
    #[serde(rename = "black_belt")]
    Black,
    #[serde(rename = "master_black_belt")]
    MasterBlack,
}

impl BeltLevel {
    pub const ALL: [BeltLevel; 4] = [
        BeltLevel::Yellow,
        BeltLevel::Green,
        BeltLevel::Black,
        BeltLevel::MasterBlack,
    ];
}

/// The indicators a unit gets a yearly target for.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Metric {
    /// Validated hard savings in the year.
    #[default]
    #[serde(rename = "ganho_hard_validado")]
    HardValidated,
    /// Validated soft savings in the year.
    #[serde(rename = "ganho_soft_validado")]
    SoftValidated,
    #[serde(rename = "projetos_encerrados")]
    ProjectsClosed,
    /// Belts of a level trained in the year.
    #[serde(rename = "belts_formados")]
    BeltsTrained,
    /// Belts of a level certified in the year.
    #[serde(rename = "belts_certificados")]
    BeltsCertified,
    /// Belts (green or above) per 100 people of the unit.
    #[serde(rename = "cobertura")]
    Coverage,
}

impl Metric {
    pub const ALL: [Metric; 6] = [
        Metric::HardValidated,
        Metric::SoftValidated,
        Metric::ProjectsClosed,
        Metric::BeltsTrained,
        Metric::BeltsCertified,
        Metric::Coverage,
    ];

    /// Whether the target is per belt level.
    pub fn per_level(self) -> bool {
        matches!(self, Metric::BeltsTrained | Metric::BeltsCertified)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub id: Id,
    pub name: String,
    /// Days between selection committees (monthly by default).
    pub cadence_days: i64,
}

pub const DEFAULT_CADENCE_DAYS: i64 = 30;

pub fn read(state: &State, id: Id) -> Option<Program> {
    let e = state.entity(id).filter(|e| e.kind == Kind::Program)?;
    Some(Program {
        id,
        name: e.text(fields::NAME),
        cadence_days: e
            .integer(fields::CADENCE)
            .filter(|d| *d > 0)
            .unwrap_or(DEFAULT_CADENCE_DAYS),
    })
}

/// Edits that create the program entity.
pub fn create(program: Id, name: &str) -> Vec<Edit> {
    let r = EntityRef::new(Kind::Program, program);
    vec![
        Edit::new(r, fields::NAME, name.trim()),
        Edit::new(r, fields::CADENCE, DEFAULT_CADENCE_DAYS),
    ]
}

fn of_program<T>(
    state: &State,
    kind: Kind,
    program: Id,
    read: impl Fn(Id, &Entity) -> T,
) -> Vec<T> {
    let key = program.to_string();
    state
        .of_kind(kind)
        .filter(|(_, e)| e.get(fields::PROGRAM).and_then(Value::as_str) == Some(key.as_str()))
        .map(|(id, e)| read(id, e))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: Id,
    pub name: String,
    pub champion: Option<Id>,
    pub sponsor: Option<Id>,
    pub headcount: Option<f64>,
    pub portfolio: Option<Id>,
}

impl Unit {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            name: e.text(fields::NAME),
            champion: e.id(fields::CHAMPION),
            sponsor: e.id(fields::SPONSOR),
            headcount: e.number(fields::HEADCOUNT).filter(|n| *n > 0.0),
            portfolio: e.id(fields::PORTFOLIO),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::BusinessUnit, id)
    }
}

pub fn units(state: &State, program: Id) -> Vec<Unit> {
    let mut v = of_program(state, Kind::BusinessUnit, program, Unit::read);
    v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    v
}

pub fn add_unit(program: Id, id: Id, name: &str) -> Vec<Edit> {
    let r = Unit::entity_ref(id);
    vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::NAME, name.trim()),
    ]
}

/// The program unit of a portfolio: the unit linked to it (`carteira`), else the unit
/// whose id is the portfolio's own unit (a portfolio created inside the program).
pub fn unit_of_portfolio(units: &[Unit], portfolio: Id, local_unit: Option<Id>) -> Option<Id> {
    units
        .iter()
        .find(|u| u.portfolio == Some(portfolio))
        .or_else(|| units.iter().find(|u| Some(u.id) == local_unit))
        .map(|u| u.id)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    pub id: Id,
    pub name: String,
    pub weight: f64,
    /// Weight of the objective in each unit (absent: not deployed there).
    pub deployment: BTreeMap<Id, f64>,
}

impl Objective {
    pub fn read(id: Id, e: &Entity) -> Self {
        let deployment = e
            .fields
            .iter()
            .filter_map(|(k, v)| {
                let unit = k.strip_prefix(fields::DEPLOY_PREFIX)?;
                let unit = id_from_key(unit)?;
                let w = v.value.as_f64().filter(|w| w.is_finite() && *w > 0.0)?;
                Some((unit, w))
            })
            .collect();
        Self {
            id,
            name: e.text(fields::NAME),
            weight: e.number(fields::WEIGHT).unwrap_or(1.0).max(0.0),
            deployment,
        }
    }
}

/// The id from a field key ([`portfolio::id_key`]: hex without dashes).
pub fn id_from_key(key: &str) -> Option<Id> {
    if key.len() != 32 {
        return None;
    }
    let s = format!(
        "{}-{}-{}-{}-{}",
        &key[0..8],
        &key[8..12],
        &key[12..16],
        &key[16..20],
        &key[20..32]
    );
    s.parse().ok()
}

pub fn deploy_field(unit: Id) -> String {
    format!("{}{}", fields::DEPLOY_PREFIX, portfolio::id_key(unit))
}

pub fn objectives(state: &State, program: Id) -> Vec<Objective> {
    let mut v = of_program(state, Kind::StrategicObjective, program, Objective::read);
    v.sort_by(|a, b| b.weight.total_cmp(&a.weight).then(a.id.cmp(&b.id)));
    v
}

pub fn add_objective(program: Id, id: Id, name: &str, weight: f64) -> Vec<Edit> {
    let r = EntityRef::new(Kind::StrategicObjective, id);
    vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::NAME, name.trim()),
        Edit::new(r, fields::WEIGHT, weight.max(0.0)),
    ]
}

/// Edit that deploys an objective to a unit with a weight (0 removes it).
pub fn deploy(objective: Id, unit: Id, weight: f64) -> Edit {
    let v = if weight > 0.0 {
        Value::from(weight)
    } else {
        Value::Null
    };
    Edit::new(
        EntityRef::new(Kind::StrategicObjective, objective),
        deploy_field(unit),
        v,
    )
}

/// The objectives deployed to a unit with their weights normalised to sum 1: the program
/// weight times the unit weight. Empty when nothing is deployed there.
pub fn unit_objectives(objectives: &[Objective], unit: Id) -> Vec<(Id, f64)> {
    let raw: Vec<(Id, f64)> = objectives
        .iter()
        .filter_map(|o| o.deployment.get(&unit).map(|w| (o.id, o.weight * w)))
        .filter(|(_, w)| *w > 0.0)
        .collect();
    let total: f64 = raw.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    raw.into_iter().map(|(id, w)| (id, w / total)).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub id: Id,
    pub unit: Option<Id>,
    pub year: i32,
    pub metric: Metric,
    pub level: Option<BeltLevel>,
    pub value: f64,
}

impl Target {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            unit: e.id(fields::UNIT),
            year: e.integer(fields::YEAR).unwrap_or(0) as i32,
            metric: e.typed(fields::METRIC).unwrap_or_default(),
            level: e.typed(fields::LEVEL),
            value: e.number(fields::VALUE).unwrap_or(0.0),
        }
    }
}

pub fn targets(state: &State, program: Id) -> Vec<Target> {
    let mut v = of_program(state, Kind::ProgramTarget, program, Target::read);
    v.sort_by(|a, b| {
        (a.year, a.unit, a.metric, a.level)
            .cmp(&(b.year, b.unit, b.metric, b.level))
            .then(a.id.cmp(&b.id))
    });
    v
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProgramError {
    #[error("a meta precisa de um valor maior que zero")]
    BadTarget,
    #[error("escolha o nível de belt da meta")]
    NeedsLevel,
    #[error("só uma ideia em triagem vai ao comitê de seleção")]
    NotInScreening,
    #[error("esta ideia já está na pauta do comitê")]
    AlreadySent,
    #[error("registre o motivo da decisão do comitê")]
    NoReason,
    #[error("oportunidade não encontrada")]
    NotFound,
    #[error("a decisão desta oportunidade já foi tomada")]
    Decided,
}

/// Edits that set a yearly target of a unit (a new one, or replacing the value of the
/// existing target for the same unit, year, metric and level).
#[allow(clippy::too_many_arguments)]
pub fn set_target(
    state: &State,
    program: Id,
    id: Id,
    unit: Id,
    year: i32,
    metric: Metric,
    level: Option<BeltLevel>,
    value: f64,
) -> Result<Vec<Edit>, ProgramError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(ProgramError::BadTarget);
    }
    if metric.per_level() && level.is_none() {
        return Err(ProgramError::NeedsLevel);
    }
    let level = if metric.per_level() { level } else { None };
    if let Some(t) = targets(state, program)
        .into_iter()
        .find(|t| t.unit == Some(unit) && t.year == year && t.metric == metric && t.level == level)
    {
        return Ok(vec![Edit::new(
            EntityRef::new(Kind::ProgramTarget, t.id),
            fields::VALUE,
            value,
        )]);
    }
    let r = EntityRef::new(Kind::ProgramTarget, id);
    let mut e = vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::UNIT, unit.to_string()),
        Edit::new(r, fields::YEAR, year),
        Edit::new(r, fields::METRIC, enum_value(metric)),
        Edit::new(r, fields::VALUE, value),
    ];
    if let Some(l) = level {
        e.push(Edit::new(r, fields::LEVEL, enum_value(l)));
    }
    Ok(e)
}

/// A person of the program (the people list kept by the MBB, `pessoas/`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Member {
    pub id: Id,
    pub name: String,
    pub email: String,
    pub level: Option<BeltLevel>,
    pub unit: Option<Id>,
    pub area: String,
    /// Hours per week dedicated to the program.
    pub hours_week: Option<f64>,
    /// Simultaneous projects (overrides the default of the level).
    pub project_limit: Option<u32>,
    pub needs_certification: bool,
    pub trained_year: Option<i32>,
    pub certified_on: Option<Date>,
    pub consent: bool,
    pub purpose: String,
    pub consent_on: Option<Date>,
    pub anonymised: bool,
}

impl Member {
    pub fn read(id: Id, e: &Entity) -> Self {
        let f = |k| e.opt_text(k).and_then(|s| s.parse::<Date>().ok());
        Self {
            id,
            name: e.text(dfields::person::NAME),
            email: e.text(dfields::person::EMAIL),
            level: e.typed(fields::LEVEL),
            unit: e.id(fields::UNIT),
            area: e.text(fields::AREA),
            hours_week: e.number(fields::HOURS_WEEK).filter(|h| *h >= 0.0),
            project_limit: e
                .integer(fields::PROJECT_LIMIT)
                .filter(|n| *n > 0)
                .map(|n| n.min(50) as u32),
            needs_certification: e.boolean(fields::NEEDS_CERTIFICATION),
            trained_year: e.integer(fields::TRAINED_YEAR).map(|y| y as i32),
            certified_on: f(fields::CERTIFIED_ON),
            consent: e.boolean(fields::CONSENT),
            purpose: e.text(fields::PURPOSE),
            consent_on: f(fields::CONSENT_ON),
            anonymised: e.boolean(fields::ANONYMISED),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Person, id)
    }

    /// A belt: has a level.
    pub fn is_belt(&self) -> bool {
        self.level.is_some()
    }
}

/// Every person of the program journal, by name.
pub fn members(state: &State) -> Vec<Member> {
    let mut v: Vec<Member> = state
        .of_kind(Kind::Person)
        .map(|(id, e)| Member::read(id, e))
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    v
}

// --- What the program reads of each folder ------------------------------------------

/// A tollgate of a project, as the program sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct TollgateRecord {
    pub id: Id,
    pub phase: Id,
    pub phase_name: String,
    pub phase_index: usize,
    pub status: TollgateStatus,
    pub round: i64,
    pub planned: Option<Date>,
    pub submitted_on: Option<Date>,
    /// Date of the decision that approved it (the final decision).
    pub approved_on: Option<Date>,
    /// The verified checklist items and whether the app finds them satisfied.
    pub verified: Vec<(String, bool)>,
}

/// A tool deliverable of a project.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUse {
    pub tool: Tool,
    pub deliverable: Id,
    pub phase_index: usize,
    /// The deliverable is done and its phase approved at the tollgate.
    pub approved: bool,
}

/// The summary of a project folder the program reads (it never opens more than this).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectRecord {
    pub id: Id,
    pub title: String,
    /// Folder name, to open it from the program.
    pub folder: String,
    pub portfolio: Option<Id>,
    /// The program unit (from its portfolio).
    pub unit: Option<Id>,
    pub approach: Approach,
    pub status: ProjectStatus,
    pub phase: String,
    pub process: String,
    pub area: String,
    pub training: bool,
    pub belts: Vec<Id>,
    pub champions: Vec<Id>,
    pub sponsors: Vec<Id>,
    pub opened_on: Option<Date>,
    pub closed_on: Option<Date>,
    pub totals: Totals,
    /// Validated monthly entries by year and category: the only official savings.
    pub validated_by_year: BTreeMap<(i32, Category), f64>,
    pub tollgates: Vec<TollgateRecord>,
    pub tools: Vec<ToolUse>,
    /// Hours per week of the open schedule tasks, by person (field `horas_semana` of a
    /// task).
    pub task_hours: Vec<(Id, f64)>,
}

impl ProjectRecord {
    pub fn is_running(&self) -> bool {
        matches!(
            self.status,
            ProjectStatus::Running | ProjectStatus::Suspended
        )
    }

    /// Official validated savings of a category in a year.
    pub fn validated(&self, year: i32, c: Category) -> f64 {
        self.validated_by_year
            .get(&(year, c))
            .copied()
            .unwrap_or(0.0)
    }
}

/// Field of a schedule task: hours per week the responsible spends while it is open.
pub const TASK_HOURS_WEEK: &str = "horas_semana";

/// Reads the summary of a project from its state. `created` is the folder's creation date
/// (the manifest), `portfolio` the portfolio folder next to it and `unit` its program unit.
pub fn record(
    state: &State,
    project: Id,
    folder: &str,
    created: Option<Date>,
    portfolio: Option<Id>,
    unit: Option<Id>,
) -> Option<ProjectRecord> {
    let sum = portfolio::summary(state, project)?;
    let e = state.entity(project)?;
    let ph = phases(state, project);
    let phase_index = |id: Id| ph.iter().position(|p| p.id == id).unwrap_or(usize::MAX);
    let mut tgs = Vec::new();
    for (tid, te) in state.of_kind(Kind::Tollgate) {
        let tg = tollgate::Tollgate::read(tid, te);
        let Some(i) = ph.iter().position(|p| p.id == tg.phase) else {
            continue;
        };
        let approved_on = tg
            .final_decision
            .and_then(|d| state.entity(d))
            .map(|d| tollgate::DecisionRecord::read(tg.final_decision.unwrap_or(Id::NIL), d))
            .filter(|d| d.decision.is_some_and(|x| x.approves()))
            .and_then(|d| d.date);
        let planned = te
            .opt_text(tollgate::fields::PLANNED)
            .and_then(|s| s.parse().ok());
        let verified = tollgate::evaluate(state, project, &tg)
            .into_iter()
            .filter(|r| r.item.item_type == tollgate::ItemType::Verified)
            .map(|r| (r.item.text, r.ok))
            .collect();
        tgs.push(TollgateRecord {
            id: tid,
            phase: tg.phase,
            phase_name: ph[i].name.clone(),
            phase_index: i,
            status: tg.status,
            round: tg.round,
            planned,
            submitted_on: tg.submitted_on,
            approved_on: if tg.status == TollgateStatus::Approved {
                approved_on
            } else {
                None
            },
            verified,
        });
    }
    tgs.sort_by_key(|t| (t.phase_index, t.id));
    let tools_used = tools::deliverables(state, project)
        .into_iter()
        .filter_map(|d| {
            let tool = d.tool?;
            let i = d.phase.map_or(usize::MAX, phase_index);
            let phase_ok = d
                .phase
                .and_then(|p| ph.iter().find(|x| x.id == p))
                .is_some_and(|p| p.status == PhaseStatus::Approved);
            Some(ToolUse {
                tool,
                deliverable: d.id,
                phase_index: i,
                approved: phase_ok && d.status == DeliverableStatus::Done,
            })
        })
        .collect();
    let mut validated_by_year: BTreeMap<(i32, Category), f64> = BTreeMap::new();
    for p in savings::plans(state, project) {
        let c = Category::from(p.saving.category);
        for en in savings::entries(state, p.saving.id) {
            if let (true, Some(m)) = (en.validated, en.month) {
                *validated_by_year.entry((m.year, c)).or_default() += en.value;
            }
        }
    }
    let mut hours: BTreeMap<Id, f64> = BTreeMap::new();
    for (_, te) in state.of_kind(Kind::Task) {
        if te.id(crate::schedule::fields::PROJECT) != Some(project) {
            continue;
        }
        let t = crate::schedule::Task::read(Id::NIL, te);
        if t.percent >= 100.0 {
            continue;
        }
        if let (Some(w), Some(h)) = (t.who, te.number(TASK_HOURS_WEEK)) {
            *hours.entry(w).or_default() += h.max(0.0);
        }
    }
    Some(ProjectRecord {
        id: project,
        title: sum.title,
        folder: folder.to_string(),
        portfolio,
        unit,
        approach: sum.approach,
        status: sum.status,
        phase: sum.phase,
        process: sum.process,
        area: e.text(dfields::project::AREA),
        training: e.boolean(fields::TRAINING),
        belts: sum.belts,
        champions: crate::roles::holders(state, project, RoleKind::Champion),
        sponsors: crate::roles::holders(state, project, RoleKind::Sponsor),
        opened_on: created,
        closed_on: e
            .opt_text(crate::closure::fields::CLOSED_ON)
            .and_then(|s| s.parse().ok()),
        totals: sum.savings,
        validated_by_year,
        tollgates: tgs,
        tools: tools_used,
        task_hours: hours.into_iter().collect(),
    })
}

/// An idea of a portfolio, as the program's pipeline sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct IdeaRecord {
    pub id: Id,
    pub portfolio: Id,
    pub unit: Option<Id>,
    pub title: String,
    pub status: ProjectStatus,
    /// Day the idea entered its current state.
    pub since: Option<Date>,
    /// Weighted score of the portfolio (0 to 100).
    pub score: f64,
    pub rough_gain: f64,
    pub approach: Option<Approach>,
    pub belt: Option<Id>,
    pub process: String,
    pub project: Option<Id>,
}

/// The ideas of a portfolio folder.
pub fn idea_records(
    state: &State,
    portfolio_id: Id,
    unit: Option<Id>,
    offset_minutes: i32,
) -> Vec<IdeaRecord> {
    let cs = portfolio::criteria(state, portfolio_id);
    let os = portfolio::objectives(state, portfolio_id);
    let mut v: Vec<IdeaRecord> = portfolio::ideas(state, portfolio_id)
        .into_iter()
        .map(|i| {
            let since = i
                .entity
                .fields
                .get(portfolio::fields::STATUS)
                .map(|f| Date::from_unix_ms(f.at_ms, offset_minutes));
            IdeaRecord {
                id: i.id,
                portfolio: portfolio_id,
                unit,
                title: i.title.clone(),
                status: i.status,
                since,
                score: portfolio::score(&i, &cs, &os).total,
                rough_gain: i.rough_gain,
                approach: i.approach,
                belt: i.belt,
                process: i.process.clone(),
                project: i.project,
            }
        })
        .collect();
    v.sort_by_key(|i| i.id);
    v
}

// --- Scoreboard -----------------------------------------------------------------------

/// One line of the scoreboard: target and realised of a metric in a unit (or the whole
/// program when `unit` is `None`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreRow {
    pub unit: Option<Id>,
    pub metric: Metric,
    pub level: Option<BeltLevel>,
    pub target: Option<f64>,
    /// The official realised value (savings: validated only).
    pub actual: f64,
    /// For savings: the forecast not yet validated, shown apart and never in `actual`.
    pub forecast: f64,
}

impl ScoreRow {
    /// Fraction of the target reached (`None` without a target).
    pub fn reached(&self) -> Option<f64> {
        self.target.filter(|t| *t > 0.0).map(|t| self.actual / t)
    }
}

fn in_unit(unit: Option<Id>, of: Option<Id>) -> bool {
    unit.is_none() || unit == of
}

/// Realised value of a metric in a unit (or all, `None`) in a year: `(official, forecast)`.
pub fn actual(
    metric: Metric,
    level: Option<BeltLevel>,
    unit: Option<Id>,
    year: i32,
    projects: &[ProjectRecord],
    members: &[Member],
    units: &[Unit],
) -> (f64, f64) {
    let real = || {
        projects
            .iter()
            .filter(move |p| !p.training && in_unit(unit, p.unit))
    };
    match metric {
        Metric::HardValidated | Metric::SoftValidated => {
            let c = if metric == Metric::HardValidated {
                Category::Hard
            } else {
                Category::Soft
            };
            let official: f64 = real().map(|p| p.validated(year, c)).sum();
            let forecast: f64 = real()
                .map(|p| p.totals.get(c, SavingStatus::Forecast))
                .sum();
            (official, (forecast - official).max(0.0))
        }
        Metric::ProjectsClosed => (
            real()
                .filter(|p| {
                    p.status == ProjectStatus::Closed && p.closed_on.is_some_and(|d| d.year == year)
                })
                .count() as f64,
            0.0,
        ),
        Metric::BeltsTrained => (
            members
                .iter()
                .filter(|m| in_unit(unit, m.unit) && !m.anonymised)
                .filter(|m| level.is_none_or(|l| m.level == Some(l)))
                .filter(|m| m.trained_year == Some(year))
                .count() as f64,
            0.0,
        ),
        Metric::BeltsCertified => (
            members
                .iter()
                .filter(|m| in_unit(unit, m.unit) && !m.anonymised)
                .filter(|m| level.is_none_or(|l| m.level == Some(l)))
                .filter(|m| m.certified_on.is_some_and(|d| d.year == year))
                .count() as f64,
            0.0,
        ),
        Metric::Coverage => {
            let people: f64 = units
                .iter()
                .filter(|u| in_unit(unit, Some(u.id)))
                .filter_map(|u| u.headcount)
                .sum();
            let belts = members
                .iter()
                .filter(|m| in_unit(unit, m.unit) && !m.anonymised)
                .filter(|m| m.level.is_some_and(|l| l >= BeltLevel::Green))
                .count() as f64;
            (
                if people > 0.0 {
                    100.0 * belts / people
                } else {
                    0.0
                },
                0.0,
            )
        }
    }
}

/// The scoreboard of a year: for every unit, one row per target, plus the rows of the
/// savings and closed projects even without target; then the program totals.
pub fn scoreboard(
    state: &State,
    program: Id,
    year: i32,
    projects: &[ProjectRecord],
    members: &[Member],
) -> Vec<ScoreRow> {
    let us = units(state, program);
    let ts: Vec<Target> = targets(state, program)
        .into_iter()
        .filter(|t| t.year == year)
        .collect();
    let mut rows = Vec::new();
    let mut keys: Vec<(Option<Id>, Metric, Option<BeltLevel>)> = Vec::new();
    for u in &us {
        for m in [
            Metric::HardValidated,
            Metric::SoftValidated,
            Metric::ProjectsClosed,
        ] {
            keys.push((Some(u.id), m, None));
        }
        for t in ts.iter().filter(|t| t.unit == Some(u.id)) {
            let k = (Some(u.id), t.metric, t.level);
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    let mut program_keys: Vec<(Metric, Option<BeltLevel>)> = vec![
        (Metric::HardValidated, None),
        (Metric::SoftValidated, None),
        (Metric::ProjectsClosed, None),
    ];
    for t in &ts {
        if !program_keys.contains(&(t.metric, t.level)) {
            program_keys.push((t.metric, t.level));
        }
    }
    for (unit, metric, level) in keys {
        let target = ts
            .iter()
            .find(|t| t.unit == unit && t.metric == metric && t.level == level)
            .map(|t| t.value);
        let (a, f) = actual(metric, level, unit, year, projects, members, &us);
        rows.push(ScoreRow {
            unit,
            metric,
            level,
            target,
            actual: a,
            forecast: f,
        });
    }
    for (metric, level) in program_keys {
        let mine: Vec<&Target> = ts
            .iter()
            .filter(|t| t.metric == metric && t.level == level)
            .collect();
        // Coverage is a ratio: the program target is not the sum of the units'.
        let target = if mine.is_empty() || metric == Metric::Coverage {
            None
        } else {
            Some(mine.iter().map(|t| t.value).sum())
        };
        let (a, f) = actual(metric, level, None, year, projects, members, &us);
        rows.push(ScoreRow {
            unit: None,
            metric,
            level,
            target,
            actual: a,
            forecast: f,
        });
    }
    rows
}

// --- Pipeline and selection committee -------------------------------------------------

/// Stages of the pipeline across portfolios (section 8.1), then the project's life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    Idea,
    Screening,
    Selection,
    Opening,
    Running,
    Closed,
}

impl Stage {
    pub const ALL: [Stage; 6] = [
        Stage::Idea,
        Stage::Screening,
        Stage::Selection,
        Stage::Opening,
        Stage::Running,
        Stage::Closed,
    ];
}

/// What the selection committee decides of an opportunity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Choice {
    #[serde(rename = "selecionar")]
    Select,
    #[serde(rename = "adiar")]
    Postpone,
    #[serde(rename = "devolver_triagem")]
    BackToScreening,
    #[serde(rename = "recusar")]
    Refuse,
}

impl Choice {
    pub const ALL: [Choice; 4] = [
        Choice::Select,
        Choice::Postpone,
        Choice::BackToScreening,
        Choice::Refuse,
    ];

    /// The word written in the minutes (the file content is Portuguese, like the default
    /// checklists).
    pub fn minutes_label(self) -> &'static str {
        match self {
            Choice::Select => "selecionar",
            Choice::Postpone => "adiar",
            Choice::BackToScreening => "devolver à triagem",
            Choice::Refuse => "recusar",
        }
    }

    /// The state the idea takes in its portfolio (`None`: it stays where it is).
    pub fn idea_status(self) -> Option<ProjectStatus> {
        match self {
            Choice::Select => Some(ProjectStatus::Approved),
            Choice::Refuse => Some(ProjectStatus::Cancelled),
            Choice::Postpone | Choice::BackToScreening => None,
        }
    }
}

/// An idea sent by the champion to the selection committee.
#[derive(Debug, Clone, PartialEq)]
pub struct Opportunity {
    pub id: Id,
    pub idea: Option<Id>,
    pub portfolio: Option<Id>,
    pub unit: Option<Id>,
    pub title: String,
    pub score: f64,
    pub sent_on: Option<Date>,
    pub decision: Option<Choice>,
    pub reason: String,
    pub belt: Option<Id>,
}

impl Opportunity {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            idea: e.id(fields::IDEA),
            portfolio: e.id(fields::PORTFOLIO),
            unit: e.id(fields::UNIT),
            title: e.text(fields::TITLE),
            score: e.number(fields::SCORE).unwrap_or(0.0),
            sent_on: e.opt_text(fields::SENT_ON).and_then(|s| s.parse().ok()),
            decision: e.typed(fields::DECISION),
            reason: e.text(fields::REASON),
            belt: e.id(fields::BELT),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Opportunity, id)
    }

    /// Still on the committee's table: not decided, or postponed.
    pub fn open(&self) -> bool {
        matches!(self.decision, None | Some(Choice::Postpone))
    }
}

pub fn opportunities(state: &State, program: Id) -> Vec<Opportunity> {
    let mut v = of_program(state, Kind::Opportunity, program, Opportunity::read);
    v.sort_by_key(|o| o.id);
    v
}

/// The stage of an idea given its opportunity (if any). Cancelled and archived ideas are
/// out of the pipeline (`None`).
pub fn stage(
    idea: &IdeaRecord,
    opportunity: Option<&Opportunity>,
    project: Option<&ProjectRecord>,
) -> Option<Stage> {
    if let Some(p) = project {
        return match p.status {
            ProjectStatus::Closed => Some(Stage::Closed),
            ProjectStatus::Cancelled | ProjectStatus::Archived => None,
            _ => Some(Stage::Running),
        };
    }
    match idea.status {
        ProjectStatus::Idea => Some(Stage::Idea),
        ProjectStatus::Screening => {
            if opportunity.is_some_and(Opportunity::open) {
                Some(Stage::Selection)
            } else {
                Some(Stage::Screening)
            }
        }
        ProjectStatus::Approved => Some(Stage::Opening),
        ProjectStatus::Running | ProjectStatus::Suspended => Some(Stage::Running),
        ProjectStatus::Closed => Some(Stage::Closed),
        ProjectStatus::Cancelled | ProjectStatus::Archived => None,
    }
}

/// The latest opportunity of each idea.
pub fn opportunity_of(opps: &[Opportunity], idea: Id) -> Option<&Opportunity> {
    opps.iter()
        .filter(|o| o.idea == Some(idea))
        .max_by_key(|o| o.id)
}

/// The project of an idea, by the id the portfolio recorded when it was opened.
pub fn project_of<'a>(
    projects: &'a [ProjectRecord],
    idea: &IdeaRecord,
) -> Option<&'a ProjectRecord> {
    idea.project
        .and_then(|p| projects.iter().find(|r| r.id == p))
}

/// Edits that send an idea in screening to the committee (by its unit's champion or the
/// MBB; the permission is checked by [`crate::perfis`]).
pub fn send_to_committee(
    program: Id,
    id: Id,
    idea: &IdeaRecord,
    opps: &[Opportunity],
    by: Id,
    today: Date,
) -> Result<Vec<Edit>, ProgramError> {
    if idea.status != ProjectStatus::Screening {
        return Err(ProgramError::NotInScreening);
    }
    if opportunity_of(opps, idea.id).is_some_and(Opportunity::open) {
        return Err(ProgramError::AlreadySent);
    }
    let r = Opportunity::entity_ref(id);
    // The unit first: the permission of a champion is checked against it.
    let mut e = Vec::new();
    if let Some(u) = idea.unit {
        e.push(Edit::new(r, fields::UNIT, u.to_string()));
    }
    e.extend([
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::IDEA, idea.id.to_string()),
        Edit::new(r, fields::PORTFOLIO, idea.portfolio.to_string()),
        Edit::new(r, fields::TITLE, idea.title.clone()),
        Edit::new(r, fields::SCORE, idea.score),
        Edit::new(r, fields::SENT_ON, today.to_string()),
        Edit::new(r, fields::SENT_BY, by.to_string()),
    ]);
    Ok(e)
}

/// A meeting of the selection committee (a program meeting with `comite`).
pub fn new_committee(program: Id, id: Id, title: &str, date: Date) -> Vec<Edit> {
    let mut e = meeting::new_meeting(program, id, title, date, MeetingKind::Portfolio);
    e.push(Edit::new(
        EntityRef::new(Kind::Meeting, id),
        fields::COMMITTEE,
        true,
    ));
    e
}

/// Committee meetings, latest first.
pub fn committees(state: &State, program: Id) -> Vec<crate::domain::Meeting> {
    meeting::meetings(state, program)
        .into_iter()
        .filter(|m| {
            state
                .entity(m.id)
                .is_some_and(|e| e.boolean(fields::COMMITTEE))
        })
        .collect()
}

/// The date of the next committee: the last one plus the cadence (today without one).
pub fn next_committee(state: &State, program: Id, today: Date) -> Date {
    let cadence = read(state, program).map_or(DEFAULT_CADENCE_DAYS, |p| p.cadence_days);
    committees(state, program)
        .iter()
        .filter_map(|m| m.date)
        .max()
        .map_or(today, |d| d.add_days(cadence))
}

/// One line of the generated committee agenda.
#[derive(Debug, Clone, PartialEq)]
pub enum AgendaLine {
    /// An opportunity to decide, best score first.
    Decide {
        opportunity: Id,
        title: String,
        unit: Option<Id>,
        score: f64,
    },
    /// Free capacity of the belts of a level: people with a free project slot.
    Capacity { level: BeltLevel, free: usize },
    /// A unit target still open in the year: target and realised.
    OpenTarget {
        unit: Option<Id>,
        metric: Metric,
        level: Option<BeltLevel>,
        target: f64,
        actual: f64,
    },
}

/// The agenda of the committee: opportunities in selection by score, the free capacity
/// per belt level (`free_by_level`, from [`crate::designacao`]) and the targets of the
/// year still open.
pub fn agenda(
    opps: &[Opportunity],
    free_by_level: &BTreeMap<BeltLevel, usize>,
    score: &[ScoreRow],
) -> Vec<AgendaLine> {
    let mut open: Vec<&Opportunity> = opps.iter().filter(|o| o.open()).collect();
    open.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    let mut v: Vec<AgendaLine> = open
        .into_iter()
        .map(|o| AgendaLine::Decide {
            opportunity: o.id,
            title: o.title.clone(),
            unit: o.unit,
            score: o.score,
        })
        .collect();
    for (level, free) in free_by_level {
        v.push(AgendaLine::Capacity {
            level: *level,
            free: *free,
        });
    }
    for r in score.iter().filter(|r| r.unit.is_some()) {
        if let Some(t) = r.target.filter(|t| r.actual < *t) {
            v.push(AgendaLine::OpenTarget {
                unit: r.unit,
                metric: r.metric,
                level: r.level,
                target: t,
                actual: r.actual,
            });
        }
    }
    v
}

/// A committee decision: the history entity and the opportunity's current decision.
#[derive(Debug, Clone, PartialEq)]
pub struct CommitteeDecision {
    pub id: Id,
    pub opportunity: Option<Id>,
    pub meeting: Option<Id>,
    pub choice: Option<Choice>,
    pub reason: String,
    pub date: Option<Date>,
    pub by: Option<Id>,
}

impl CommitteeDecision {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            opportunity: e.id(fields::OPPORTUNITY),
            meeting: e.id(fields::MEETING),
            choice: e.typed(fields::DECISION),
            reason: e.text(fields::REASON),
            date: e.opt_text(fields::DATE).and_then(|s| s.parse().ok()),
            by: e.id(fields::BY),
        }
    }
}

pub fn decisions(state: &State, opportunity: Id) -> Vec<CommitteeDecision> {
    let mut v: Vec<CommitteeDecision> = state
        .of_kind(Kind::CommitteeDecision)
        .map(|(id, e)| CommitteeDecision::read(id, e))
        .filter(|d| d.opportunity == Some(opportunity))
        .collect();
    v.sort_by_key(|d| d.id);
    v
}

/// What a decision of the committee asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionInput {
    pub opportunity: Id,
    pub meeting: Option<Id>,
    pub choice: Choice,
    pub reason: String,
    pub belt: Option<Id>,
    pub by: Id,
    pub date: Date,
}

/// Edits of the program journal that record a committee decision (the history entity,
/// the opportunity's decision and a decision line in the minutes when the meeting is
/// given); the caller moves the idea in its portfolio to [`Choice::idea_status`] in the
/// same step (see `portfolio::move_idea`), so a refused move refuses the decision.
pub fn decide(
    state: &State,
    input: &DecisionInput,
    mut ids: impl FnMut() -> Id,
) -> Result<Vec<Edit>, ProgramError> {
    let e = state
        .entity(input.opportunity)
        .filter(|e| e.kind == Kind::Opportunity && !e.is_deleted())
        .ok_or(ProgramError::NotFound)?;
    let o = Opportunity::read(input.opportunity, e);
    if !o.open() {
        return Err(ProgramError::Decided);
    }
    if input.reason.trim().is_empty() {
        return Err(ProgramError::NoReason);
    }
    let d = EntityRef::new(Kind::CommitteeDecision, ids());
    let r = Opportunity::entity_ref(o.id);
    let mut v = vec![
        Edit::new(d, fields::OPPORTUNITY, o.id.to_string()),
        Edit::new(d, fields::DECISION, enum_value(input.choice)),
        Edit::new(d, fields::REASON, input.reason.trim()),
        Edit::new(d, fields::DATE, input.date.to_string()),
        Edit::new(d, fields::BY, input.by.to_string()),
        Edit::new(r, fields::DECISION, enum_value(input.choice)),
        Edit::new(r, fields::REASON, input.reason.trim()),
    ];
    if let Some(m) = input.meeting {
        v.push(Edit::new(d, fields::MEETING, m.to_string()));
        v.push(Edit::new(r, fields::MEETING, m.to_string()));
        v.extend(meeting::add_item(
            state,
            m,
            ids(),
            ItemKind::Decision,
            &format!(
                "{}: {} ({})",
                o.title,
                input.choice.minutes_label(),
                input.reason.trim()
            ),
        ));
    }
    if let Some(b) = input.belt {
        v.push(Edit::new(r, fields::BELT, b.to_string()));
    }
    Ok(v)
}

#[cfg(test)]
pub(crate) mod testkit {
    //! A synthetic program for the tests of the program modules.
    use super::*;

    pub struct T {
        pub s: State,
        pub n: u64,
        pub me: Id,
        pub program: Id,
        /// Every operation applied, in order.
        pub ops: Vec<crate::op::Op>,
    }

    impl T {
        pub fn new() -> T {
            let mut t = T {
                s: State::new(),
                n: 0,
                me: Id::NIL,
                program: Id::NIL,
                ops: Vec::new(),
            };
            t.me = t.id();
            t.program = t.id();
            let p = t.program;
            t.commit(create(p, "Programa LSS"));
            t
        }
        pub fn id(&mut self) -> Id {
            self.n += 1;
            Id::v7(self.n, [self.n as u8; 10])
        }
        pub fn commit(&mut self, e: Vec<Edit>) {
            let me = self.me;
            self.commit_as(me, e);
        }
        pub fn commit_as(&mut self, who: Id, e: Vec<Edit>) {
            let mut n = self.n;
            let ops = self.s.prepare(who, 0, e, || {
                n += 1;
                Id::v7(n, [n as u8; 10])
            });
            self.n = n;
            for op in ops {
                self.s.apply(&op).unwrap();
                self.ops.push(op);
            }
        }
        pub fn person(&mut self, name: &str, level: Option<BeltLevel>, unit: Option<Id>) -> Id {
            let id = self.id();
            let r = Member::entity_ref(id);
            let mut e = vec![Edit::new(r, dfields::person::NAME, name)];
            if let Some(l) = level {
                e.push(Edit::new(r, fields::LEVEL, enum_value(l)));
            }
            if let Some(u) = unit {
                e.push(Edit::new(r, fields::UNIT, u.to_string()));
            }
            self.commit(e);
            id
        }
        pub fn set(&mut self, kind: Kind, id: Id, f: &str, v: impl Into<Value>) {
            self.commit(vec![Edit::new(EntityRef::new(kind, id), f, v)]);
        }
    }

    pub fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    /// A project record with known numbers.
    pub fn project(id: Id, unit: Option<Id>, status: ProjectStatus) -> ProjectRecord {
        ProjectRecord {
            id,
            title: format!("P{}", id.short()),
            unit,
            status,
            ..ProjectRecord::default()
        }
    }

    pub fn idea(id: Id, unit: Option<Id>, status: ProjectStatus, score: f64) -> IdeaRecord {
        IdeaRecord {
            id,
            portfolio: Id::v7(9_999, [9; 10]),
            unit,
            title: format!("I{}", id.short()),
            status,
            since: Some(d(2026, 10, 1)),
            score,
            rough_gain: 0.0,
            approach: Some(Approach::Dmaic),
            belt: None,
            process: String::new(),
            project: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::domain::SavingStatus as S;
    use proptest::prelude::*;

    #[test]
    fn structure_objectives_deployed_per_unit() {
        let mut t = T::new();
        let (u1, u2) = (t.id(), t.id());
        let p = t.program;
        t.commit(add_unit(p, u1, "Planta 1"));
        t.commit(add_unit(p, u2, "Logística"));
        let (o1, o2) = (t.id(), t.id());
        t.commit(add_objective(p, o1, "Reduzir custo", 3.0));
        t.commit(add_objective(p, o2, "Segurança", 1.0));
        t.commit(vec![
            deploy(o1, u1, 1.0),
            deploy(o2, u1, 1.0),
            deploy(o2, u2, 2.0),
        ]);
        let os = objectives(&t.s, p);
        assert_eq!(os[0].id, o1, "heavier first");
        let w1 = unit_objectives(&os, u1);
        assert_eq!(w1.len(), 2);
        assert!((w1.iter().map(|x| x.1).sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((w1.iter().find(|x| x.0 == o1).unwrap().1 - 0.75).abs() < 1e-12);
        assert_eq!(unit_objectives(&os, u2), vec![(o2, 1.0)]);
        t.commit(vec![deploy(o2, u2, 0.0)]);
        assert!(unit_objectives(&objectives(&t.s, p), u2).is_empty());
        let us = units(&t.s, p);
        assert_eq!(us[0].name, "Logística");
        // Portfolio linked by id, or by its own unit id.
        t.set(
            Kind::BusinessUnit,
            u1,
            fields::PORTFOLIO,
            Id::v7(5, [5; 10]).to_string(),
        );
        let us = units(&t.s, p);
        assert_eq!(unit_of_portfolio(&us, Id::v7(5, [5; 10]), None), Some(u1));
        assert_eq!(
            unit_of_portfolio(&us, Id::v7(6, [6; 10]), Some(u2)),
            Some(u2)
        );
        assert_eq!(unit_of_portfolio(&us, Id::v7(6, [6; 10]), None), None);
        assert_eq!(id_from_key(&portfolio::id_key(u1)), Some(u1));
    }

    #[test]
    fn targets_replace_and_validate() {
        let mut t = T::new();
        let u = t.id();
        let p = t.program;
        t.commit(add_unit(p, u, "Planta 1"));
        let id = t.id();
        assert_eq!(
            set_target(&t.s, p, id, u, 2026, Metric::HardValidated, None, 0.0),
            Err(ProgramError::BadTarget)
        );
        assert_eq!(
            set_target(&t.s, p, id, u, 2026, Metric::BeltsCertified, None, 3.0),
            Err(ProgramError::NeedsLevel)
        );
        let e = set_target(&t.s, p, id, u, 2026, Metric::HardValidated, None, 500_000.0).unwrap();
        t.commit(e);
        let id2 = t.id();
        let e = set_target(
            &t.s,
            p,
            id2,
            u,
            2026,
            Metric::HardValidated,
            None,
            600_000.0,
        )
        .unwrap();
        assert_eq!(e.len(), 1, "same key: the value changes");
        t.commit(e);
        let ts = targets(&t.s, p);
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].value, 600_000.0);
    }

    /// A program with two units and known numbers: the scoreboard counts only validated
    /// savings, never training projects, and the forecast stays apart.
    #[test]
    fn scoreboard_counts_only_validated_savings() {
        let mut t = T::new();
        let p = t.program;
        let (u1, u2) = (t.id(), t.id());
        t.commit(add_unit(p, u1, "Planta 1"));
        t.commit(add_unit(p, u2, "Planta 2"));
        t.set(Kind::BusinessUnit, u1, fields::HEADCOUNT, 200);
        for (unit, metric, level, v) in [
            (u1, Metric::HardValidated, None, 100_000.0),
            (u1, Metric::ProjectsClosed, None, 2.0),
            (u1, Metric::BeltsCertified, Some(BeltLevel::Green), 2.0),
            (u1, Metric::Coverage, None, 2.0),
            (u2, Metric::HardValidated, None, 50_000.0),
        ] {
            let id = t.id();
            let e = set_target(&t.s, p, id, unit, 2026, metric, level, v).unwrap();
            t.commit(e);
        }
        let g1 = t.person("Ana", Some(BeltLevel::Green), Some(u1));
        t.set(Kind::Person, g1, fields::CERTIFIED_ON, "2026-05-10");
        let g2 = t.person("Bia", Some(BeltLevel::Green), Some(u1));
        t.set(Kind::Person, g2, fields::CERTIFIED_ON, "2025-05-10");
        t.person("Caio", Some(BeltLevel::Yellow), Some(u1));
        let ms = members(&t.s);

        let mut a = project(t.id(), Some(u1), ProjectStatus::Closed);
        a.closed_on = Some(d(2026, 6, 1));
        a.validated_by_year.insert((2026, Category::Hard), 40_000.0);
        a.validated_by_year.insert((2025, Category::Hard), 7_000.0);
        a.totals.by.insert((Category::Hard, S::Forecast), 90_000.0);
        let mut b = project(t.id(), Some(u1), ProjectStatus::Running);
        b.totals.by.insert((Category::Hard, S::Forecast), 30_000.0);
        b.totals.by.insert((Category::Hard, S::Realized), 25_000.0);
        let mut tr = project(t.id(), Some(u1), ProjectStatus::Closed);
        tr.training = true;
        tr.closed_on = Some(d(2026, 2, 1));
        tr.validated_by_year.insert((2026, Category::Hard), 1e9);
        let mut c = project(t.id(), Some(u2), ProjectStatus::Closed);
        c.closed_on = Some(d(2025, 12, 31));
        c.validated_by_year.insert((2026, Category::Soft), 5_000.0);
        let ps = vec![a, b, tr, c];

        let rows = scoreboard(&t.s, p, 2026, &ps, &ms);
        let row = |u: Option<Id>, m: Metric| {
            rows.iter()
                .find(|r| r.unit == u && r.metric == m)
                .unwrap()
                .clone()
        };
        let h1 = row(Some(u1), Metric::HardValidated);
        assert_eq!(h1.actual, 40_000.0, "validated only; training out");
        assert_eq!(h1.forecast, 80_000.0, "forecast apart: 90k + 30k - 40k");
        assert_eq!(h1.target, Some(100_000.0));
        assert!((h1.reached().unwrap() - 0.4).abs() < 1e-12);
        assert_eq!(row(Some(u1), Metric::ProjectsClosed).actual, 1.0);
        assert_eq!(
            row(Some(u2), Metric::ProjectsClosed).actual,
            0.0,
            "closed in 2025"
        );
        assert_eq!(row(Some(u2), Metric::SoftValidated).actual, 5_000.0);
        assert_eq!(row(Some(u1), Metric::BeltsCertified).actual, 1.0);
        assert_eq!(
            row(Some(u1), Metric::Coverage).actual,
            1.0,
            "2 greens per 200 people"
        );
        let total = row(None, Metric::HardValidated);
        assert_eq!(total.actual, 40_000.0);
        assert_eq!(total.target, Some(150_000.0));
        assert_eq!(row(None, Metric::Coverage).target, None);
        // Realised (not validated) never reaches the official total.
        assert!(rows.iter().all(|r| r.actual != 65_000.0));
    }

    #[test]
    fn pipeline_stages_and_committee() {
        let mut t = T::new();
        let p = t.program;
        let u = t.id();
        t.commit(add_unit(p, u, "Planta 1"));
        let i_screen = idea(t.id(), Some(u), ProjectStatus::Screening, 70.0);
        let i_idea = idea(t.id(), Some(u), ProjectStatus::Idea, 10.0);
        let i_low = idea(t.id(), Some(u), ProjectStatus::Screening, 40.0);
        assert_eq!(stage(&i_idea, None, None), Some(Stage::Idea));
        assert_eq!(stage(&i_screen, None, None), Some(Stage::Screening));
        assert_eq!(
            send_to_committee(p, t.id(), &i_idea, &[], t.me, d(2026, 11, 3)),
            Err(ProgramError::NotInScreening)
        );
        let o1 = t.id();
        let e = send_to_committee(p, o1, &i_screen, &[], t.me, d(2026, 11, 3)).unwrap();
        assert_eq!(
            e[0].field,
            fields::UNIT,
            "unit first, for the champion's check"
        );
        t.commit(e);
        let o2 = t.id();
        let e = send_to_committee(p, o2, &i_low, &[], t.me, d(2026, 11, 3)).unwrap();
        t.commit(e);
        let opps = opportunities(&t.s, p);
        assert_eq!(
            send_to_committee(p, t.id(), &i_screen, &opps, t.me, d(2026, 11, 3)),
            Err(ProgramError::AlreadySent)
        );
        assert_eq!(
            stage(&i_screen, opportunity_of(&opps, i_screen.id), None),
            Some(Stage::Selection)
        );
        // Agenda: best score first, capacity and open targets.
        let free = BTreeMap::from([(BeltLevel::Green, 2usize)]);
        let rows = vec![ScoreRow {
            unit: Some(u),
            metric: Metric::HardValidated,
            level: None,
            target: Some(10.0),
            actual: 3.0,
            forecast: 0.0,
        }];
        let ag = agenda(&opps, &free, &rows);
        assert!(matches!(&ag[0], AgendaLine::Decide { opportunity, .. } if *opportunity == o1));
        assert!(matches!(&ag[1], AgendaLine::Decide { opportunity, .. } if *opportunity == o2));
        assert!(matches!(ag[2], AgendaLine::Capacity { free: 2, .. }));
        assert!(matches!(ag[3], AgendaLine::OpenTarget { target, .. } if target == 10.0));
        // Committee meeting and decisions with the reason recorded.
        let m = t.id();
        t.commit(new_committee(p, m, "Comitê de novembro", d(2026, 11, 10)));
        assert_eq!(committees(&t.s, p).len(), 1);
        assert_eq!(next_committee(&t.s, p, d(2026, 11, 3)), d(2026, 12, 10));
        let mut input = DecisionInput {
            opportunity: o2,
            meeting: Some(m),
            choice: Choice::Postpone,
            reason: " ".into(),
            belt: None,
            by: t.me,
            date: d(2026, 11, 10),
        };
        let mut k = 900;
        let mut ids = || {
            k += 1;
            Id::v7(k, [7; 10])
        };
        assert_eq!(decide(&t.s, &input, &mut ids), Err(ProgramError::NoReason));
        input.reason = "sem capacidade de black belt".into();
        let e = decide(&t.s, &input, &mut ids).unwrap();
        t.commit(e);
        let opps = opportunities(&t.s, p);
        assert_eq!(
            stage(&i_low, opportunity_of(&opps, i_low.id), None),
            Some(Stage::Selection),
            "postponed stays in selection"
        );
        input.choice = Choice::Refuse;
        let e = decide(&t.s, &input, &mut ids).unwrap();
        t.commit(e);
        assert_eq!(decide(&t.s, &input, &mut ids), Err(ProgramError::Decided));
        assert_eq!(decisions(&t.s, o2).len(), 2);
        assert_eq!(meeting::items(&t.s, m).len(), 2);
        assert!(meeting::items(&t.s, m)[1].text.contains("recusar"));
        assert_eq!(Choice::Select.idea_status(), Some(ProjectStatus::Approved));
        assert_eq!(Choice::Postpone.idea_status(), None);
        // Opened ideas follow their project.
        let mut opened = idea(t.id(), Some(u), ProjectStatus::Running, 1.0);
        let pr = project(t.id(), Some(u), ProjectStatus::Closed);
        opened.project = Some(pr.id);
        let ps = vec![pr];
        assert_eq!(
            stage(&opened, None, project_of(&ps, &opened)),
            Some(Stage::Closed)
        );
    }

    proptest! {
        /// Whatever the entries, the official total is the sum of the validated ones and
        /// never counts realised or forecast values.
        #[test]
        fn official_total_is_the_validated_sum(
            entries in proptest::collection::vec((0u8..3, 1.0f64..1e6, any::<bool>()), 0..20)
        ) {
            let mut ps = Vec::new();
            let mut expected = 0.0;
            for (i, (kind, v, training)) in entries.iter().enumerate() {
                let mut r = project(Id::v7(i as u64 + 1, [1; 10]), None, ProjectStatus::Running);
                r.training = *training;
                match kind {
                    0 => {
                        r.validated_by_year.insert((2026, Category::Hard), *v);
                        if !training { expected += v; }
                    }
                    1 => { r.totals.by.insert((Category::Hard, S::Realized), *v); }
                    _ => { r.totals.by.insert((Category::Hard, S::Forecast), *v); }
                }
                ps.push(r);
            }
            let (a, f) = actual(Metric::HardValidated, None, None, 2026, &ps, &[], &[]);
            prop_assert!((a - expected).abs() < 1e-6);
            prop_assert!(f >= 0.0);
        }
    }
}
