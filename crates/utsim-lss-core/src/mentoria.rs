//! Coaching and the growth of the belts (`docs/plano_lean_seis_sigma.md`, 8.3; decision
//! MB8 for the personal data).
//!
//! - **Coaching log** of each belt: sessions (date, duration, form, project and phase,
//!   topics, tools discussed, difficulties, an optional 1 to 4 rating per competence) and
//!   commitments, which become cards on the project's board or stay on the belt's
//!   personal list. The log lives in a folder of its own per belt (`mentoria/<belt>.lss`),
//!   shared on the Drive only with the MBB and the belt; the app shows it only to them
//!   ([`can_see`]). A session needs the belt's recorded consent.
//! - **Competence map**: belt × tool with four levels, `estudou` (module done, stage L7),
//!   `praticou` (correct exercise, stage L8), `aplicou` (the tool is in an approved
//!   deliverable of a real project: filled here from the project records) and
//!   `demonstrou` (confirmed by the MBB). Every cell keeps its evidence.
//! - **Timeline** per belt: sessions, approved tollgates, new tools and certification.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::domain::{Column, enum_value};
use crate::id::Id;
use crate::kanban;
use crate::op::{Edit, EntityRef, Kind};
use crate::programa::{Member, ProjectRecord};
use crate::state::{Entity, State};
use crate::tollgate::TollgateStatus;
use crate::tools::Tool;

pub mod fields {
    pub const PROGRAM: &str = "programa";
    pub const BELT: &str = "belt";
    pub const MENTOR: &str = "mentor";
    pub const DATE: &str = "data";
    pub const MINUTES: &str = "duracao_min";
    pub const FORM: &str = "forma";
    pub const PROJECT: &str = "projeto";
    pub const PHASE: &str = "fase";
    pub const TOPICS: &str = "temas";
    /// Array of tool keys.
    pub const TOOLS: &str = "ferramentas";
    pub const DIFFICULTIES: &str = "dificuldades";
    /// `avaliacao_<tool>`: 1 to 4.
    pub const RATING_PREFIX: &str = "avaliacao_";
    // Commitment.
    pub const SESSION: &str = "sessao";
    pub const TEXT: &str = "texto";
    pub const DUE: &str = "prazo";
    pub const TARGET: &str = "destino";
    pub const CARD: &str = "cartao";
    pub const DONE: &str = "feito";
    // Competence entry (program journal).
    pub const PERSON: &str = "pessoa";
    pub const TOOL: &str = "ferramenta";
    pub const LEVEL: &str = "nivel";
    pub const SOURCE: &str = "origem";
    pub const EVIDENCE: &str = "evidencia";
    pub const BY: &str = "por";
}

/// The tools of the competence map: the deliverable tools of the app (section 2, their
/// file keys) and the analyses and methods of the later stages, so the map, the
/// curriculum (L7) and the exercises (L8) share one list.
pub const EXTRA_TOOLS: [&str; 10] = [
    "termo",
    "cronograma",
    "msa",
    "capacidade",
    "cep",
    "testes_hipotese",
    "regressao",
    "doe",
    "vsm",
    "kaizen",
];

/// File key of a deliverable tool (`fmea`, `sipoc`...).
pub fn tool_key(t: Tool) -> String {
    enum_value(t).as_str().unwrap_or_default().to_string()
}

/// Every tool key of the map, in screen order.
pub fn all_tools() -> Vec<String> {
    Tool::ALL
        .iter()
        .map(|t| tool_key(*t))
        .chain(EXTRA_TOOLS.iter().map(|s| s.to_string()))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Form {
    #[default]
    #[serde(rename = "individual")]
    Individual,
    #[serde(rename = "grupo")]
    Group,
    #[serde(rename = "gemba")]
    Gemba,
}

impl Form {
    pub const ALL: [Form; 3] = [Form::Individual, Form::Group, Form::Gemba];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: Id,
    pub belt: Option<Id>,
    pub mentor: Option<Id>,
    pub date: Option<Date>,
    pub minutes: f64,
    pub form: Form,
    pub project: Option<Id>,
    pub phase: String,
    pub topics: String,
    pub tools: Vec<String>,
    pub difficulties: String,
    pub ratings: BTreeMap<String, u8>,
}

impl Session {
    pub fn read(id: Id, e: &Entity) -> Self {
        let ratings = e
            .fields
            .iter()
            .filter_map(|(k, v)| {
                let tool = k.strip_prefix(fields::RATING_PREFIX)?;
                let r = v.value.as_i64().filter(|r| (1..=4).contains(r))?;
                Some((tool.to_string(), r as u8))
            })
            .collect();
        Self {
            id,
            belt: e.id(fields::BELT),
            mentor: e.id(fields::MENTOR),
            date: e.opt_text(fields::DATE).and_then(|s| s.parse().ok()),
            minutes: e.number(fields::MINUTES).unwrap_or(0.0).max(0.0),
            form: e.typed(fields::FORM).unwrap_or_default(),
            project: e.id(fields::PROJECT),
            phase: e.text(fields::PHASE),
            topics: e.text(fields::TOPICS),
            tools: e
                .get(fields::TOOLS)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            difficulties: e.text(fields::DIFFICULTIES),
            ratings,
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::CoachingSession, id)
    }
}

pub fn rating_field(tool: &str) -> String {
    format!("{}{}", fields::RATING_PREFIX, tool)
}

/// Sessions of the log, oldest first.
pub fn sessions(state: &State) -> Vec<Session> {
    let mut v: Vec<Session> = state
        .of_kind(Kind::CoachingSession)
        .map(|(id, e)| Session::read(id, e))
        .collect();
    v.sort_by_key(|s| (s.date, s.id));
    v
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoachingError {
    #[error("sem o consentimento registrado do belt, o diário de mentoria não guarda dados dele")]
    NoConsent,
    #[error("a pessoa não é belt do programa")]
    NotABelt,
    #[error("escreva o compromisso")]
    EmptyCommitment,
    #[error("o compromisso já virou cartão")]
    AlreadyCard,
    #[error("o compromisso não está ligado a um projeto")]
    NoProject,
}

/// What a new session records.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionInput {
    pub belt: Id,
    pub mentor: Id,
    pub date: Date,
    pub minutes: f64,
    pub form: Form,
    pub project: Option<Id>,
    pub phase: String,
    pub topics: String,
    pub tools: Vec<String>,
}

/// Edits that record a session in the belt's log. The belt must be a belt of the program
/// with consent recorded (MB8).
pub fn new_session(
    belt: &Member,
    id: Id,
    input: &SessionInput,
) -> Result<Vec<Edit>, CoachingError> {
    if !belt.is_belt() {
        return Err(CoachingError::NotABelt);
    }
    if !belt.consent {
        return Err(CoachingError::NoConsent);
    }
    let r = Session::entity_ref(id);
    let tools: Vec<Value> = input
        .tools
        .iter()
        .map(|t| Value::from(t.as_str()))
        .collect();
    let mut e = vec![
        Edit::new(r, fields::BELT, input.belt.to_string()),
        Edit::new(r, fields::MENTOR, input.mentor.to_string()),
        Edit::new(r, fields::DATE, input.date.to_string()),
        Edit::new(r, fields::MINUTES, input.minutes.max(0.0)),
        Edit::new(r, fields::FORM, enum_value(input.form)),
        Edit::new(r, fields::PHASE, input.phase.trim()),
        Edit::new(r, fields::TOPICS, input.topics.trim()),
        Edit::new(r, fields::TOOLS, Value::from(tools)),
    ];
    if let Some(p) = input.project {
        e.push(Edit::new(r, fields::PROJECT, p.to_string()));
    }
    Ok(e)
}

/// Where a commitment goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Target {
    /// A card on the board of the session's project.
    #[serde(rename = "projeto")]
    Project,
    /// The belt's personal list (stays in the log).
    #[default]
    #[serde(rename = "pessoal")]
    Personal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Commitment {
    pub id: Id,
    pub session: Option<Id>,
    pub text: String,
    pub due: Option<Date>,
    pub target: Target,
    pub card: Option<Id>,
    pub done: bool,
}

impl Commitment {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            session: e.id(fields::SESSION),
            text: e.text(fields::TEXT),
            due: e.opt_text(fields::DUE).and_then(|s| s.parse().ok()),
            target: e.typed(fields::TARGET).unwrap_or_default(),
            card: e.id(fields::CARD),
            done: e.boolean(fields::DONE),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Commitment, id)
    }
}

pub fn commitments(state: &State) -> Vec<Commitment> {
    let mut v: Vec<Commitment> = state
        .of_kind(Kind::Commitment)
        .map(|(id, e)| Commitment::read(id, e))
        .collect();
    v.sort_by_key(|c| (c.due, c.id));
    v
}

pub fn add_commitment(
    session: Id,
    id: Id,
    text: &str,
    due: Option<Date>,
    target: Target,
) -> Result<Vec<Edit>, CoachingError> {
    if text.trim().is_empty() {
        return Err(CoachingError::EmptyCommitment);
    }
    let r = Commitment::entity_ref(id);
    let mut e = vec![
        Edit::new(r, fields::SESSION, session.to_string()),
        Edit::new(r, fields::TEXT, text.trim()),
        Edit::new(r, fields::TARGET, enum_value(target)),
    ];
    if let Some(d) = due {
        e.push(Edit::new(r, fields::DUE, d.to_string()));
    }
    Ok(e)
}

/// A commitment turned into an action: the card edits for the project's journal and the
/// link edit for the coaching log. The card carries the text, the due date and the belt
/// as responsible.
pub fn commitment_card(
    project_state: &State,
    project: Id,
    commitment: &Commitment,
    belt: Id,
    card: Id,
) -> Result<(Vec<Edit>, Edit), CoachingError> {
    if commitment.card.is_some() {
        return Err(CoachingError::AlreadyCard);
    }
    let mut e = kanban::new_card(project_state, project, card, &commitment.text, Column::Todo);
    let a = EntityRef::new(Kind::Action, card);
    use crate::domain::fields::action as af;
    e.push(Edit::new(a, af::WHO, belt.to_string()));
    if let Some(d) = commitment.due {
        e.push(Edit::new(a, af::DUE, d.to_string()));
    }
    let link = Edit::new(
        Commitment::entity_ref(commitment.id),
        fields::CARD,
        card.to_string(),
    );
    Ok((e, link))
}

/// The progress of a belt in the log.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Progress {
    pub sessions: usize,
    pub hours: f64,
    pub commitments: usize,
    pub done: usize,
    /// For each rated competence: first and last rating.
    pub ratings: BTreeMap<String, (u8, u8)>,
}

/// Progress of the log; a commitment that became a card counts as done when the card is
/// in "feito" (`card_done`, from the project) or when it is marked done in the log.
pub fn progress(state: &State, card_done: impl Fn(Id) -> bool) -> Progress {
    let ss = sessions(state);
    let cs = commitments(state);
    let mut ratings: BTreeMap<String, (u8, u8)> = BTreeMap::new();
    for s in &ss {
        for (t, r) in &s.ratings {
            ratings
                .entry(t.clone())
                .and_modify(|x| x.1 = *r)
                .or_insert((*r, *r));
        }
    }
    Progress {
        sessions: ss.len(),
        hours: ss.iter().map(|s| s.minutes).sum::<f64>() / 60.0,
        commitments: cs.len(),
        done: cs
            .iter()
            .filter(|c| c.done || c.card.is_some_and(&card_done))
            .count(),
        ratings,
    }
}

/// Whether `viewer` may see the coaching log of `belt` (MB8): the MBB and the belt.
pub fn can_see(viewer: Id, belt: Id, is_mbb: bool) -> bool {
    is_mbb || viewer == belt
}

// --- Competence map -------------------------------------------------------------------

/// Levels of the competence map, in increasing order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Level {
    #[default]
    #[serde(rename = "estudou")]
    Studied,
    #[serde(rename = "praticou")]
    Practiced,
    #[serde(rename = "aplicou")]
    Applied,
    #[serde(rename = "demonstrou")]
    Demonstrated,
}

impl Level {
    pub const ALL: [Level; 4] = [
        Level::Studied,
        Level::Practiced,
        Level::Applied,
        Level::Demonstrated,
    ];
}

/// Where an entry of the map came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Source {
    /// A module of the curriculum concluded (L7).
    #[serde(rename = "modulo")]
    Module,
    /// A correct exercise in teaching mode (L8).
    #[serde(rename = "exercicio")]
    Exercise,
    /// Confirmed by the MBB in a review or coaching session.
    #[default]
    #[serde(rename = "mbb")]
    Mbb,
}

/// An entry of the program journal that sets a level (all but `aplicou`, which comes from
/// the projects).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: Id,
    pub person: Option<Id>,
    pub tool: String,
    pub level: Level,
    pub source: Source,
    pub evidence: String,
    pub date: Option<Date>,
    pub by: Option<Id>,
}

impl Entry {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            person: e.id(fields::PERSON),
            tool: e.text(fields::TOOL),
            level: e.typed(fields::LEVEL).unwrap_or_default(),
            source: e.typed(fields::SOURCE).unwrap_or_default(),
            evidence: e.text(fields::EVIDENCE),
            date: e.opt_text(fields::DATE).and_then(|s| s.parse().ok()),
            by: e.id(fields::BY),
        }
    }
}

pub fn entries(state: &State) -> Vec<Entry> {
    state
        .of_kind(Kind::Competence)
        .map(|(id, e)| Entry::read(id, e))
        .collect()
}

/// Edits of an entry: the level first, so the permission check knows it.
#[allow(clippy::too_many_arguments)]
pub fn record_level(
    program: Id,
    id: Id,
    person: Id,
    tool: &str,
    level: Level,
    source: Source,
    evidence: &str,
    by: Id,
    date: Date,
) -> Vec<Edit> {
    let r = EntityRef::new(Kind::Competence, id);
    vec![
        Edit::new(r, fields::LEVEL, enum_value(level)),
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::PERSON, person.to_string()),
        Edit::new(r, fields::TOOL, tool),
        Edit::new(r, fields::SOURCE, enum_value(source)),
        Edit::new(r, fields::EVIDENCE, evidence.trim()),
        Edit::new(r, fields::BY, by.to_string()),
        Edit::new(r, fields::DATE, date.to_string()),
    ]
}

/// The evidence of a level in a cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Evidence {
    /// An entry of the program journal.
    Entry { id: Id, level: Level, text: String },
    /// An approved deliverable of a real project.
    Deliverable { project: Id, deliverable: Id },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    pub level: Option<Level>,
    pub evidence: Vec<Evidence>,
}

/// The map: `(person, tool)` → the highest level reached, with every evidence.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompetenceMap {
    pub cells: BTreeMap<(Id, String), Cell>,
}

impl CompetenceMap {
    pub fn level(&self, person: Id, tool: &str) -> Option<Level> {
        self.cells
            .get(&(person, tool.to_string()))
            .and_then(|c| c.level)
    }

    /// Tools of a person at `aplicou` or above.
    pub fn applied(&self, person: Id) -> Vec<&str> {
        self.cells
            .iter()
            .filter(|((p, _), c)| *p == person && c.level >= Some(Level::Applied))
            .map(|((_, t), _)| t.as_str())
            .collect()
    }

    fn raise(&mut self, person: Id, tool: &str, level: Level, ev: Evidence) {
        let c = self.cells.entry((person, tool.to_string())).or_default();
        if c.level < Some(level) {
            c.level = Some(level);
        }
        c.evidence.push(ev);
    }
}

/// Builds the map from the journal's entries and the project records: every belt of a
/// real (not training) project whose tool deliverable was approved at the tollgate gets
/// `aplicou` for that tool. `demonstrou` only counts from an entry of an MBB
/// (`is_mbb`); an entry of anyone else at that level is ignored.
pub fn competence_map(
    entries: &[Entry],
    projects: &[ProjectRecord],
    is_mbb: impl Fn(Id) -> bool,
) -> CompetenceMap {
    let mut m = CompetenceMap::default();
    for p in projects.iter().filter(|p| !p.training) {
        for u in p.tools.iter().filter(|u| u.approved) {
            let key = tool_key(u.tool);
            for b in &p.belts {
                m.raise(
                    *b,
                    &key,
                    Level::Applied,
                    Evidence::Deliverable {
                        project: p.id,
                        deliverable: u.deliverable,
                    },
                );
            }
        }
    }
    for e in entries {
        let Some(person) = e.person else { continue };
        if e.tool.is_empty() {
            continue;
        }
        if e.level == Level::Demonstrated && !e.by.is_some_and(&is_mbb) {
            continue;
        }
        if e.level == Level::Applied {
            // `aplicou` only comes from an approved deliverable of a real project.
            continue;
        }
        m.raise(
            person,
            &e.tool,
            e.level,
            Evidence::Entry {
                id: e.id,
                level: e.level,
                text: e.evidence.clone(),
            },
        );
    }
    m
}

/// One event of a belt's timeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Session {
        date: Date,
        topics: String,
    },
    TollgateApproved {
        date: Date,
        project: Id,
        phase: String,
    },
    NewTool {
        date: Date,
        tool: String,
    },
    Certified {
        date: Date,
    },
}

impl Event {
    pub fn date(&self) -> Date {
        match self {
            Event::Session { date, .. }
            | Event::TollgateApproved { date, .. }
            | Event::NewTool { date, .. }
            | Event::Certified { date } => *date,
        }
    }
}

/// The timeline of a belt: sessions (when the log is visible), approved tollgates of the
/// belt's projects, tools confirmed by entries and the certification, by date.
pub fn timeline(
    belt: &Member,
    sessions: &[Session],
    projects: &[ProjectRecord],
    entries: &[Entry],
) -> Vec<Event> {
    let mut v = Vec::new();
    for s in sessions.iter().filter(|s| s.belt == Some(belt.id)) {
        if let Some(date) = s.date {
            v.push(Event::Session {
                date,
                topics: s.topics.clone(),
            });
        }
    }
    for p in projects
        .iter()
        .filter(|p| !p.training && p.belts.contains(&belt.id))
    {
        for t in p
            .tollgates
            .iter()
            .filter(|t| t.status == TollgateStatus::Approved)
        {
            if let Some(date) = t.approved_on {
                v.push(Event::TollgateApproved {
                    date,
                    project: p.id,
                    phase: t.phase_name.clone(),
                });
            }
        }
    }
    let mut first: BTreeMap<&str, Date> = BTreeMap::new();
    for e in entries.iter().filter(|e| e.person == Some(belt.id)) {
        if let Some(d) = e.date {
            first
                .entry(e.tool.as_str())
                .and_modify(|x| *x = (*x).min(d))
                .or_insert(d);
        }
    }
    for (tool, date) in first {
        v.push(Event::NewTool {
            date,
            tool: tool.to_string(),
        });
    }
    if let Some(date) = belt.certified_on {
        v.push(Event::Certified { date });
    }
    v.sort_by_key(|e| e.date());
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ProjectStatus;
    use crate::programa::testkit::*;
    use crate::programa::{BeltLevel, ToolUse};

    fn belt(consent: bool) -> Member {
        Member {
            id: Id::v7(1, [1; 10]),
            name: "Ana".into(),
            level: Some(BeltLevel::Green),
            consent,
            ..Member::default()
        }
    }

    fn input() -> SessionInput {
        SessionInput {
            belt: Id::v7(1, [1; 10]),
            mentor: Id::v7(2, [2; 10]),
            date: d(2026, 11, 3),
            minutes: 90.0,
            form: Form::Gemba,
            project: Some(Id::v7(3, [3; 10])),
            phase: "Medir".into(),
            topics: "MSA da balança".into(),
            tools: vec!["msa".into(), "fmea".into()],
        }
    }

    #[test]
    fn sessions_need_consent_and_commitments_become_cards() {
        let mut t = T::new();
        assert_eq!(
            new_session(&belt(false), t.id(), &input()),
            Err(CoachingError::NoConsent)
        );
        let mut nb = belt(true);
        nb.level = None;
        assert_eq!(
            new_session(&nb, t.id(), &input()),
            Err(CoachingError::NotABelt)
        );
        let s = t.id();
        t.commit(new_session(&belt(true), s, &input()).unwrap());
        t.set(Kind::CoachingSession, s, &rating_field("msa"), 2);
        let s2 = t.id();
        let mut i2 = input();
        i2.date = d(2026, 12, 1);
        t.commit(new_session(&belt(true), s2, &i2).unwrap());
        t.set(Kind::CoachingSession, s2, &rating_field("msa"), 4);
        let ss = sessions(&t.s);
        assert_eq!(ss.len(), 2);
        assert_eq!(ss[0].tools, vec!["msa", "fmea"]);
        assert_eq!(ss[0].form, Form::Gemba);
        assert!(add_commitment(s, t.id(), " ", None, Target::Personal).is_err());
        let (c1, c2) = (t.id(), t.id());
        t.commit(
            add_commitment(
                s,
                c1,
                "Refazer o estudo R&R",
                Some(d(2026, 11, 20)),
                Target::Project,
            )
            .unwrap(),
        );
        t.commit(add_commitment(s, c2, "Ler o capítulo de MSA", None, Target::Personal).unwrap());
        let c = commitments(&t.s).into_iter().find(|c| c.id == c1).unwrap();
        // The card goes to the project journal; the link to the log.
        let project_state = State::new();
        let card = t.id();
        let (card_edits, link) =
            commitment_card(&project_state, Id::v7(3, [3; 10]), &c, nb.id, card).unwrap();
        assert!(
            card_edits
                .iter()
                .any(|e| e.field == crate::domain::fields::action::DUE)
        );
        t.commit(vec![link]);
        let c = commitments(&t.s).into_iter().find(|c| c.id == c1).unwrap();
        assert_eq!(
            commitment_card(&project_state, Id::NIL, &c, nb.id, card),
            Err(CoachingError::AlreadyCard)
        );
        t.set(Kind::Commitment, c2, fields::DONE, true);
        let pr = progress(&t.s, |k| k == card);
        assert_eq!(pr.sessions, 2);
        assert!((pr.hours - 3.0).abs() < 1e-12);
        assert_eq!((pr.commitments, pr.done), (2, 2));
        assert_eq!(pr.ratings["msa"], (2, 4));
        assert_eq!(progress(&t.s, |_| false).done, 1);
        assert!(can_see(nb.id, nb.id, false));
        assert!(can_see(Id::v7(9, [9; 10]), nb.id, true));
        assert!(!can_see(Id::v7(9, [9; 10]), nb.id, false));
    }

    #[test]
    fn competence_levels_and_evidence() {
        let mut t = T::new();
        let p = t.program;
        let (ana, bia, mbb, other) = (t.id(), t.id(), t.id(), t.id());
        let mut pr = project(t.id(), None, ProjectStatus::Running);
        pr.belts = vec![ana];
        let (d1, d2) = (t.id(), t.id());
        pr.tools = vec![
            ToolUse {
                tool: Tool::Fmea,
                deliverable: d1,
                phase_index: 2,
                approved: true,
            },
            ToolUse {
                tool: Tool::Sipoc,
                deliverable: d2,
                phase_index: 0,
                approved: false,
            },
        ];
        let mut tr = project(t.id(), None, ProjectStatus::Running);
        tr.training = true;
        tr.belts = vec![bia];
        tr.tools = vec![ToolUse {
            tool: Tool::Pareto,
            deliverable: t.id(),
            phase_index: 0,
            approved: true,
        }];
        let today = d(2026, 11, 3);
        for (who, tool, level, by) in [
            (ana, "msa", Level::Studied, mbb),
            (ana, "msa", Level::Practiced, mbb),
            (ana, "fmea", Level::Demonstrated, mbb),
            (bia, "pareto", Level::Demonstrated, other),
            (bia, "pareto", Level::Applied, mbb),
            (bia, "cep", Level::Studied, other),
        ] {
            let id = t.id();
            t.commit(record_level(
                p,
                id,
                who,
                tool,
                level,
                Source::Mbb,
                "revisão",
                by,
                today,
            ));
        }
        let m = competence_map(&entries(&t.s), &[pr.clone(), tr], |x| x == mbb);
        assert_eq!(m.level(ana, "fmea"), Some(Level::Demonstrated));
        assert_eq!(m.cells[&(ana, "fmea".to_string())].evidence.len(), 2);
        assert!(
            m.cells[&(ana, "fmea".to_string())]
                .evidence
                .contains(&Evidence::Deliverable {
                    project: pr.id,
                    deliverable: d1
                })
        );
        assert_eq!(m.level(ana, "sipoc"), None, "deliverable not approved");
        assert_eq!(m.level(ana, "msa"), Some(Level::Practiced));
        assert_eq!(
            m.level(bia, "pareto"),
            None,
            "training project, entry not by the MBB, applied by hand"
        );
        assert_eq!(m.level(bia, "cep"), Some(Level::Studied));
        assert_eq!(m.applied(ana), vec!["fmea"]);
        assert!(all_tools().contains(&"fmea".to_string()));
        assert!(all_tools().contains(&"msa".to_string()));
        assert_eq!(Level::ALL.iter().max(), Some(&Level::Demonstrated));
    }

    #[test]
    fn timeline_in_date_order() {
        let b = Member {
            id: Id::v7(1, [1; 10]),
            certified_on: Some(d(2026, 12, 20)),
            ..belt(true)
        };
        let mut pr = project(Id::v7(5, [5; 10]), None, ProjectStatus::Running);
        pr.belts = vec![b.id];
        pr.tollgates = vec![crate::programa::TollgateRecord {
            id: Id::v7(6, [6; 10]),
            phase: Id::v7(7, [7; 10]),
            phase_name: "Definir".into(),
            phase_index: 0,
            status: TollgateStatus::Approved,
            round: 1,
            planned: None,
            submitted_on: None,
            approved_on: Some(d(2026, 9, 1)),
            verified: vec![],
        }];
        let s = Session {
            id: Id::v7(8, [8; 10]),
            belt: Some(b.id),
            mentor: None,
            date: Some(d(2026, 10, 1)),
            minutes: 60.0,
            form: Form::Individual,
            project: None,
            phase: String::new(),
            topics: "Pareto".into(),
            tools: vec![],
            difficulties: String::new(),
            ratings: BTreeMap::new(),
        };
        let ev = timeline(&b, &[s], &[pr], &[]);
        assert_eq!(ev.len(), 3);
        assert!(matches!(ev[0], Event::TollgateApproved { .. }));
        assert!(matches!(ev[1], Event::Session { .. }));
        assert!(matches!(ev[2], Event::Certified { .. }));
    }
}
