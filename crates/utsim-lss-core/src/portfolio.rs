//! The portfolio (`docs/plano_lean_seis_sigma.md`, 1.1): the bank of ideas with its states,
//! the screening questions that suggest the approach (sections 1.1 and 1.5), the
//! prioritisation (impact × effort matrix, weighted scoring, strategic alignment) and the
//! selection that opens a project.
//!
//! Structure (section 8.1, decision MB1 adopted provisionally): `Programa → UnidadeNegocio →
//! Carteira → Projeto`. A portfolio is a folder with its own journal (the same mechanism as
//! a project) holding a `carteira` entity, its ideas, criteria and objectives; it points to
//! its business unit, which points to its program. The program screens and the program
//! folder are stage L6; here the unit and the program are entities of the portfolio's
//! journal, ready to move up.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{Approach, ProjectStatus, enum_value, fields as dfields};
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};

pub mod fields {
    /// Portfolio, unit and program.
    pub const UNIT: &str = "unidade";
    pub const PROGRAM: &str = "programa";
    pub const NAME: &str = "nome";
    /// Weekly projects a green belt may hold at once (capacity warning).
    pub const BELT_LIMIT: &str = "limite_projetos_belt";
    // Idea.
    pub const PORTFOLIO: &str = "carteira";
    pub const TITLE: &str = "titulo";
    pub const PROCESS: &str = "processo";
    pub const PROBLEM: &str = "problema";
    pub const DATA: &str = "dados_disponiveis";
    pub const ROUGH_GAIN: &str = "ganho_grosseiro";
    pub const STATUS: &str = "estado";
    pub const IMPACT: &str = "impacto";
    pub const EFFORT: &str = "esforco";
    pub const APPROACH: &str = "abordagem";
    pub const BELT: &str = "belt";
    /// The project opened from the idea and its folder name (next to the portfolio).
    pub const PROJECT: &str = "projeto";
    pub const FOLDER: &str = "pasta";
    pub const REASON: &str = "motivo";
    /// Prefixes: `nota_<criterion>`, `alinhamento_<objective>`, `resposta_<question>`.
    pub const SCORE_PREFIX: &str = "nota_";
    pub const ALIGN_PREFIX: &str = "alinhamento_";
    pub const ANSWER_PREFIX: &str = "resposta_";
    // Criterion and objective.
    pub const WEIGHT: &str = "peso";
    pub const SCALE: &str = "escala";
    /// Lower is better (duration, risk).
    pub const INVERSE: &str = "inverso";
    /// The criterion takes its value from the strategic alignment.
    pub const FROM_ALIGNMENT: &str = "do_alinhamento";
    pub const ORDER: &str = "ordem";
}

/// Field key from an id: hex without dashes (field names are `[a-z0-9_]`).
pub fn id_key(id: Id) -> String {
    id.to_string().replace('-', "")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Portfolio {
    pub id: Id,
    pub name: String,
    pub unit: Option<Id>,
    pub program: Option<Id>,
    pub belt_limit: u32,
}

/// Edits that create a portfolio with its business unit and program (one each, by name).
pub fn create(portfolio: Id, name: &str, unit: (Id, &str), program: (Id, &str)) -> Vec<Edit> {
    let p = EntityRef::new(Kind::Portfolio, portfolio);
    let u = EntityRef::new(Kind::BusinessUnit, unit.0);
    let g = EntityRef::new(Kind::Program, program.0);
    vec![
        Edit::new(g, fields::NAME, program.1.trim()),
        Edit::new(u, fields::NAME, unit.1.trim()),
        Edit::new(u, fields::PROGRAM, program.0.to_string()),
        Edit::new(p, dfields::portfolio::NAME, name.trim()),
        Edit::new(p, fields::UNIT, unit.0.to_string()),
        Edit::new(p, fields::PROGRAM, program.0.to_string()),
        Edit::new(p, fields::BELT_LIMIT, 2),
    ]
}

pub fn read(state: &State, id: Id) -> Option<Portfolio> {
    let e = state.entity(id).filter(|e| e.kind == Kind::Portfolio)?;
    Some(Portfolio {
        id,
        name: e.text(dfields::portfolio::NAME),
        unit: e.id(fields::UNIT),
        program: e.id(fields::PROGRAM),
        belt_limit: e
            .integer(fields::BELT_LIMIT)
            .filter(|n| *n > 0)
            .map_or(2, |n| n.min(50) as u32),
    })
}

/// Name of an entity (`nome`), empty when unknown.
pub fn name_of(state: &State, id: Option<Id>) -> String {
    id.and_then(|i| state.entity(i))
        .map(|e| e.text(fields::NAME))
        .unwrap_or_default()
}

/// The screening questions (sections 1.1 and 1.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Question {
    /// Is it a new process or product, or can the current one not reach the goal?
    NewDesign,
    /// Are the cause and the solution already known?
    CauseKnown,
    /// Is the risk of the solution low?
    LowRisk,
    /// Is the problem local, solvable by the process team in a few days?
    LocalQuick,
    /// Does one page suffice to conduct and communicate it?
    OnePage,
    /// Is it a short, repetitive improvement cycle?
    ShortCycle,
}

impl Question {
    pub const ALL: [Question; 6] = [
        Question::NewDesign,
        Question::CauseKnown,
        Question::LowRisk,
        Question::LocalQuick,
        Question::OnePage,
        Question::ShortCycle,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Question::NewDesign => "projeto_novo",
            Question::CauseKnown => "causa_conhecida",
            Question::LowRisk => "risco_baixo",
            Question::LocalQuick => "local_rapido",
            Question::OnePage => "uma_pagina",
            Question::ShortCycle => "ciclo_curto",
        }
    }

    pub fn field(self) -> String {
        format!("{}{}", fields::ANSWER_PREFIX, self.key())
    }
}

/// The approach the answers suggest, by the table of section 1.1, and the question that
/// decided it (`None`: the default, DMAIC: existing process, unknown cause).
pub fn suggest(answers: &BTreeMap<Question, bool>) -> (Approach, Option<Question>) {
    let yes = |q| answers.get(&q).copied().unwrap_or(false);
    if yes(Question::NewDesign) {
        (Approach::Dmadv, Some(Question::NewDesign))
    } else if yes(Question::CauseKnown) && yes(Question::LowRisk) {
        (Approach::DirectAction, Some(Question::CauseKnown))
    } else if yes(Question::LocalQuick) {
        (Approach::Kaizen, Some(Question::LocalQuick))
    } else if yes(Question::OnePage) {
        (Approach::A3, Some(Question::OnePage))
    } else if yes(Question::ShortCycle) {
        (Approach::Pdca, Some(Question::ShortCycle))
    } else {
        (Approach::Dmaic, None)
    }
}

/// Scale of a criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Scale {
    #[default]
    #[serde(rename = "1a5")]
    OneToFive,
    #[serde(rename = "1-3-9")]
    OneThreeNine,
}

impl Scale {
    pub fn values(self) -> &'static [u8] {
        match self {
            Scale::OneToFive => &[1, 2, 3, 4, 5],
            Scale::OneThreeNine => &[1, 3, 9],
        }
    }

    pub fn max(self) -> f64 {
        match self {
            Scale::OneToFive => 5.0,
            Scale::OneThreeNine => 9.0,
        }
    }

    /// 0 for the lowest value, 1 for the highest.
    pub fn normalize(self, v: f64) -> f64 {
        ((v - 1.0) / (self.max() - 1.0)).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Criterion {
    pub id: Id,
    pub name: String,
    pub weight: f64,
    pub scale: Scale,
    pub inverse: bool,
    pub from_alignment: bool,
    pub order: f64,
}

impl Criterion {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields as f;
        Self {
            id,
            name: e.text(f::NAME),
            weight: e.number(f::WEIGHT).unwrap_or(1.0).max(0.0),
            scale: e.typed(f::SCALE).unwrap_or_default(),
            inverse: e.boolean(f::INVERSE),
            from_alignment: e.boolean(f::FROM_ALIGNMENT),
            order: e.number(f::ORDER).unwrap_or(0.0),
        }
    }

    pub fn score_field(&self) -> String {
        format!("{}{}", fields::SCORE_PREFIX, id_key(self.id))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    pub id: Id,
    pub name: String,
    pub weight: f64,
}

impl Objective {
    pub fn align_field(&self) -> String {
        format!("{}{}", fields::ALIGN_PREFIX, id_key(self.id))
    }
}

fn of_portfolio<T>(
    state: &State,
    kind: Kind,
    portfolio: Id,
    read: impl Fn(Id, &Entity) -> T,
) -> Vec<T> {
    let key = portfolio.to_string();
    state
        .of_kind(kind)
        .filter(|(_, e)| e.get(fields::PORTFOLIO).and_then(Value::as_str) == Some(key.as_str()))
        .map(|(id, e)| read(id, e))
        .collect()
}

pub fn criteria(state: &State, portfolio: Id) -> Vec<Criterion> {
    let mut v = of_portfolio(state, Kind::Criterion, portfolio, Criterion::read);
    v.sort_by(|a, b| a.order.total_cmp(&b.order).then(a.id.cmp(&b.id)));
    v
}

pub fn objectives(state: &State, portfolio: Id) -> Vec<Objective> {
    of_portfolio(state, Kind::StrategicObjective, portfolio, |id, e| {
        Objective {
            id,
            name: e.text(fields::NAME),
            weight: e.number(fields::WEIGHT).unwrap_or(1.0).max(0.0),
        }
    })
}

/// The default criteria of section 1.1: `(name, weight, scale, inverse, from alignment)`.
pub const DEFAULT_CRITERIA: [(&str, f64, Scale, bool, bool); 7] = [
    ("Ganho financeiro", 3.0, Scale::OneToFive, false, false),
    ("Satisfação do cliente", 2.0, Scale::OneToFive, false, false),
    ("Segurança", 2.0, Scale::OneToFive, false, false),
    (
        "Alinhamento estratégico",
        2.0,
        Scale::OneToFive,
        false,
        true,
    ),
    (
        "Disponibilidade de dados",
        1.0,
        Scale::OneToFive,
        false,
        false,
    ),
    ("Duração", 1.0, Scale::OneToFive, true, false),
    ("Risco", 1.0, Scale::OneToFive, true, false),
];

/// Edits that add a criterion.
#[allow(clippy::too_many_arguments)]
pub fn add_criterion(
    portfolio: Id,
    id: Id,
    name: &str,
    weight: f64,
    scale: Scale,
    inverse: bool,
    from_alignment: bool,
    order: f64,
) -> Vec<Edit> {
    let r = EntityRef::new(Kind::Criterion, id);
    vec![
        Edit::new(r, fields::PORTFOLIO, portfolio.to_string()),
        Edit::new(r, fields::NAME, name.trim()),
        Edit::new(r, fields::WEIGHT, weight),
        Edit::new(r, fields::SCALE, enum_value(scale)),
        Edit::new(r, fields::INVERSE, inverse),
        Edit::new(r, fields::FROM_ALIGNMENT, from_alignment),
        Edit::new(r, fields::ORDER, order),
    ]
}

/// Edits that add the default criteria (when the portfolio has none).
pub fn default_criteria(state: &State, portfolio: Id, mut ids: impl FnMut() -> Id) -> Vec<Edit> {
    if !criteria(state, portfolio).is_empty() {
        return Vec::new();
    }
    DEFAULT_CRITERIA
        .iter()
        .enumerate()
        .flat_map(|(i, (n, w, s, inv, al))| {
            add_criterion(portfolio, ids(), n, *w, *s, *inv, *al, i as f64 + 1.0)
        })
        .collect()
}

pub fn add_objective(portfolio: Id, id: Id, name: &str, weight: f64) -> Vec<Edit> {
    let r = EntityRef::new(Kind::StrategicObjective, id);
    vec![
        Edit::new(r, fields::PORTFOLIO, portfolio.to_string()),
        Edit::new(r, fields::NAME, name.trim()),
        Edit::new(r, fields::WEIGHT, weight),
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub struct Idea {
    pub id: Id,
    pub title: String,
    pub process: String,
    pub problem: String,
    pub data: String,
    pub rough_gain: f64,
    pub status: ProjectStatus,
    pub impact: Option<u8>,
    pub effort: Option<u8>,
    pub approach: Option<Approach>,
    pub belt: Option<Id>,
    pub project: Option<Id>,
    pub folder: String,
    pub answers: BTreeMap<Question, bool>,
    /// Raw fields (scores and alignment are read through the criteria and objectives).
    pub entity: Entity,
}

impl Idea {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields as f;
        let scale = |field| {
            e.integer(field)
                .filter(|v| (1..=5).contains(v))
                .map(|v| v as u8)
        };
        Self {
            id,
            title: e.text(f::TITLE),
            process: e.text(f::PROCESS),
            problem: e.text(f::PROBLEM),
            data: e.text(f::DATA),
            rough_gain: e.number(f::ROUGH_GAIN).unwrap_or(0.0),
            status: e.typed(f::STATUS).unwrap_or(ProjectStatus::Idea),
            impact: scale(f::IMPACT),
            effort: scale(f::EFFORT),
            approach: e.typed(f::APPROACH),
            belt: e.id(f::BELT),
            project: e.id(f::PROJECT),
            folder: e.text(f::FOLDER),
            answers: Question::ALL
                .iter()
                .filter_map(|q| e.get(&q.field()).and_then(Value::as_bool).map(|b| (*q, b)))
                .collect(),
            entity: e.clone(),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Idea, id)
    }

    pub fn score(&self, c: &Criterion) -> Option<f64> {
        self.entity.number(&c.score_field())
    }

    pub fn alignment(&self, o: &Objective) -> Option<f64> {
        self.entity.number(&o.align_field())
    }
}

pub fn ideas(state: &State, portfolio: Id) -> Vec<Idea> {
    of_portfolio(state, Kind::Idea, portfolio, Idea::read)
}

/// Edits that register an idea (anyone may).
pub fn new_idea(portfolio: Id, id: Id, title: &str) -> Vec<Edit> {
    let r = Idea::entity_ref(id);
    vec![
        Edit::new(r, fields::PORTFOLIO, portfolio.to_string()),
        Edit::new(r, fields::TITLE, title.trim()),
        Edit::new(r, fields::STATUS, enum_value(ProjectStatus::Idea)),
    ]
}

/// Quadrant of the impact × effort matrix (impact and effort ≥ 3 count as high).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Quadrant {
    /// High impact, low effort.
    QuickWin,
    /// High impact, high effort.
    Strategic,
    /// Low impact, low effort.
    Minor,
    /// Low impact, high effort.
    Avoid,
}

pub fn quadrant(impact: u8, effort: u8) -> Quadrant {
    match (impact >= 3, effort >= 3) {
        (true, false) => Quadrant::QuickWin,
        (true, true) => Quadrant::Strategic,
        (false, false) => Quadrant::Minor,
        (false, true) => Quadrant::Avoid,
    }
}

/// Strategic alignment of an idea, 0 to 1: `Σ weight × intensity / (Σ weight × 9)`, with
/// intensities 0, 1, 3 or 9 (missing: 0). `None` without objectives.
pub fn alignment(idea: &Idea, objectives: &[Objective]) -> Option<f64> {
    let total: f64 = objectives.iter().map(|o| o.weight).sum();
    if total <= 0.0 {
        return None;
    }
    let s: f64 = objectives
        .iter()
        .map(|o| o.weight * idea.alignment(o).unwrap_or(0.0).clamp(0.0, 9.0))
        .sum();
    Some(s / (total * 9.0))
}

/// The weighted score of an idea.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub idea: Id,
    /// 0 to 100 over the criteria with a value.
    pub total: f64,
    /// Fraction of the weight with a value (1: every criterion scored).
    pub completeness: f64,
}

/// `100 × Σ wᵢ·xᵢ / Σ wᵢ` over the scored criteria, with `xᵢ` the score normalised to 0..1
/// (reversed for "lower is better") or the alignment for the alignment criterion.
pub fn score(idea: &Idea, criteria: &[Criterion], objectives: &[Objective]) -> Score {
    let all: f64 = criteria.iter().map(|c| c.weight).sum();
    let (mut num, mut den) = (0.0, 0.0);
    for c in criteria {
        let x = if c.from_alignment {
            alignment(idea, objectives)
        } else {
            idea.score(c).map(|v| {
                let n = c.scale.normalize(v);
                if c.inverse { 1.0 - n } else { n }
            })
        };
        if let Some(x) = x {
            num += c.weight * x;
            den += c.weight;
        }
    }
    Score {
        idea: idea.id,
        total: if den > 0.0 {
            (100.0 * num / den).clamp(0.0, 100.0)
        } else {
            0.0
        },
        completeness: if all > 0.0 { den / all } else { 0.0 },
    }
}

/// Ideas still to decide (idea, screening, approved), best score first.
pub fn ranking(state: &State, portfolio: Id) -> Vec<(Idea, Score)> {
    let cs = criteria(state, portfolio);
    let os = objectives(state, portfolio);
    let mut v: Vec<(Idea, Score)> = ideas(state, portfolio)
        .into_iter()
        .filter(|i| {
            matches!(
                i.status,
                ProjectStatus::Idea | ProjectStatus::Screening | ProjectStatus::Approved
            )
        })
        .map(|i| {
            let s = score(&i, &cs, &os);
            (i, s)
        })
        .collect();
    v.sort_by(|a, b| b.1.total.total_cmp(&a.1.total).then(a.0.id.cmp(&b.0.id)));
    v
}

/// Ideas per state (the portfolio board).
pub fn by_status(state: &State, portfolio: Id) -> BTreeMap<u8, Vec<Idea>> {
    let mut m: BTreeMap<u8, Vec<Idea>> = BTreeMap::new();
    for i in ideas(state, portfolio) {
        m.entry(status_order(i.status)).or_default().push(i);
    }
    m
}

/// Position of a state on the portfolio board.
pub fn status_order(s: ProjectStatus) -> u8 {
    match s {
        ProjectStatus::Idea => 0,
        ProjectStatus::Screening => 1,
        ProjectStatus::Approved => 2,
        ProjectStatus::Running => 3,
        ProjectStatus::Suspended => 4,
        ProjectStatus::Closed => 5,
        ProjectStatus::Cancelled => 6,
        ProjectStatus::Archived => 7,
    }
}

/// The states of the board, in order.
pub const BOARD: [ProjectStatus; 8] = [
    ProjectStatus::Idea,
    ProjectStatus::Screening,
    ProjectStatus::Approved,
    ProjectStatus::Running,
    ProjectStatus::Suspended,
    ProjectStatus::Closed,
    ProjectStatus::Cancelled,
    ProjectStatus::Archived,
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortfolioError {
    #[error("ideia não encontrada")]
    NotFound,
    #[error("passagem de estado não permitida para a ideia")]
    BadTransition,
    #[error("escolha a abordagem antes de aprovar")]
    NoApproach,
}

/// Allowed moves: idea → screening → approved → running → closed; cancelled or archived
/// from any open state; back one step before running.
pub fn can_move(from: ProjectStatus, to: ProjectStatus) -> bool {
    use ProjectStatus::*;
    matches!(
        (from, to),
        (Idea, Screening)
            | (Screening, Approved)
            | (Screening, Idea)
            | (Approved, Screening)
            | (Approved, Running)
            | (Running, Closed)
            | (Running, Suspended)
            | (Suspended, Running)
            | (
                Idea | Screening | Approved | Running | Suspended,
                Cancelled | Archived
            )
            | (Cancelled | Archived, Idea)
    )
}

/// Edits that move an idea. Approving needs the approach chosen.
pub fn move_idea(
    state: &State,
    idea: Id,
    to: ProjectStatus,
    reason: &str,
) -> Result<Vec<Edit>, PortfolioError> {
    let e = state
        .entity(idea)
        .filter(|e| e.kind == Kind::Idea && !e.is_deleted())
        .ok_or(PortfolioError::NotFound)?;
    let i = Idea::read(idea, e);
    if !can_move(i.status, to) {
        return Err(PortfolioError::BadTransition);
    }
    if to == ProjectStatus::Approved && i.approach.is_none() {
        return Err(PortfolioError::NoApproach);
    }
    let r = Idea::entity_ref(idea);
    let mut edits = vec![Edit::new(r, fields::STATUS, enum_value(to))];
    if !reason.trim().is_empty() {
        edits.push(Edit::new(r, fields::REASON, reason.trim()));
    }
    Ok(edits)
}

/// Edits that record the project opened from an approved idea: the idea becomes running,
/// with the project id and its folder name.
pub fn opened(idea: Id, project: Id, folder: &str) -> Vec<Edit> {
    let r = Idea::entity_ref(idea);
    vec![
        Edit::new(r, fields::STATUS, enum_value(ProjectStatus::Running)),
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::FOLDER, folder),
    ]
}

/// Projects per belt among the running ideas, and who is over the portfolio's limit.
pub fn belt_load(state: &State, portfolio: Id) -> Vec<(Id, usize, bool)> {
    let limit = read(state, portfolio).map_or(2, |p| p.belt_limit) as usize;
    let mut m: BTreeMap<Id, usize> = BTreeMap::new();
    for i in ideas(state, portfolio) {
        if matches!(i.status, ProjectStatus::Running | ProjectStatus::Approved) {
            if let Some(b) = i.belt {
                *m.entry(b).or_default() += 1;
            }
        }
    }
    m.into_iter().map(|(b, n)| (b, n, n > limit)).collect()
}

/// What the portfolio shows of each project folder next to it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSummary {
    pub id: Id,
    pub title: String,
    pub approach: Approach,
    pub status: ProjectStatus,
    pub phase: String,
    pub process: String,
    pub belts: Vec<Id>,
    pub savings: crate::savings::Totals,
}

/// The summary of a project from its state.
pub fn summary(state: &State, project: Id) -> Option<ProjectSummary> {
    let e = state.entity(project).filter(|e| e.kind == Kind::Project)?;
    let p = crate::domain::Project::read(project, e);
    let phase = p
        .current_phase
        .and_then(|id| state.entity(id))
        .map(|e| e.text(dfields::phase::NAME))
        .unwrap_or_default();
    let mut belts = crate::roles::holders(state, project, crate::domain::RoleKind::GreenBelt);
    belts.extend(crate::roles::holders(
        state,
        project,
        crate::domain::RoleKind::BlackBelt,
    ));
    Some(ProjectSummary {
        id: project,
        title: p.title,
        approach: p.approach,
        status: p.status,
        phase,
        process: p.process,
        belts,
        savings: crate::savings::totals(state, project),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    struct T {
        s: State,
        n: u64,
        me: Id,
        c: Id,
    }

    impl T {
        fn new() -> T {
            let mut t = T {
                s: State::new(),
                n: 0,
                me: Id::NIL,
                c: Id::NIL,
            };
            t.me = t.id();
            t.c = t.id();
            let (u, g) = (t.id(), t.id());
            t.commit(create(
                t.c,
                "Carteira Estamparia",
                (u, "Planta 1"),
                (g, "Programa LSS"),
            ));
            t
        }
        fn id(&mut self) -> Id {
            self.n += 1;
            Id::v7(self.n, [self.n as u8; 10])
        }
        fn commit(&mut self, e: Vec<Edit>) {
            let mut n = self.n;
            let ops = self.s.prepare(self.me, 0, e, || {
                n += 1;
                Id::v7(n, [n as u8; 10])
            });
            self.n = n;
            for op in ops {
                self.s.apply(&op).unwrap();
            }
        }
        fn set(&mut self, idea: Id, f: &str, v: impl Into<Value>) {
            self.commit(vec![Edit::new(Idea::entity_ref(idea), f, v)]);
        }
    }

    #[test]
    fn structure_unit_and_program() {
        let t = T::new();
        let p = read(&t.s, t.c).unwrap();
        assert_eq!(p.name, "Carteira Estamparia");
        assert_eq!(name_of(&t.s, p.unit), "Planta 1");
        assert_eq!(name_of(&t.s, p.program), "Programa LSS");
        let unit = t.s.entity(p.unit.unwrap()).unwrap();
        assert_eq!(unit.id(fields::PROGRAM), p.program);
        assert_eq!(p.belt_limit, 2);
    }

    #[test]
    fn suggested_approach_follows_the_table() {
        let a = |pairs: &[(Question, bool)]| suggest(&pairs.iter().copied().collect()).0;
        use Question::*;
        assert_eq!(a(&[]), Approach::Dmaic);
        assert_eq!(
            a(&[(NewDesign, true), (CauseKnown, true), (LowRisk, true)]),
            Approach::Dmadv
        );
        assert_eq!(
            a(&[(CauseKnown, true), (LowRisk, true)]),
            Approach::DirectAction
        );
        assert_eq!(
            a(&[(CauseKnown, true), (LowRisk, false), (LocalQuick, true)]),
            Approach::Kaizen
        );
        assert_eq!(a(&[(OnePage, true), (ShortCycle, true)]), Approach::A3);
        assert_eq!(a(&[(ShortCycle, true)]), Approach::Pdca);
        assert_eq!(suggest(&BTreeMap::new()).1, None);
    }

    #[test]
    fn matrix_scoring_alignment_and_ranking() {
        let mut t = T::new();
        let e = default_criteria(&t.s, t.c, || Id::NIL);
        assert_eq!(e.len(), 7 * 7);
        let mut k = 500u64;
        let e = default_criteria(&t.s, t.c, || {
            k += 1;
            Id::v7(k, [3; 10])
        });
        t.commit(e);
        assert!(default_criteria(&t.s, t.c, || Id::NIL).is_empty());
        let (o1, o2) = (t.id(), t.id());
        t.commit(add_objective(t.c, o1, "Reduzir custo", 2.0));
        t.commit(add_objective(t.c, o2, "Segurança zero acidente", 1.0));
        let (a, b) = (t.id(), t.id());
        t.commit(new_idea(t.c, a, "Refugo na prensa 3"));
        t.commit(new_idea(t.c, b, "Etiquetas trocadas"));
        let cs = criteria(&t.s, t.c);
        let os = objectives(&t.s, t.c);
        // Idea a: every criterion at the top, full alignment to both objectives.
        for c in cs.iter().filter(|c| !c.from_alignment) {
            let v = if c.inverse { 1 } else { 5 };
            t.set(a, &c.score_field(), v);
        }
        for o in &os {
            t.set(a, &o.align_field(), 9);
        }
        // Idea b: only the financial gain, at 3.
        t.set(b, &cs[0].score_field(), 3);
        t.set(b, &os[1].align_field(), 3);
        let ia = Idea::read(a, t.s.entity(a).unwrap());
        let ib = Idea::read(b, t.s.entity(b).unwrap());
        let sa = score(&ia, &cs, &os);
        assert!((sa.total - 100.0).abs() < 1e-9);
        assert!((sa.completeness - 1.0).abs() < 1e-9);
        assert!((alignment(&ib, &os).unwrap() - 3.0 / 27.0).abs() < 1e-12);
        let sb = score(&ib, &cs, &os);
        // Financial 0.5 × 3 + alignment (1/9) × 2 over weights 5.
        assert!((sb.total - 100.0 * (1.5 + 2.0 / 9.0) / 5.0).abs() < 1e-9);
        assert!((sb.completeness - 5.0 / 12.0).abs() < 1e-9);
        let r = ranking(&t.s, t.c);
        assert_eq!(r[0].0.id, a);
        assert_eq!(quadrant(5, 1), Quadrant::QuickWin);
        assert_eq!(quadrant(3, 3), Quadrant::Strategic);
        assert_eq!(quadrant(2, 2), Quadrant::Minor);
        assert_eq!(quadrant(1, 5), Quadrant::Avoid);
    }

    #[test]
    fn idea_flow_selection_and_belt_load() {
        let mut t = T::new();
        let a = t.id();
        t.commit(new_idea(t.c, a, "Setup longo"));
        assert_eq!(
            move_idea(&t.s, a, ProjectStatus::Approved, ""),
            Err(PortfolioError::BadTransition)
        );
        let e = move_idea(&t.s, a, ProjectStatus::Screening, "").unwrap();
        t.commit(e);
        assert_eq!(
            move_idea(&t.s, a, ProjectStatus::Approved, ""),
            Err(PortfolioError::NoApproach)
        );
        t.set(a, &Question::LocalQuick.field(), true);
        let i = Idea::read(a, t.s.entity(a).unwrap());
        assert_eq!(suggest(&i.answers).0, Approach::Kaizen);
        t.set(a, fields::APPROACH, enum_value(Approach::Kaizen));
        t.set(a, fields::BELT, t.me.to_string());
        let e = move_idea(&t.s, a, ProjectStatus::Approved, "comitê de novembro").unwrap();
        t.commit(e);
        let pid = t.id();
        t.commit(opened(a, pid, "Setup longo.lss"));
        let i = Idea::read(a, t.s.entity(a).unwrap());
        assert_eq!(i.status, ProjectStatus::Running);
        assert_eq!(i.project, Some(pid));
        assert_eq!(i.entity.text(fields::REASON), "comitê de novembro");
        assert!(
            ranking(&t.s, t.c).is_empty(),
            "running ideas leave the ranking"
        );
        for title in ["b", "c"] {
            let x = t.id();
            t.commit(new_idea(t.c, x, title));
            t.set(x, fields::STATUS, enum_value(ProjectStatus::Running));
            t.set(x, fields::BELT, t.me.to_string());
        }
        assert_eq!(belt_load(&t.s, t.c), vec![(t.me, 3, true)]);
        assert_eq!(
            by_status(&t.s, t.c)[&status_order(ProjectStatus::Running)].len(),
            3
        );
    }

    proptest! {
        #[test]
        fn score_is_bounded_and_monotone(v in 1u8..=5, w in 0.1f64..10.0) {
            let mut t = T::new();
            let c = t.id();
            t.commit(add_criterion(t.c, c, "x", w, Scale::OneToFive, false, false, 1.0));
            let a = t.id();
            t.commit(new_idea(t.c, a, "a"));
            let cs = criteria(&t.s, t.c);
            t.set(a, &cs[0].score_field(), v);
            let s1 = score(&Idea::read(a, t.s.entity(a).unwrap()), &cs, &[]).total;
            prop_assert!((0.0..=100.0).contains(&s1));
            if v < 5 {
                t.set(a, &cs[0].score_field(), v + 1);
                let s2 = score(&Idea::read(a, t.s.entity(a).unwrap()), &cs, &[]).total;
                prop_assert!(s2 > s1);
            }
        }
    }
}
