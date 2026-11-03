//! Typed views of the entities (`docs/plano_lean_seis_sigma.md`, 4.3).
//!
//! The journal stores loose fields; these structs read them with defaults, so a field
//! that is missing, of the wrong type or written by a newer version never breaks the
//! reading. Each kind has a module of field names (the JSON keys, in Portuguese) used both
//! to read and to build [`Edit`]s.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};

/// Field names of each kind.
pub mod fields {
    pub mod portfolio {
        pub const NAME: &str = "nome";
        pub const DESCRIPTION: &str = "descricao";
    }
    pub mod project {
        pub const TITLE: &str = "titulo";
        pub const APPROACH: &str = "abordagem";
        pub const STATUS: &str = "estado";
        pub const CURRENT_PHASE: &str = "fase_atual";
        pub const PORTFOLIO: &str = "carteira";
        pub const PROCESS: &str = "processo";
        pub const AREA: &str = "area";
        pub const PROBLEM: &str = "problema";
        pub const GOAL: &str = "meta";
        pub const BASELINE: &str = "linha_de_base";
        pub const DEADLINE: &str = "prazo";
        pub const SCOPE_IN: &str = "escopo_dentro";
        pub const SCOPE_OUT: &str = "escopo_fora";
        pub const BUSINESS_CASE: &str = "caso_de_negocio";
    }
    pub mod phase {
        pub const PROJECT: &str = "projeto";
        pub const NAME: &str = "nome";
        pub const ORDER: &str = "ordem";
        pub const STATUS: &str = "estado";
    }
    pub mod deliverable {
        pub const PHASE: &str = "fase";
        pub const NAME: &str = "nome";
        pub const TOOL: &str = "ferramenta";
        pub const STATUS: &str = "estado";
        pub const CONTENT: &str = "conteudo";
    }
    pub mod tollgate {
        pub const PHASE: &str = "fase";
        pub const STATUS: &str = "estado";
        pub const FROZEN_HASH: &str = "hash_congelado";
    }
    pub mod checklist_item {
        pub const TOLLGATE: &str = "tollgate";
        pub const TEXT: &str = "texto";
        pub const REQUIRED: &str = "obrigatorio";
        pub const DONE: &str = "feito";
        pub const ORDER: &str = "ordem";
    }
    pub mod person {
        pub const NAME: &str = "nome";
        pub const EMAIL: &str = "email";
    }
    pub mod role {
        pub const PROJECT: &str = "projeto";
        pub const PERSON: &str = "pessoa";
        pub const ROLE: &str = "papel";
    }
    pub mod action {
        pub const PROJECT: &str = "projeto";
        /// 5W2H: o quê.
        pub const TITLE: &str = "titulo";
        pub const WHY: &str = "porque";
        pub const WHERE: &str = "onde";
        /// Due date, `AAAA-MM-DD`.
        pub const DUE: &str = "prazo";
        /// Person id.
        pub const WHO: &str = "responsavel";
        pub const HOW: &str = "como";
        pub const HOW_MUCH: &str = "quanto";
        pub const ORIGIN: &str = "origem";
        pub const COLUMN: &str = "coluna";
        pub const ORDER: &str = "ordem";
        pub const PHASE: &str = "fase";
    }
    pub mod risk {
        pub const PROJECT: &str = "projeto";
        pub const DESCRIPTION: &str = "descricao";
        pub const CAUSE: &str = "causa";
        pub const EFFECT: &str = "efeito";
        pub const PROBABILITY: &str = "probabilidade";
        pub const IMPACT: &str = "impacto";
        pub const RESPONSE: &str = "resposta";
        pub const OWNER: &str = "dono";
        pub const STATUS: &str = "estado";
    }
    pub mod meeting {
        pub const PROJECT: &str = "projeto";
        pub const TITLE: &str = "titulo";
        pub const DATE: &str = "data";
        pub const AGENDA: &str = "pauta";
        pub const NOTES: &str = "anotacoes";
        pub const DECISIONS: &str = "decisoes";
    }
    pub mod saving {
        pub const PROJECT: &str = "projeto";
        pub const DESCRIPTION: &str = "descricao";
        pub const CATEGORY: &str = "categoria";
        pub const STATUS: &str = "estado";
        pub const VALUE: &str = "valor";
        pub const RATIONALE: &str = "memoria";
    }
}

/// Approach of a project (`docs/plano_lean_seis_sigma.md`, 1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Approach {
    #[serde(rename = "acao_direta")]
    DirectAction,
    #[serde(rename = "kaizen")]
    Kaizen,
    #[serde(rename = "a3")]
    A3,
    #[serde(rename = "pdca")]
    Pdca,
    #[default]
    #[serde(rename = "dmaic")]
    Dmaic,
    #[serde(rename = "dmadv")]
    Dmadv,
}

impl Approach {
    pub const ALL: [Approach; 6] = [
        Approach::DirectAction,
        Approach::Kaizen,
        Approach::A3,
        Approach::Pdca,
        Approach::Dmaic,
        Approach::Dmadv,
    ];

    /// Default phases of the approach (the flow templates of section 4.7 will replace this).
    pub fn phases(self) -> &'static [&'static str] {
        match self {
            Approach::DirectAction => &["Executar"],
            Approach::Kaizen => &["Pré-evento", "Evento", "Acompanhamento"],
            Approach::A3 => &["A3"],
            Approach::Pdca => &["Planejar", "Fazer", "Verificar", "Agir"],
            Approach::Dmaic => &["Definir", "Medir", "Analisar", "Melhorar", "Controlar"],
            Approach::Dmadv => &["Definir", "Medir", "Analisar", "Desenhar", "Verificar"],
        }
    }
}

/// Portfolio states of a project (`ideia → triagem → aprovada → em execução → encerrada |
/// cancelada | arquivada`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProjectStatus {
    #[serde(rename = "ideia")]
    Idea,
    #[serde(rename = "triagem")]
    Screening,
    #[serde(rename = "aprovada")]
    Approved,
    #[default]
    #[serde(rename = "em_execucao")]
    Running,
    #[serde(rename = "encerrada")]
    Closed,
    #[serde(rename = "cancelada")]
    Cancelled,
    #[serde(rename = "arquivada")]
    Archived,
    /// Stopped by a tollgate decision (`suspender`); it can resume.
    #[serde(rename = "suspensa")]
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PhaseStatus {
    #[default]
    #[serde(rename = "nao_iniciada")]
    NotStarted,
    #[serde(rename = "em_andamento")]
    InProgress,
    #[serde(rename = "em_tollgate")]
    InTollgate,
    #[serde(rename = "aprovada")]
    Approved,
}

/// Columns of the action board (`docs/plano_lean_seis_sigma.md`, 1.7).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Column {
    #[default]
    #[serde(rename = "a_fazer")]
    Todo,
    #[serde(rename = "fazendo")]
    Doing,
    #[serde(rename = "bloqueado")]
    Blocked,
    #[serde(rename = "verificar")]
    Verify,
    #[serde(rename = "feito")]
    Done,
}

impl Column {
    pub const ALL: [Column; 5] = [
        Column::Todo,
        Column::Doing,
        Column::Blocked,
        Column::Verify,
        Column::Done,
    ];
}

/// Where an action came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Origin {
    #[default]
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "fmea")]
    Fmea,
    #[serde(rename = "ata")]
    Minutes,
    #[serde(rename = "tollgate")]
    Tollgate,
    #[serde(rename = "risco")]
    Risk,
    #[serde(rename = "plano_de_resposta")]
    ResponsePlan,
    #[serde(rename = "kaizen")]
    Kaizen,
}

/// Roles in a project (`docs/plano_lean_seis_sigma.md`, 1.2).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum RoleKind {
    #[serde(rename = "patrocinador")]
    Sponsor,
    #[serde(rename = "champion")]
    Champion,
    #[serde(rename = "dono_do_processo")]
    ProcessOwner,
    #[serde(rename = "master_black_belt")]
    MasterBlackBelt,
    #[serde(rename = "black_belt")]
    BlackBelt,
    #[serde(rename = "green_belt")]
    GreenBelt,
    #[serde(rename = "yellow_belt")]
    YellowBelt,
    #[default]
    #[serde(rename = "equipe")]
    Team,
    #[serde(rename = "controladoria")]
    Controller,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RiskResponse {
    #[serde(rename = "evitar")]
    Avoid,
    #[default]
    #[serde(rename = "mitigar")]
    Mitigate,
    #[serde(rename = "transferir")]
    Transfer,
    #[serde(rename = "aceitar")]
    Accept,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RiskStatus {
    #[default]
    #[serde(rename = "aberto")]
    Open,
    #[serde(rename = "fechado")]
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SavingCategory {
    #[default]
    #[serde(rename = "hard")]
    Hard,
    #[serde(rename = "soft")]
    Soft,
}

/// `estimado → previsto → realizado → validado` (`docs/plano_lean_seis_sigma.md`, 1.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum SavingStatus {
    #[default]
    #[serde(rename = "estimado")]
    Estimated,
    #[serde(rename = "previsto")]
    Forecast,
    #[serde(rename = "realizado")]
    Realized,
    #[serde(rename = "validado")]
    Validated,
}

/// Serde value of a unit enum (its file key).
pub fn enum_value<T: Serialize>(v: T) -> Value {
    serde_json::to_value(v).expect("unit enum")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub id: Id,
    pub title: String,
    pub approach: Approach,
    pub status: ProjectStatus,
    pub current_phase: Option<Id>,
    pub process: String,
    pub area: String,
    pub problem: String,
    pub goal: String,
    pub baseline: String,
    pub deadline: Option<Date>,
    pub scope_in: String,
    pub scope_out: String,
    pub business_case: String,
}

impl Project {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields::project as f;
        Self {
            id,
            title: e.text(f::TITLE),
            approach: e.typed(f::APPROACH).unwrap_or_default(),
            status: e.typed(f::STATUS).unwrap_or_default(),
            current_phase: e.id(f::CURRENT_PHASE),
            process: e.text(f::PROCESS),
            area: e.text(f::AREA),
            problem: e.text(f::PROBLEM),
            goal: e.text(f::GOAL),
            baseline: e.text(f::BASELINE),
            deadline: e.opt_text(f::DEADLINE).and_then(|s| s.parse().ok()),
            scope_in: e.text(f::SCOPE_IN),
            scope_out: e.text(f::SCOPE_OUT),
            business_case: e.text(f::BUSINESS_CASE),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Project, id)
    }

    /// The edits that create a project with its phases. `ids` supplies one id per phase.
    pub fn create(
        id: Id,
        title: &str,
        approach: Approach,
        mut ids: impl FnMut() -> Id,
    ) -> Vec<Edit> {
        use fields::project as f;
        let p = Self::entity_ref(id);
        let mut edits = vec![
            Edit::new(p, f::TITLE, title),
            Edit::new(p, f::APPROACH, enum_value(approach)),
            Edit::new(p, f::STATUS, enum_value(ProjectStatus::Running)),
        ];
        let mut first = None;
        for (i, name) in approach.phases().iter().enumerate() {
            let phase = EntityRef::new(Kind::Phase, ids());
            first.get_or_insert(phase.id);
            edits.push(Edit::new(phase, fields::phase::PROJECT, id.to_string()));
            edits.push(Edit::new(phase, fields::phase::NAME, *name));
            edits.push(Edit::new(phase, fields::phase::ORDER, i as u64));
            let status = if i == 0 {
                PhaseStatus::InProgress
            } else {
                PhaseStatus::NotStarted
            };
            edits.push(Edit::new(phase, fields::phase::STATUS, enum_value(status)));
        }
        if let Some(first) = first {
            edits.push(Edit::new(p, f::CURRENT_PHASE, first.to_string()));
        }
        edits
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    pub id: Id,
    pub project: Option<Id>,
    pub name: String,
    pub order: i64,
    pub status: PhaseStatus,
}

impl Phase {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields::phase as f;
        Self {
            id,
            project: e.id(f::PROJECT),
            name: e.text(f::NAME),
            order: e.integer(f::ORDER).unwrap_or(0),
            status: e.typed(f::STATUS).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub id: Id,
    pub name: String,
    pub email: String,
}

impl Person {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            name: e.text(fields::person::NAME),
            email: e.text(fields::person::EMAIL),
        }
    }

    /// Edits that register (or update) a person in a project.
    pub fn register(state: &State, id: Id, name: &str, email: &str) -> Vec<Edit> {
        let r = EntityRef::new(Kind::Person, id);
        let current = state.entity(id);
        let mut edits = Vec::new();
        for (field, value) in [(fields::person::NAME, name), (fields::person::EMAIL, email)] {
            if current.map(|e| e.text(field)).as_deref() != Some(value) {
                edits.push(Edit::new(r, field, value));
            }
        }
        edits
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Role {
    pub id: Id,
    pub project: Option<Id>,
    pub person: Option<Id>,
    pub role: RoleKind,
}

impl Role {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields::role as f;
        Self {
            id,
            project: e.id(f::PROJECT),
            person: e.id(f::PERSON),
            role: e.typed(f::ROLE).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub id: Id,
    pub project: Option<Id>,
    pub title: String,
    pub why: String,
    pub place: String,
    pub due: Option<Date>,
    pub who: Option<Id>,
    pub how: String,
    pub how_much: String,
    pub origin: Origin,
    pub column: Column,
    pub order: f64,
    pub phase: Option<Id>,
}

impl Action {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields::action as f;
        Self {
            id,
            project: e.id(f::PROJECT),
            title: e.text(f::TITLE),
            why: e.text(f::WHY),
            place: e.text(f::WHERE),
            due: e.opt_text(f::DUE).and_then(|s| s.parse().ok()),
            who: e.id(f::WHO),
            how: e.text(f::HOW),
            how_much: e.text(f::HOW_MUCH),
            origin: e.typed(f::ORIGIN).unwrap_or_default(),
            column: e.typed(f::COLUMN).unwrap_or_default(),
            order: e.number(f::ORDER).unwrap_or(0.0),
            phase: e.id(f::PHASE),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Action, id)
    }

    pub fn is_overdue(&self, today: Date) -> bool {
        self.column != Column::Done && self.due.is_some_and(|d| d < today)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Risk {
    pub id: Id,
    pub project: Option<Id>,
    pub description: String,
    pub cause: String,
    pub effect: String,
    /// 1 to 5.
    pub probability: u8,
    /// 1 to 5.
    pub impact: u8,
    pub response: RiskResponse,
    pub owner: Option<Id>,
    pub status: RiskStatus,
}

impl Risk {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields::risk as f;
        let scale = |field| e.integer(field).map_or(1, |v| v.clamp(1, 5) as u8);
        Self {
            id,
            project: e.id(f::PROJECT),
            description: e.text(f::DESCRIPTION),
            cause: e.text(f::CAUSE),
            effect: e.text(f::EFFECT),
            probability: scale(f::PROBABILITY),
            impact: scale(f::IMPACT),
            response: e.typed(f::RESPONSE).unwrap_or_default(),
            owner: e.id(f::OWNER),
            status: e.typed(f::STATUS).unwrap_or_default(),
        }
    }

    /// Level in the 5 × 5 matrix: probability × impact (1 to 25).
    pub fn level(&self) -> u8 {
        self.probability * self.impact
    }

    /// High risk: level 15 or more (the usual red zone of a 5 × 5 matrix).
    pub fn is_high(&self) -> bool {
        self.level() >= 15
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Meeting {
    pub id: Id,
    pub project: Option<Id>,
    pub title: String,
    pub date: Option<Date>,
    pub agenda: String,
    pub notes: String,
    pub decisions: String,
}

impl Meeting {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields::meeting as f;
        Self {
            id,
            project: e.id(f::PROJECT),
            title: e.text(f::TITLE),
            date: e.opt_text(f::DATE).and_then(|s| s.parse().ok()),
            agenda: e.text(f::AGENDA),
            notes: e.text(f::NOTES),
            decisions: e.text(f::DECISIONS),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Saving {
    pub id: Id,
    pub project: Option<Id>,
    pub description: String,
    pub category: SavingCategory,
    pub status: SavingStatus,
    /// Value in the company's currency.
    pub value: f64,
    pub rationale: String,
}

impl Saving {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields::saving as f;
        Self {
            id,
            project: e.id(f::PROJECT),
            description: e.text(f::DESCRIPTION),
            category: e.typed(f::CATEGORY).unwrap_or_default(),
            status: e.typed(f::STATUS).unwrap_or_default(),
            value: e.number(f::VALUE).unwrap_or(0.0),
            rationale: e.text(f::RATIONALE),
        }
    }
}

/// Entities of `kind` whose `projeto` field is `project`, read as `T`.
pub fn of_project<T>(
    state: &State,
    kind: Kind,
    project: Id,
    read: impl Fn(Id, &Entity) -> T,
) -> Vec<T> {
    let key = project.to_string();
    state
        .of_kind(kind)
        .filter(|(_, e)| e.get("projeto").and_then(Value::as_str) == Some(key.as_str()))
        .map(|(id, e)| read(id, e))
        .collect()
}

/// The projects of the state (a project folder holds one; a portfolio may hold more).
pub fn projects(state: &State) -> Vec<Project> {
    state
        .of_kind(Kind::Project)
        .map(|(id, e)| Project::read(id, e))
        .collect()
}

/// Phases of a project in order.
pub fn phases(state: &State, project: Id) -> Vec<Phase> {
    let mut v = of_project(state, Kind::Phase, project, Phase::read);
    v.sort_by_key(|p| (p.order, p.id));
    v
}

pub fn people(state: &State) -> Vec<Person> {
    state
        .of_kind(Kind::Person)
        .map(|(id, e)| Person::read(id, e))
        .collect()
}

/// Display name of a person id, or its short id when unknown.
pub fn person_name(state: &State, id: Id) -> String {
    state
        .entity(id)
        .filter(|e| e.kind == Kind::Person)
        .map(|e| e.text(fields::person::NAME))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| id.short())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen_ids() -> impl FnMut() -> Id {
        let mut n = 0u64;
        move || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        }
    }

    #[test]
    fn create_project_with_dmaic_phases() {
        let mut ids = gen_ids();
        let pid = ids();
        let me = ids();
        let edits = Project::create(pid, "Reduzir refugo da linha 2", Approach::Dmaic, &mut ids);
        let s = State::new();
        let ops = s.prepare(me, 0, edits, &mut ids);
        let s = State::replay(&ops);
        let p = &projects(&s)[0];
        assert_eq!(p.title, "Reduzir refugo da linha 2");
        assert_eq!(p.approach, Approach::Dmaic);
        let ph = phases(&s, pid);
        let names: Vec<_> = ph.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            ["Definir", "Medir", "Analisar", "Melhorar", "Controlar"]
        );
        assert_eq!(p.current_phase, Some(ph[0].id));
        assert_eq!(ph[0].status, PhaseStatus::InProgress);
        assert_eq!(ph[1].status, PhaseStatus::NotStarted);
    }

    #[test]
    fn lenient_reading() {
        let mut ids = gen_ids();
        let rid = ids();
        let r = EntityRef::new(Kind::Risk, rid);
        let s = State::new();
        let ops = s.prepare(
            ids(),
            0,
            [
                Edit::new(r, fields::risk::PROBABILITY, 9),
                Edit::new(r, fields::risk::IMPACT, "alto"),
                Edit::new(r, fields::risk::RESPONSE, "desconhecida"),
            ],
            &mut ids,
        );
        let s = State::replay(&ops);
        let risk = Risk::read(rid, s.entity(rid).unwrap());
        assert_eq!((risk.probability, risk.impact), (5, 1));
        assert_eq!(risk.response, RiskResponse::Mitigate);
        assert_eq!(risk.level(), 5);
        assert!(!risk.is_high());
    }

    #[test]
    fn enum_keys() {
        assert_eq!(enum_value(Column::Todo), Value::from("a_fazer"));
        assert_eq!(
            enum_value(ProjectStatus::Running),
            Value::from("em_execucao")
        );
        assert_eq!(enum_value(SavingStatus::Validated), Value::from("validado"));
        assert_eq!(
            enum_value(RoleKind::ProcessOwner),
            Value::from("dono_do_processo")
        );
        assert!(SavingStatus::Estimated < SavingStatus::Validated);
        for a in Approach::ALL {
            assert!(!a.phases().is_empty());
        }
    }

    #[test]
    fn register_person_only_changes_what_differs() {
        let mut ids = gen_ids();
        let me = ids();
        let s = State::new();
        let edits = Person::register(&s, me, "Ana", "ana@exemplo.com");
        assert_eq!(edits.len(), 2);
        let s = State::replay(&s.prepare(me, 0, edits, &mut ids));
        assert!(Person::register(&s, me, "Ana", "ana@exemplo.com").is_empty());
        assert_eq!(
            Person::register(&s, me, "Ana Lima", "ana@exemplo.com").len(),
            1
        );
        assert_eq!(person_name(&s, me), "Ana");
        let stranger = ids();
        assert_eq!(person_name(&s, stranger), stranger.short());
    }
}
