//! Training of the belts (`docs/plano_lean_seis_sigma.md`, 8.5; decisions MB3, MB6, MB7 and
//! MB8): waves and classes, the choice of the trainees, the training teams, the
//! curriculum, the attendance, the exams and the instructors. The certification is in
//! [`crate::certificacao`].
//!
//! - A **wave** (`onda`) is a cycle of the program (a semester, say) with training targets
//!   per level and unit; a **class** (`turma`) has a level, dates, sessions, a place or
//!   form, seats, instructors, the minimum attendance (80 %) and the minimum grade (70 %).
//! - **Nomination** (`indicacao`) by the manager, the champion or the person, with a
//!   justification and, for green and black belt, a candidate opportunity of the pipeline;
//!   the company's **nomination criteria**, each eliminatory or classificatory; the
//!   **manager's approval** (the manager, never the nominee); the **capacity check** (the
//!   training hours plus the project's fit in the person's free hours of [`crate::designacao`]
//!   and the class has a seat); the **selection** by the MBB with the **waiting list**
//!   ordered by the classificatory criteria ([`rank`], [`proposal`]).
//! - **Training teams**: a deterministic suggestion under constraints in [`teams`].
//! - **Curriculum** per level with modules linked to the app's tools in [`curriculum`];
//!   attendance and exercises feed `estudou` and `praticou` of the competence map
//!   ([`learning_entries`]).
//! - **Exams** from a question bank with a recorded seed in [`exams`].
//!
//! Where things live (section 8.9): waves, classes, sessions with the attendance,
//! nominations, criteria, curriculum, groups, instructors and exam definitions (without
//! the answer key) in the program journal; the question bank with the keys in
//! `formacao/questoes.lss` and the grades in `formacao/notas.lss` (restricted to the MBB
//! and the instructors); the trainees' papers and answers in the class folder
//! `formacao/turmas/<turma>.lss`.

pub mod curriculum;
pub mod exams;
pub mod teams;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::designacao::{self, Assignment, Load};
use crate::domain::enum_value;
use crate::id::Id;
use crate::mentoria::{self, Level as SkillLevel, Source};
use crate::op::{Edit, EntityRef, FIELD_DELETED, Kind};
use crate::portfolio;
use crate::programa::{self, BeltLevel, Member, Opportunity};
use crate::state::{Entity, State};

pub mod fields {
    pub const PROGRAM: &str = "programa";
    pub const NAME: &str = "nome";
    pub const START: &str = "inicio";
    pub const END: &str = "fim";
    /// Wave targets: `meta_<level>` for the program, `meta_<level>_<unit>` per unit.
    pub const TARGET_PREFIX: &str = "meta_";
    // Class.
    pub const WAVE: &str = "onda";
    pub const LEVEL: &str = "nivel";
    pub const PLACE: &str = "local";
    pub const SEATS: &str = "vagas";
    pub const INSTRUCTORS: &str = "instrutores";
    pub const MIN_ATTENDANCE: &str = "frequencia_minima";
    pub const MIN_GRADE: &str = "nota_minima";
    pub const CLOSED: &str = "encerrada";
    // Class session.
    pub const CLASS: &str = "turma";
    pub const DATE: &str = "data";
    pub const HOURS: &str = "horas";
    pub const MODULES: &str = "modulos";
    pub const INSTRUCTOR: &str = "instrutor";
    /// `presenca_<person>`: `presente`, `ausente` or `justificado`.
    pub const ATTENDANCE_PREFIX: &str = "presenca_";
    /// `exercicio_<person>`: the exercise of the session done correctly.
    pub const EXERCISE_PREFIX: &str = "exercicio_";
    // Nomination.
    pub const PERSON: &str = "pessoa";
    pub const BY: &str = "indicado_por";
    pub const JUSTIFICATION: &str = "justificativa";
    pub const OPPORTUNITY: &str = "oportunidade";
    pub const MANAGER: &str = "gestor";
    pub const TENURE: &str = "tempo_empresa_meses";
    pub const FUNCTION: &str = "funcao";
    pub const RATING: &str = "desempenho";
    pub const HOURS_AVAILABLE: &str = "horas_disponiveis";
    pub const NOMINATED_ON: &str = "indicado_em";
    pub const MANAGER_APPROVAL: &str = "aprovacao_gestor";
    pub const MANAGER_ON: &str = "aprovacao_gestor_em";
    pub const MANAGER_REASON: &str = "motivo_gestor";
    pub const STATUS: &str = "situacao";
    pub const POSITION: &str = "posicao_espera";
    pub const STATUS_REASON: &str = "motivo_situacao";
    pub const DECIDED_BY: &str = "decidido_por";
    pub const DECIDED_ON: &str = "decidido_em";
    // Nomination criterion.
    pub const KIND: &str = "tipo";
    pub const ELIMINATORY: &str = "eliminatorio";
    pub const THRESHOLD: &str = "limite";
    pub const WEIGHT: &str = "peso";
    pub const TEXT: &str = "texto";
    // Training group.
    pub const MEMBERS: &str = "membros";
    pub const CONFIRMED_BY: &str = "confirmado_por";
    pub const CONFIRMED_ON: &str = "confirmado_em";
    // Program settings of the training.
    pub const GROUP_SIZE: &str = "tamanho_grupo";
    pub const MAX_SAME_AREA: &str = "maximo_mesma_area";
}

/// Default minimum attendance of a class.
pub const DEFAULT_MIN_ATTENDANCE: f64 = 0.8;
/// Default minimum grade of an exam.
pub const DEFAULT_MIN_GRADE: f64 = 0.7;
/// Default seats of a class.
pub const DEFAULT_SEATS: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrainingError {
    #[error("escolha a pessoa indicada")]
    NoPerson,
    #[error("escreva a justificativa da indicação")]
    NoJustification,
    #[error("green e black belt precisam de uma oportunidade candidata do funil (o projeto de formação)")]
    NoOpportunity,
    #[error("informe o gestor imediato, que aprova as horas; não pode ser a própria pessoa")]
    BadManager,
    #[error("a pessoa já tem uma indicação ativa para este nível nesta onda")]
    Duplicate,
    #[error("só o gestor indicado aprova ou recusa a indicação")]
    NotTheManager,
    #[error("registre o motivo da recusa")]
    NoReason,
    #[error("a indicação ainda não foi aprovada pelo gestor")]
    NotApproved,
    #[error("dê um nome")]
    NoName,
    #[error("indicação não encontrada")]
    NotFound,
}

// --- Small helpers ----------------------------------------------------------------------

/// A list of ids stored as an array of strings.
pub fn ids_in(e: &Entity, field: &str) -> Vec<Id> {
    e.get(field)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn ids_value(ids: &[Id]) -> Value {
    Value::from(
        ids.iter()
            .map(|i| Value::from(i.to_string()))
            .collect::<Vec<_>>(),
    )
}

/// A list of strings stored as an array.
pub fn strings_in(e: &Entity, field: &str) -> Vec<String> {
    e.get(field)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// A number as shown in the evaluation (up to two decimals).
fn num(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn date_of(e: &Entity, field: &str) -> Option<Date> {
    e.opt_text(field).and_then(|s| s.parse().ok())
}

fn live(state: &State, kind: Kind) -> impl Iterator<Item = (Id, &Entity)> {
    state.of_kind(kind).filter(|(_, e)| !e.is_deleted())
}

/// The file key of a belt level (`green_belt`...).
pub fn level_key(l: BeltLevel) -> String {
    enum_value(l).as_str().unwrap_or_default().to_string()
}

/// The level just below (the prerequisite of the "nível anterior" criterion).
pub fn previous_level(l: BeltLevel) -> Option<BeltLevel> {
    match l {
        BeltLevel::Yellow => None,
        BeltLevel::Green => Some(BeltLevel::Yellow),
        BeltLevel::Black => Some(BeltLevel::Green),
        BeltLevel::MasterBlack => Some(BeltLevel::Black),
    }
}

// --- Waves ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Wave {
    pub id: Id,
    pub name: String,
    pub start: Option<Date>,
    pub end: Option<Date>,
    /// People to train per level, for the program (`None`) or a unit.
    pub targets: BTreeMap<(BeltLevel, Option<Id>), u32>,
}

impl Wave {
    pub fn read(id: Id, e: &Entity) -> Self {
        let mut targets = BTreeMap::new();
        for (k, v) in &e.fields {
            let Some(rest) = k.strip_prefix(fields::TARGET_PREFIX) else {
                continue;
            };
            let Some(n) = v.value.as_u64() else { continue };
            for l in BeltLevel::ALL {
                let lk = level_key(l);
                if rest == lk {
                    targets.insert((l, None), n as u32);
                } else if let Some(u) = rest
                    .strip_prefix(lk.as_str())
                    .and_then(|r| r.strip_prefix('_'))
                    .and_then(programa::id_from_key)
                {
                    targets.insert((l, Some(u)), n as u32);
                }
            }
        }
        Self {
            id,
            name: e.text(fields::NAME),
            start: date_of(e, fields::START),
            end: date_of(e, fields::END),
            targets,
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Wave, id)
    }
}

pub fn waves(state: &State, program: Id) -> Vec<Wave> {
    let key = program.to_string();
    let mut v: Vec<Wave> = live(state, Kind::Wave)
        .filter(|(_, e)| e.text(fields::PROGRAM) == key)
        .map(|(id, e)| Wave::read(id, e))
        .collect();
    v.sort_by_key(|w| (w.start, w.id));
    v
}

pub fn new_wave(
    program: Id,
    id: Id,
    name: &str,
    start: Option<Date>,
    end: Option<Date>,
) -> Result<Vec<Edit>, TrainingError> {
    if name.trim().is_empty() {
        return Err(TrainingError::NoName);
    }
    let r = Wave::entity_ref(id);
    let mut e = vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::NAME, name.trim()),
    ];
    if let Some(d) = start {
        e.push(Edit::new(r, fields::START, d.to_string()));
    }
    if let Some(d) = end {
        e.push(Edit::new(r, fields::END, d.to_string()));
    }
    Ok(e)
}

/// The field of a wave target.
pub fn target_field(level: BeltLevel, unit: Option<Id>) -> String {
    match unit {
        None => format!("{}{}", fields::TARGET_PREFIX, level_key(level)),
        Some(u) => format!(
            "{}{}_{}",
            fields::TARGET_PREFIX,
            level_key(level),
            portfolio::id_key(u)
        ),
    }
}

/// Edit that sets a wave target (0 removes it).
pub fn set_wave_target(wave: Id, level: BeltLevel, unit: Option<Id>, n: u32) -> Edit {
    let v = if n > 0 { Value::from(n) } else { Value::Null };
    Edit::new(Wave::entity_ref(wave), target_field(level, unit), v)
}

// --- Classes ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub id: Id,
    pub wave: Option<Id>,
    pub name: String,
    pub level: BeltLevel,
    pub start: Option<Date>,
    pub end: Option<Date>,
    pub place: String,
    pub seats: u32,
    pub instructors: Vec<Id>,
    pub min_attendance: f64,
    pub min_grade: f64,
    pub closed: bool,
}

impl Class {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            wave: e.id(fields::WAVE),
            name: e.text(fields::NAME),
            level: e.typed(fields::LEVEL).unwrap_or_default(),
            start: date_of(e, fields::START),
            end: date_of(e, fields::END),
            place: e.text(fields::PLACE),
            seats: e
                .integer(fields::SEATS)
                .filter(|n| *n > 0)
                .map_or(DEFAULT_SEATS, |n| n.min(10_000) as u32),
            instructors: ids_in(e, fields::INSTRUCTORS),
            min_attendance: e
                .number(fields::MIN_ATTENDANCE)
                .filter(|x| (0.0..=1.0).contains(x))
                .unwrap_or(DEFAULT_MIN_ATTENDANCE),
            min_grade: e
                .number(fields::MIN_GRADE)
                .filter(|x| (0.0..=1.0).contains(x))
                .unwrap_or(DEFAULT_MIN_GRADE),
            closed: e.boolean(fields::CLOSED),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::TrainingClass, id)
    }
}

pub fn classes(state: &State, program: Id) -> Vec<Class> {
    let key = program.to_string();
    let mut v: Vec<Class> = live(state, Kind::TrainingClass)
        .filter(|(_, e)| e.text(fields::PROGRAM) == key)
        .map(|(id, e)| Class::read(id, e))
        .collect();
    v.sort_by_key(|c| (c.start, c.level, c.id));
    v
}

/// What a new class records.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassInput {
    pub wave: Option<Id>,
    pub name: String,
    pub level: BeltLevel,
    pub start: Option<Date>,
    pub end: Option<Date>,
    pub place: String,
    pub seats: u32,
}

pub fn new_class(program: Id, id: Id, input: &ClassInput) -> Result<Vec<Edit>, TrainingError> {
    if input.name.trim().is_empty() {
        return Err(TrainingError::NoName);
    }
    let r = Class::entity_ref(id);
    let mut e = vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::NAME, input.name.trim()),
        Edit::new(r, fields::LEVEL, enum_value(input.level)),
        Edit::new(r, fields::PLACE, input.place.trim()),
        Edit::new(r, fields::SEATS, input.seats.max(1)),
        Edit::new(r, fields::MIN_ATTENDANCE, DEFAULT_MIN_ATTENDANCE),
        Edit::new(r, fields::MIN_GRADE, DEFAULT_MIN_GRADE),
    ];
    if let Some(w) = input.wave {
        e.push(Edit::new(r, fields::WAVE, w.to_string()));
    }
    if let Some(d) = input.start {
        e.push(Edit::new(r, fields::START, d.to_string()));
    }
    if let Some(d) = input.end {
        e.push(Edit::new(r, fields::END, d.to_string()));
    }
    Ok(e)
}

/// Edit that sets the instructors of a class.
pub fn set_instructors(class: Id, instructors: &[Id]) -> Edit {
    let mut v = instructors.to_vec();
    v.sort();
    v.dedup();
    Edit::new(Class::entity_ref(class), fields::INSTRUCTORS, ids_value(&v))
}

// --- Sessions and attendance ------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Attendance {
    #[default]
    #[serde(rename = "presente")]
    Present,
    #[serde(rename = "ausente")]
    Absent,
    #[serde(rename = "justificado")]
    Justified,
}

impl Attendance {
    pub const ALL: [Attendance; 3] = [
        Attendance::Present,
        Attendance::Absent,
        Attendance::Justified,
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassSession {
    pub id: Id,
    pub class: Option<Id>,
    pub date: Option<Date>,
    pub hours: f64,
    pub modules: Vec<Id>,
    pub instructor: Option<Id>,
    pub attendance: BTreeMap<Id, Attendance>,
    /// Trainees whose exercise of the session was correct.
    pub exercises: BTreeSet<Id>,
}

impl ClassSession {
    pub fn read(id: Id, e: &Entity) -> Self {
        let mut attendance = BTreeMap::new();
        let mut exercises = BTreeSet::new();
        for (k, v) in &e.fields {
            if let Some(p) = k
                .strip_prefix(fields::ATTENDANCE_PREFIX)
                .and_then(programa::id_from_key)
            {
                if let Ok(a) = serde_json::from_value::<Attendance>(v.value.clone()) {
                    attendance.insert(p, a);
                }
            } else if let Some(p) = k
                .strip_prefix(fields::EXERCISE_PREFIX)
                .and_then(programa::id_from_key)
            {
                if v.value.as_bool() == Some(true) {
                    exercises.insert(p);
                }
            }
        }
        Self {
            id,
            class: e.id(fields::CLASS),
            date: date_of(e, fields::DATE),
            hours: e.number(fields::HOURS).unwrap_or(0.0).max(0.0),
            modules: ids_in(e, fields::MODULES),
            instructor: e.id(fields::INSTRUCTOR),
            attendance,
            exercises,
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::ClassSession, id)
    }

    /// A session is held once someone's attendance is recorded.
    pub fn held(&self) -> bool {
        !self.attendance.is_empty()
    }
}

/// Every session of the program journal, by date.
pub fn all_sessions(state: &State) -> Vec<ClassSession> {
    let mut v: Vec<ClassSession> = live(state, Kind::ClassSession)
        .map(|(id, e)| ClassSession::read(id, e))
        .collect();
    v.sort_by_key(|s| (s.date, s.id));
    v
}

pub fn sessions_of(all: &[ClassSession], class: Id) -> Vec<&ClassSession> {
    all.iter().filter(|s| s.class == Some(class)).collect()
}

/// What a new session records (the class first: the permission check reads it).
pub fn new_session(
    class: Id,
    id: Id,
    date: Date,
    hours: f64,
    modules: &[Id],
    instructor: Option<Id>,
) -> Vec<Edit> {
    let r = ClassSession::entity_ref(id);
    let mut e = vec![
        Edit::new(r, fields::CLASS, class.to_string()),
        Edit::new(r, fields::DATE, date.to_string()),
        Edit::new(r, fields::HOURS, hours.max(0.0)),
        Edit::new(r, fields::MODULES, ids_value(modules)),
    ];
    if let Some(i) = instructor {
        e.push(Edit::new(r, fields::INSTRUCTOR, i.to_string()));
    }
    e
}

pub fn attendance_field(person: Id) -> String {
    format!("{}{}", fields::ATTENDANCE_PREFIX, portfolio::id_key(person))
}

pub fn exercise_field(person: Id) -> String {
    format!("{}{}", fields::EXERCISE_PREFIX, portfolio::id_key(person))
}

pub fn mark(session: Id, person: Id, a: Attendance) -> Edit {
    Edit::new(
        ClassSession::entity_ref(session),
        attendance_field(person),
        enum_value(a),
    )
}

pub fn mark_exercise(session: Id, person: Id, correct: bool) -> Edit {
    Edit::new(
        ClassSession::entity_ref(session),
        exercise_field(person),
        correct,
    )
}

/// The attendance of a trainee in a class.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Completion {
    /// Hours of the held sessions.
    pub hours_total: f64,
    pub hours_present: f64,
    pub hours_justified: f64,
    /// Present hours over the held hours (a justified absence is still an absence; it is
    /// recorded with its reason).
    pub rate: f64,
    pub min: f64,
    pub ok: bool,
}

pub fn completion(class: &Class, sessions: &[ClassSession], person: Id) -> Completion {
    let mut c = Completion {
        min: class.min_attendance,
        ..Completion::default()
    };
    for s in sessions
        .iter()
        .filter(|s| s.class == Some(class.id) && s.held())
    {
        c.hours_total += s.hours;
        match s.attendance.get(&person) {
            Some(Attendance::Present) => c.hours_present += s.hours,
            Some(Attendance::Justified) => c.hours_justified += s.hours,
            _ => {}
        }
    }
    c.rate = if c.hours_total > 0.0 {
        c.hours_present / c.hours_total
    } else {
        0.0
    };
    c.ok = c.hours_total > 0.0 && c.rate + 1e-9 >= c.min;
    c
}

/// Training hours per week of a class: its sessions' hours over the weeks between the
/// first and last day (the class dates, else the sessions' dates).
pub fn training_hours_week(class: &Class, sessions: &[ClassSession]) -> f64 {
    let mine: Vec<&ClassSession> = sessions
        .iter()
        .filter(|s| s.class == Some(class.id))
        .collect();
    let total: f64 = mine.iter().map(|s| s.hours).sum();
    let first = class.start.or_else(|| mine.iter().filter_map(|s| s.date).min());
    let last = class.end.or_else(|| mine.iter().filter_map(|s| s.date).max());
    let weeks = match (first, last) {
        (Some(a), Some(b)) => ((a.days_until(b) + 1) as f64 / 7.0).ceil().max(1.0),
        _ => 1.0,
    };
    total / weeks
}

// --- Nomination criteria ------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum CriterionKind {
    /// Months in the company ≥ the threshold.
    #[default]
    #[serde(rename = "tempo_empresa")]
    Tenure,
    /// The function is one of the comma-separated list.
    #[serde(rename = "funcao")]
    Function,
    /// The level below concluded (the person's level).
    #[serde(rename = "nivel_anterior")]
    PriorLevel,
    /// Performance declared by the manager, 1 to 5.
    #[serde(rename = "desempenho")]
    ManagerRating,
    /// Hours per week available ≥ the threshold.
    #[serde(rename = "horas_disponiveis")]
    HoursAvailable,
    /// The unit has an open training target of the level in the wave.
    #[serde(rename = "unidade_com_meta")]
    UnitTargetOpen,
}

impl CriterionKind {
    pub const ALL: [CriterionKind; 6] = [
        CriterionKind::Tenure,
        CriterionKind::Function,
        CriterionKind::PriorLevel,
        CriterionKind::ManagerRating,
        CriterionKind::HoursAvailable,
        CriterionKind::UnitTargetOpen,
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Criterion {
    pub id: Id,
    pub kind: CriterionKind,
    pub eliminatory: bool,
    pub threshold: f64,
    pub text: String,
    pub weight: f64,
    /// Applies only to nominations of this level (`None`: every level).
    pub level: Option<BeltLevel>,
}

impl Criterion {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            kind: e.typed(fields::KIND).unwrap_or_default(),
            eliminatory: e.boolean(fields::ELIMINATORY),
            threshold: e.number(fields::THRESHOLD).unwrap_or(0.0),
            text: e.text(fields::TEXT),
            weight: e.number(fields::WEIGHT).unwrap_or(1.0).max(0.0),
            level: e.typed(fields::LEVEL),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::NominationCriterion, id)
    }

    fn applies(&self, level: BeltLevel) -> bool {
        self.level.is_none_or(|l| l == level)
    }
}

pub fn criteria(state: &State, program: Id) -> Vec<Criterion> {
    let key = program.to_string();
    let mut v: Vec<Criterion> = live(state, Kind::NominationCriterion)
        .filter(|(_, e)| e.text(fields::PROGRAM) == key)
        .map(|(id, e)| Criterion::read(id, e))
        .collect();
    v.sort_by_key(|c| (!c.eliminatory, c.kind, c.id));
    v
}

#[allow(clippy::too_many_arguments)]
pub fn add_criterion(
    program: Id,
    id: Id,
    kind: CriterionKind,
    eliminatory: bool,
    threshold: f64,
    text: &str,
    weight: f64,
    level: Option<BeltLevel>,
) -> Vec<Edit> {
    let r = Criterion::entity_ref(id);
    let mut e = vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::KIND, enum_value(kind)),
        Edit::new(r, fields::ELIMINATORY, eliminatory),
        Edit::new(r, fields::THRESHOLD, threshold),
        Edit::new(r, fields::TEXT, text.trim()),
        Edit::new(r, fields::WEIGHT, weight.max(0.0)),
    ];
    if let Some(l) = level {
        e.push(Edit::new(r, fields::LEVEL, enum_value(l)));
    }
    e
}

/// The default criteria (the company edits them): six months in the company and, for
/// black belt, the green belt concluded, both eliminatory; the manager's rating (weight
/// 2), the hours available (8 h, weight 1) and a unit with an open target (weight 1),
/// classificatory.
pub fn default_criteria(program: Id, mut ids: impl FnMut() -> Id) -> Vec<Edit> {
    let mut e = Vec::new();
    e.extend(add_criterion(
        program,
        ids(),
        CriterionKind::Tenure,
        true,
        6.0,
        "",
        1.0,
        None,
    ));
    e.extend(add_criterion(
        program,
        ids(),
        CriterionKind::PriorLevel,
        true,
        0.0,
        "",
        1.0,
        Some(BeltLevel::Black),
    ));
    e.extend(add_criterion(
        program,
        ids(),
        CriterionKind::ManagerRating,
        false,
        5.0,
        "",
        2.0,
        None,
    ));
    e.extend(add_criterion(
        program,
        ids(),
        CriterionKind::HoursAvailable,
        false,
        8.0,
        "",
        1.0,
        None,
    ));
    e.extend(add_criterion(
        program,
        ids(),
        CriterionKind::UnitTargetOpen,
        false,
        0.0,
        "",
        1.0,
        None,
    ));
    e
}

// --- Nominations --------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum NominationStatus {
    /// Waiting for the manager or the MBB.
    #[default]
    #[serde(rename = "indicado")]
    Nominated,
    #[serde(rename = "selecionado")]
    Selected,
    #[serde(rename = "espera")]
    Waiting,
    #[serde(rename = "recusado")]
    Refused,
    #[serde(rename = "desistiu")]
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Nomination {
    pub id: Id,
    pub wave: Option<Id>,
    pub class: Option<Id>,
    pub person: Option<Id>,
    pub level: BeltLevel,
    pub by: Option<Id>,
    pub justification: String,
    pub opportunity: Option<Id>,
    pub manager: Option<Id>,
    pub tenure_months: Option<f64>,
    pub function: String,
    pub rating: Option<f64>,
    pub hours_available: Option<f64>,
    pub nominated_on: Option<Date>,
    /// `Some(true)` approved by the manager, `Some(false)` refused.
    pub manager_approval: Option<bool>,
    pub manager_on: Option<Date>,
    pub manager_reason: String,
    pub status: NominationStatus,
    pub position: Option<i64>,
    pub status_reason: String,
    pub decided_by: Option<Id>,
}

impl Nomination {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            wave: e.id(fields::WAVE),
            class: e.id(fields::CLASS),
            person: e.id(fields::PERSON),
            level: e.typed(fields::LEVEL).unwrap_or_default(),
            by: e.id(fields::BY),
            justification: e.text(fields::JUSTIFICATION),
            opportunity: e.id(fields::OPPORTUNITY),
            manager: e.id(fields::MANAGER),
            tenure_months: e.number(fields::TENURE),
            function: e.text(fields::FUNCTION),
            rating: e.number(fields::RATING),
            hours_available: e.number(fields::HOURS_AVAILABLE),
            nominated_on: date_of(e, fields::NOMINATED_ON),
            manager_approval: e.get(fields::MANAGER_APPROVAL).and_then(Value::as_bool),
            manager_on: date_of(e, fields::MANAGER_ON),
            manager_reason: e.text(fields::MANAGER_REASON),
            status: e.typed(fields::STATUS).unwrap_or_default(),
            position: e.integer(fields::POSITION),
            status_reason: e.text(fields::STATUS_REASON),
            decided_by: e.id(fields::DECIDED_BY),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Nomination, id)
    }

    /// Still in the race: not refused nor withdrawn, and not refused by the manager.
    pub fn active(&self) -> bool {
        !matches!(
            self.status,
            NominationStatus::Refused | NominationStatus::Withdrawn
        ) && self.manager_approval != Some(false)
    }

    /// Approved by a manager who is not the nominee.
    pub fn approved(&self) -> bool {
        self.manager_approval == Some(true) && self.manager.is_some() && self.manager != self.person
    }
}

pub fn nominations(state: &State, program: Id) -> Vec<Nomination> {
    let key = program.to_string();
    let mut v: Vec<Nomination> = live(state, Kind::Nomination)
        .filter(|(_, e)| e.text(fields::PROGRAM) == key)
        .map(|(id, e)| Nomination::read(id, e))
        .collect();
    v.sort_by_key(|n| (n.nominated_on, n.id));
    v
}

/// What a nomination records.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NominationInput {
    pub wave: Option<Id>,
    pub person: Option<Id>,
    pub level: BeltLevel,
    pub by: Id,
    pub justification: String,
    pub opportunity: Option<Id>,
    pub manager: Option<Id>,
    pub tenure_months: Option<f64>,
    pub function: String,
    pub rating: Option<f64>,
    pub hours_available: Option<f64>,
    pub date: Option<Date>,
}

/// Edits of a nomination (the person and the manager first: the permission check reads
/// them).
pub fn nominate(
    program: Id,
    id: Id,
    input: &NominationInput,
    existing: &[Nomination],
) -> Result<Vec<Edit>, TrainingError> {
    let person = input.person.ok_or(TrainingError::NoPerson)?;
    if input.justification.trim().is_empty() {
        return Err(TrainingError::NoJustification);
    }
    if input.level >= BeltLevel::Green && input.opportunity.is_none() {
        return Err(TrainingError::NoOpportunity);
    }
    let manager = input
        .manager
        .filter(|m| *m != person)
        .ok_or(TrainingError::BadManager)?;
    if existing.iter().any(|n| {
        n.person == Some(person) && n.level == input.level && n.wave == input.wave && n.active()
    }) {
        return Err(TrainingError::Duplicate);
    }
    let r = Nomination::entity_ref(id);
    let mut e = vec![
        Edit::new(r, fields::PERSON, person.to_string()),
        Edit::new(r, fields::MANAGER, manager.to_string()),
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::LEVEL, enum_value(input.level)),
        Edit::new(r, fields::BY, input.by.to_string()),
        Edit::new(r, fields::JUSTIFICATION, input.justification.trim()),
        Edit::new(r, fields::FUNCTION, input.function.trim()),
        Edit::new(r, fields::STATUS, enum_value(NominationStatus::Nominated)),
    ];
    if let Some(w) = input.wave {
        e.push(Edit::new(r, fields::WAVE, w.to_string()));
    }
    if let Some(o) = input.opportunity {
        e.push(Edit::new(r, fields::OPPORTUNITY, o.to_string()));
    }
    for (f, v) in [
        (fields::TENURE, input.tenure_months),
        (fields::RATING, input.rating),
        (fields::HOURS_AVAILABLE, input.hours_available),
    ] {
        if let Some(x) = v.filter(|x| x.is_finite()) {
            e.push(Edit::new(r, f, x));
        }
    }
    if let Some(d) = input.date {
        e.push(Edit::new(r, fields::NOMINATED_ON, d.to_string()));
    }
    Ok(e)
}

/// The manager approves (committing the hours of class and project in the period) or
/// refuses with a reason. `by` must be the nomination's manager (the MBB's override is a
/// profile matter, checked by [`crate::perfis`]).
pub fn manager_decide(
    n: &Nomination,
    by: Id,
    is_mbb: bool,
    approve: bool,
    reason: &str,
    date: Date,
) -> Result<Vec<Edit>, TrainingError> {
    if !is_mbb && (n.manager != Some(by) || n.person == Some(by)) {
        return Err(TrainingError::NotTheManager);
    }
    if !approve && reason.trim().is_empty() {
        return Err(TrainingError::NoReason);
    }
    let r = Nomination::entity_ref(n.id);
    Ok(vec![
        Edit::new(r, fields::MANAGER_APPROVAL, approve),
        Edit::new(r, fields::MANAGER_ON, date.to_string()),
        Edit::new(r, fields::MANAGER_REASON, reason.trim()),
    ])
}

/// The person withdraws (or the MBB takes the nomination out), with a reason.
pub fn withdraw(n: &Nomination, reason: &str) -> Result<Vec<Edit>, TrainingError> {
    if reason.trim().is_empty() {
        return Err(TrainingError::NoReason);
    }
    let r = Nomination::entity_ref(n.id);
    Ok(vec![
        Edit::new(r, fields::STATUS, enum_value(NominationStatus::Withdrawn)),
        Edit::new(r, fields::STATUS_REASON, reason.trim()),
    ])
}

/// Context of the criteria.
#[derive(Debug, Clone, Default)]
pub struct CriteriaContext<'a> {
    pub members: &'a [Member],
    /// `(unit, level)` with an open training target in the wave.
    pub open_targets: BTreeSet<(Option<Id>, BeltLevel)>,
}

/// The result of one criterion for a nomination.
#[derive(Debug, Clone, PartialEq)]
pub struct CriterionResult {
    pub criterion: Id,
    pub kind: CriterionKind,
    pub eliminatory: bool,
    pub passed: bool,
    /// For a classificatory criterion: its part, 0 to 1.
    pub part: f64,
    /// The value looked at (`None`: not informed).
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Evaluation {
    pub results: Vec<CriterionResult>,
    /// Every eliminatory criterion passed.
    pub passed: bool,
    /// Weighted classificatory score, 0 to 100.
    pub score: f64,
}

impl Evaluation {
    pub fn failures(&self) -> impl Iterator<Item = &CriterionResult> {
        self.results.iter().filter(|r| r.eliminatory && !r.passed)
    }
}

pub fn evaluate(n: &Nomination, cs: &[Criterion], ctx: &CriteriaContext<'_>) -> Evaluation {
    let member = n
        .person
        .and_then(|p| ctx.members.iter().find(|m| m.id == p));
    let mut ev = Evaluation {
        passed: true,
        ..Evaluation::default()
    };
    let (mut num, mut den) = (0.0, 0.0);
    for c in cs.iter().filter(|c| c.applies(n.level)) {
        let (passed, part, value) = match c.kind {
            CriterionKind::Tenure => match n.tenure_months {
                Some(t) => {
                    let part = if c.threshold > 0.0 {
                        (t / (2.0 * c.threshold)).clamp(0.0, 1.0)
                    } else {
                        (t / 60.0).clamp(0.0, 1.0)
                    };
                    (t + 1e-9 >= c.threshold, part, Some(num(t)))
                }
                None => (false, 0.0, None),
            },
            CriterionKind::Function => {
                let allowed: Vec<String> = c
                    .text
                    .split(',')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                let f = n.function.trim().to_lowercase();
                let ok = allowed.is_empty() || allowed.contains(&f);
                (
                    ok,
                    if ok { 1.0 } else { 0.0 },
                    (!f.is_empty()).then(|| n.function.trim().to_string()),
                )
            }
            CriterionKind::PriorLevel => {
                let has = member.and_then(|m| m.level);
                let ok = previous_level(n.level).is_none_or(|need| has.is_some_and(|h| h >= need));
                (
                    ok,
                    if ok { 1.0 } else { 0.0 },
                    has.map(level_key),
                )
            }
            CriterionKind::ManagerRating => match n.rating {
                Some(r) => {
                    let part = (r / 5.0).clamp(0.0, 1.0);
                    (r + 1e-9 >= c.threshold, part, Some(num(r)))
                }
                None => (false, 0.0, None),
            },
            CriterionKind::HoursAvailable => match n.hours_available {
                Some(h) => {
                    let part = if c.threshold > 0.0 {
                        (h / c.threshold).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    (h + 1e-9 >= c.threshold, part, Some(num(h)))
                }
                None => (false, 0.0, None),
            },
            CriterionKind::UnitTargetOpen => {
                let unit = member.and_then(|m| m.unit);
                let ok = ctx.open_targets.contains(&(unit, n.level))
                    || ctx.open_targets.contains(&(None, n.level));
                (ok, if ok { 1.0 } else { 0.0 }, None)
            }
        };
        if c.eliminatory {
            ev.passed &= passed;
        } else {
            num += c.weight * part;
            den += c.weight;
        }
        ev.results.push(CriterionResult {
            criterion: c.id,
            kind: c.kind,
            eliminatory: c.eliminatory,
            passed,
            part,
            value,
        });
    }
    ev.score = if den > 0.0 { 100.0 * num / den } else { 0.0 };
    ev
}

/// The `(unit, level)` pairs of a wave whose target is not reached yet by the selected
/// trainees.
pub fn open_targets(
    wave: &Wave,
    noms: &[Nomination],
    members: &[Member],
) -> BTreeSet<(Option<Id>, BeltLevel)> {
    let unit_of = |p: Option<Id>| p.and_then(|p| members.iter().find(|m| m.id == p)?.unit);
    wave.targets
        .iter()
        .filter(|((level, unit), n)| {
            let have = noms
                .iter()
                .filter(|x| x.wave == Some(wave.id) && x.level == *level)
                .filter(|x| x.status == NominationStatus::Selected)
                .filter(|x| unit.is_none() || unit_of(x.person) == *unit)
                .count();
            (have as u32) < **n
        })
        .map(|((l, u), _)| (*u, *l))
        .collect()
}

/// The capacity check of a nomination (section 8.5, step 4).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CapacityCheck {
    pub training_hours_week: f64,
    /// Hours of the training project (none for yellow belt, who joins a project).
    pub project_hours_week: f64,
    /// Free hours of the person ([`Load::free_hours`]; the nomination's hours when the
    /// program has no capacity recorded).
    pub free_hours: f64,
    pub fits: bool,
}

pub fn capacity_check(
    n: &Nomination,
    class: &Class,
    sessions: &[ClassSession],
    load: Option<&Load>,
) -> CapacityCheck {
    let training = training_hours_week(class, sessions);
    let project = if n.level >= BeltLevel::Green {
        designacao::default_hours(n.level)
    } else {
        0.0
    };
    let free = match load.filter(|l| l.capacity > 0.0) {
        Some(l) => l.free_hours(),
        None => n.hours_available.unwrap_or(0.0),
    };
    CapacityCheck {
        training_hours_week: training,
        project_hours_week: project,
        free_hours: free,
        fits: training + project <= free + 1e-9,
    }
}

/// A nomination in the selection of a class.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub nomination: Id,
    pub person: Option<Id>,
    pub evaluation: Evaluation,
    pub capacity: CapacityCheck,
    /// Approved by the manager, every eliminatory criterion passed and fits the capacity.
    pub eligible: bool,
}

/// The nominations a class can take: of its wave and level, approved by the manager,
/// still active and not selected for another class.
pub fn candidates_for<'a>(class: &Class, noms: &'a [Nomination]) -> Vec<&'a Nomination> {
    noms.iter()
        .filter(|n| n.level == class.level && n.wave == class.wave && n.active())
        .filter(|n| n.class.is_none() || n.class == Some(class.id))
        .collect()
}

/// The ranking of the selection: eligible first, then the classificatory score (higher
/// first), then the nomination date and the id, so the same input gives the same order.
pub fn rank(
    class: &Class,
    noms: &[Nomination],
    cs: &[Criterion],
    ctx: &CriteriaContext<'_>,
    sessions: &[ClassSession],
    loads: &[Load],
) -> Vec<Ranked> {
    let mut v: Vec<(Ranked, Option<Date>)> = candidates_for(class, noms)
        .into_iter()
        .map(|n| {
            let ev = evaluate(n, cs, ctx);
            let load = n.person.and_then(|p| loads.iter().find(|l| l.person == p));
            let cap = capacity_check(n, class, sessions, load);
            let eligible = n.approved() && ev.passed && cap.fits;
            (
                Ranked {
                    nomination: n.id,
                    person: n.person,
                    evaluation: ev,
                    capacity: cap,
                    eligible,
                },
                n.nominated_on,
            )
        })
        .collect();
    v.sort_by(|(a, da), (b, db)| {
        b.eligible
            .cmp(&a.eligible)
            .then(b.evaluation.score.total_cmp(&a.evaluation.score))
            .then(da.cmp(db))
            .then(a.nomination.cmp(&b.nomination))
    });
    v.into_iter().map(|(r, _)| r).collect()
}

/// What the MBB is asked to confirm: the selected up to the seats, the waiting list in
/// order and the ones out (with the reason in their ranking).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Proposal {
    pub selected: Vec<Id>,
    pub waiting: Vec<Id>,
    pub out: Vec<Id>,
}

pub fn proposal(ranked: &[Ranked], seats: u32) -> Proposal {
    let mut p = Proposal::default();
    for r in ranked {
        if !r.eligible {
            p.out.push(r.nomination);
        } else if (p.selected.len() as u32) < seats {
            p.selected.push(r.nomination);
        } else {
            p.waiting.push(r.nomination);
        }
    }
    p
}

/// Edits that confirm a proposal: the selected get the class, the waiting list its
/// positions (1, 2...), the ones out stay nominated (they may be approved later).
pub fn confirm_selection(class: Id, p: &Proposal, by: Id, date: Date) -> Vec<Edit> {
    let mut e = Vec::new();
    let mut put = |n: Id, status: NominationStatus, pos: Option<usize>| {
        let r = Nomination::entity_ref(n);
        e.push(Edit::new(r, fields::STATUS, enum_value(status)));
        e.push(Edit::new(r, fields::CLASS, class.to_string()));
        e.push(Edit::new(
            r,
            fields::POSITION,
            pos.map_or(Value::Null, |p| Value::from(p as i64 + 1)),
        ));
        e.push(Edit::new(r, fields::DECIDED_BY, by.to_string()));
        e.push(Edit::new(r, fields::DECIDED_ON, date.to_string()));
    };
    for n in &p.selected {
        put(*n, NominationStatus::Selected, None);
    }
    for (i, n) in p.waiting.iter().enumerate() {
        put(*n, NominationStatus::Waiting, Some(i));
    }
    e
}

/// The selected trainees of a class.
pub fn students(noms: &[Nomination], class: Id) -> Vec<Id> {
    let mut v: Vec<Id> = noms
        .iter()
        .filter(|n| n.class == Some(class) && n.status == NominationStatus::Selected)
        .filter_map(|n| n.person)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The waiting list of a class, in order.
pub fn waiting_list(noms: &[Nomination], class: Id) -> Vec<&Nomination> {
    let mut v: Vec<&Nomination> = noms
        .iter()
        .filter(|n| n.class == Some(class) && n.status == NominationStatus::Waiting)
        .collect();
    v.sort_by_key(|n| (n.position.unwrap_or(i64::MAX), n.id));
    v
}

/// When a seat is free, the first of the waiting list takes it.
pub fn promote(class: &Class, noms: &[Nomination], by: Id, date: Date) -> Vec<Edit> {
    let taken = students(noms, class.id).len() as u32;
    if taken >= class.seats {
        return Vec::new();
    }
    let Some(first) = waiting_list(noms, class.id).first().map(|n| n.id) else {
        return Vec::new();
    };
    let r = Nomination::entity_ref(first);
    vec![
        Edit::new(r, fields::STATUS, enum_value(NominationStatus::Selected)),
        Edit::new(r, fields::POSITION, Value::Null),
        Edit::new(r, fields::DECIDED_BY, by.to_string()),
        Edit::new(r, fields::DECIDED_ON, date.to_string()),
    ]
}

// --- Training groups ----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: Id,
    pub class: Option<Id>,
    pub name: String,
    pub members: Vec<Id>,
    pub confirmed_by: Option<Id>,
    pub confirmed_on: Option<Date>,
}

impl Group {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            class: e.id(fields::CLASS),
            name: e.text(fields::NAME),
            members: ids_in(e, fields::MEMBERS),
            confirmed_by: e.id(fields::CONFIRMED_BY),
            confirmed_on: date_of(e, fields::CONFIRMED_ON),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::TrainingGroup, id)
    }
}

pub fn groups(state: &State, class: Id) -> Vec<Group> {
    let mut v: Vec<Group> = live(state, Kind::TrainingGroup)
        .map(|(id, e)| Group::read(id, e))
        .filter(|g| g.class == Some(class))
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    v
}

/// Edits that replace the groups of a class by `new` (the old ones are deleted). The
/// names are "Grupo 1", "Grupo 2"...
pub fn save_groups(
    class: Id,
    old: &[Group],
    new: &[Vec<Id>],
    mut ids: impl FnMut() -> Id,
) -> Vec<Edit> {
    let mut e: Vec<Edit> = old
        .iter()
        .map(|g| Edit::new(Group::entity_ref(g.id), FIELD_DELETED, true))
        .collect();
    for (i, members) in new.iter().enumerate() {
        let r = Group::entity_ref(ids());
        e.push(Edit::new(r, fields::CLASS, class.to_string()));
        e.push(Edit::new(r, fields::NAME, format!("Grupo {}", i + 1)));
        e.push(Edit::new(r, fields::MEMBERS, ids_value(members)));
    }
    e
}

/// Edits that move a trainee to another group.
pub fn move_to_group(groups: &[Group], person: Id, to: Id) -> Vec<Edit> {
    let mut e = Vec::new();
    for g in groups {
        let has = g.members.contains(&person);
        if g.id == to && !has {
            let mut m = g.members.clone();
            m.push(person);
            e.push(Edit::new(
                Group::entity_ref(g.id),
                fields::MEMBERS,
                ids_value(&m),
            ));
        } else if g.id != to && has {
            let m: Vec<Id> = g.members.iter().copied().filter(|x| *x != person).collect();
            e.push(Edit::new(
                Group::entity_ref(g.id),
                fields::MEMBERS,
                ids_value(&m),
            ));
        }
    }
    e
}

/// The MBB confirms the groups of a class.
pub fn confirm_groups(groups: &[Group], by: Id, date: Date) -> Vec<Edit> {
    groups
        .iter()
        .flat_map(|g| {
            let r = Group::entity_ref(g.id);
            [
                Edit::new(r, fields::CONFIRMED_BY, by.to_string()),
                Edit::new(r, fields::CONFIRMED_ON, date.to_string()),
            ]
        })
        .collect()
}

/// The group rules of the program (group size 5 and at most 2 of the same area by
/// default).
pub fn group_rules(state: &State, program: Id, level: BeltLevel) -> teams::Rules {
    let e = state.entity(program);
    teams::Rules {
        group_size: e
            .and_then(|e| e.integer(fields::GROUP_SIZE))
            .filter(|n| *n >= 2)
            .map_or(teams::DEFAULT_GROUP_SIZE, |n| n.min(50) as usize),
        max_same_area: e
            .and_then(|e| e.integer(fields::MAX_SAME_AREA))
            .filter(|n| *n >= 1)
            .map_or(teams::DEFAULT_MAX_SAME_AREA, |n| n.min(50) as usize),
        need_project: level >= BeltLevel::Green,
    }
}

/// The trainees of a class as the team suggestion sees them: unit and area from the
/// program's people, the function from the nomination, a project when the person is
/// assigned or belt of a real project, a champion when that project (or the unit) has
/// one.
pub fn trainees(
    class: &Class,
    noms: &[Nomination],
    members: &[Member],
    assignments: &[Assignment],
    projects: &[programa::ProjectRecord],
    units: &[programa::Unit],
) -> Vec<teams::Trainee> {
    let mut v: Vec<teams::Trainee> = noms
        .iter()
        .filter(|n| n.class == Some(class.id) && n.status == NominationStatus::Selected)
        .filter_map(|n| {
            let p = n.person?;
            let m = members.iter().find(|m| m.id == p);
            let running: Vec<&programa::ProjectRecord> = projects
                .iter()
                .filter(|r| !r.training && r.belts.contains(&p))
                .collect();
            let assigned = assignments
                .iter()
                .any(|a| a.person == Some(p) && !a.ended);
            let unit = m.and_then(|m| m.unit);
            let champion = running.iter().any(|r| !r.champions.is_empty())
                || units
                    .iter()
                    .any(|u| Some(u.id) == unit && u.champion.is_some());
            Some(teams::Trainee {
                id: p,
                unit,
                area: m.map(|m| m.area.trim().to_lowercase()).unwrap_or_default(),
                function: n.function.trim().to_lowercase(),
                has_project: assigned || !running.is_empty() || n.opportunity.is_some(),
                has_champion: champion,
                avoid: Vec::new(),
            })
        })
        .collect();
    v.sort_by_key(|t| t.id);
    v.dedup_by_key(|t| t.id);
    v
}

// --- Learning feeds the competence map ------------------------------------------------------

/// The `estudou` and `praticou` entries the training gives (never written, recomputed):
/// a trainee who attended every held session of a module studied its tools; a correct
/// exercise in a session means practice of the tools of its modules.
pub fn learning_entries(
    sessions: &[ClassSession],
    modules: &[curriculum::Module],
) -> Vec<mentoria::Entry> {
    let mut out = Vec::new();
    // Module → the held sessions that cover it.
    let mut by_module: BTreeMap<Id, Vec<&ClassSession>> = BTreeMap::new();
    for s in sessions.iter().filter(|s| s.held()) {
        for m in &s.modules {
            by_module.entry(*m).or_default().push(s);
        }
    }
    for (mid, ss) in &by_module {
        let Some(module) = modules.iter().find(|m| m.id == *mid) else {
            continue;
        };
        let people: BTreeSet<Id> = ss
            .iter()
            .flat_map(|s| s.attendance.keys().copied())
            .collect();
        for p in people {
            let all = ss
                .iter()
                .all(|s| s.attendance.get(&p) == Some(&Attendance::Present));
            if !all {
                continue;
            }
            let last = ss.iter().max_by_key(|s| (s.date, s.id)).copied();
            for tool in &module.tools {
                out.push(mentoria::Entry {
                    id: last.map_or(*mid, |s| s.id),
                    person: Some(p),
                    tool: tool.clone(),
                    level: SkillLevel::Studied,
                    source: Source::Module,
                    evidence: module.name.clone(),
                    date: last.and_then(|s| s.date),
                    by: last.and_then(|s| s.instructor),
                });
            }
        }
    }
    for s in sessions {
        for p in &s.exercises {
            for mid in &s.modules {
                let Some(module) = modules.iter().find(|m| m.id == *mid) else {
                    continue;
                };
                for tool in &module.tools {
                    out.push(mentoria::Entry {
                        id: s.id,
                        person: Some(*p),
                        tool: tool.clone(),
                        level: SkillLevel::Practiced,
                        source: Source::Exercise,
                        evidence: module.name.clone(),
                        date: s.date,
                        by: s.instructor,
                    });
                }
            }
        }
    }
    out
}

// --- Instructors ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Instructor {
    pub id: Id,
    pub person: Option<Id>,
    /// The modules the instructor is qualified to give.
    pub modules: Vec<Id>,
}

impl Instructor {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            person: e.id(fields::PERSON),
            modules: ids_in(e, fields::MODULES),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Instructor, id)
    }
}

pub fn instructors(state: &State, program: Id) -> Vec<Instructor> {
    let key = program.to_string();
    let mut v: Vec<Instructor> = live(state, Kind::Instructor)
        .filter(|(_, e)| e.text(fields::PROGRAM) == key)
        .map(|(id, e)| Instructor::read(id, e))
        .collect();
    v.sort_by_key(|i| (i.person, i.id));
    v
}

pub fn add_instructor(program: Id, id: Id, person: Id, modules: &[Id]) -> Vec<Edit> {
    let r = Instructor::entity_ref(id);
    vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::PERSON, person.to_string()),
        Edit::new(r, fields::MODULES, ids_value(modules)),
    ]
}

/// Hours given by an instructor in the sessions of `[from, to]` (they count for the
/// instructor's own renewal).
pub fn hours_given(sessions: &[ClassSession], person: Id, from: Date, to: Date) -> f64 {
    sessions
        .iter()
        .filter(|s| s.instructor == Some(person) && s.held())
        .filter(|s| s.date.is_some_and(|d| d >= from && d <= to))
        .map(|s| s.hours)
        .sum()
}

/// Sessions given by someone not qualified for one of their modules: `(session, module)`.
pub fn unqualified(sessions: &[ClassSession], instructors: &[Instructor]) -> Vec<(Id, Id)> {
    let mut v = Vec::new();
    for s in sessions {
        let Some(p) = s.instructor else { continue };
        let q: Vec<Id> = instructors
            .iter()
            .filter(|i| i.person == Some(p))
            .flat_map(|i| i.modules.iter().copied())
            .collect();
        for m in &s.modules {
            if !q.contains(m) {
                v.push((s.id, *m));
            }
        }
    }
    v
}

/// The schedule of an instructor: their sessions, by date.
pub fn agenda_of(sessions: &[ClassSession], person: Id) -> Vec<&ClassSession> {
    sessions
        .iter()
        .filter(|s| s.instructor == Some(person))
        .collect()
}

// --- Certification projects on the committee's agenda ------------------------------------------

/// A trainee who needs a certification project.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRequest {
    pub nomination: Id,
    pub person: Id,
    pub level: BeltLevel,
    pub opportunity: Option<Id>,
}

/// The selected green and black belt trainees without an assignment and whose candidate
/// opportunity is not selected yet: the requests of certification projects for the
/// selection committee (section 8.1).
pub fn certification_requests(
    noms: &[Nomination],
    opps: &[Opportunity],
    assignments: &[Assignment],
) -> Vec<ProjectRequest> {
    noms.iter()
        .filter(|n| n.status == NominationStatus::Selected && n.level >= BeltLevel::Green)
        .filter_map(|n| {
            let p = n.person?;
            if assignments
                .iter()
                .any(|a| a.person == Some(p) && !a.ended)
            {
                return None;
            }
            let selected = n
                .opportunity
                .and_then(|o| opps.iter().find(|x| x.id == o || x.idea == Some(o)))
                .is_some_and(|o| o.decision == Some(programa::Choice::Select));
            (!selected).then_some(ProjectRequest {
                nomination: n.id,
                person: p,
                level: n.level,
                opportunity: n.opportunity,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
