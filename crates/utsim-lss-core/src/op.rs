//! Operations: the unit of the project journal (`docs/plano_lean_seis_sigma.md`, 4.4).
//!
//! An operation sets one field of one entity:
//! `{id, autor, instante, relogio_lamport, entidade, campo, valor, base}`, where `base` is
//! the version of the field the author saw (the id of the operation that last wrote it, or
//! absent if the field was empty). The JSON keys are the Portuguese ones of the plan; the
//! Rust names are English.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::Id;

/// Kinds of entity (`docs/plano_lean_seis_sigma.md`, 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Kind {
    #[serde(rename = "carteira")]
    Portfolio,
    #[serde(rename = "projeto")]
    Project,
    #[serde(rename = "fase")]
    Phase,
    #[serde(rename = "entregavel")]
    Deliverable,
    #[serde(rename = "tollgate")]
    Tollgate,
    #[serde(rename = "item_checklist")]
    ChecklistItem,
    #[serde(rename = "pessoa")]
    Person,
    #[serde(rename = "papel")]
    Role,
    #[serde(rename = "acao")]
    Action,
    #[serde(rename = "risco")]
    Risk,
    #[serde(rename = "reuniao")]
    Meeting,
    #[serde(rename = "ganho")]
    Saving,
    #[serde(rename = "conjunto_de_dados")]
    Dataset,
    #[serde(rename = "analise")]
    Analysis,
    #[serde(rename = "ligacao_modelo")]
    ModelLink,
    #[serde(rename = "estudo")]
    Study,
    #[serde(rename = "licao")]
    Lesson,
    #[serde(rename = "anexo")]
    Attachment,
    // Stage L1 (project management).
    #[serde(rename = "programa")]
    Program,
    #[serde(rename = "unidade_negocio")]
    BusinessUnit,
    #[serde(rename = "ideia")]
    Idea,
    #[serde(rename = "objetivo_estrategico")]
    StrategicObjective,
    #[serde(rename = "criterio")]
    Criterion,
    #[serde(rename = "versao_termo")]
    CharterVersion,
    #[serde(rename = "celula_raci")]
    RaciCell,
    #[serde(rename = "decisao_tollgate")]
    TollgateDecision,
    #[serde(rename = "item_ata")]
    MinutesItem,
    #[serde(rename = "tarefa")]
    Task,
    #[serde(rename = "feriado")]
    Holiday,
    #[serde(rename = "lancamento_ganho")]
    SavingEntry,
    /// A row or node of a tool deliverable (an FMEA row, an Ishikawa cause, a CTQ...).
    #[serde(rename = "item_ferramenta")]
    ToolItem,
    // Stage L6 (implementation program, `docs/plano_lean_seis_sigma.md`, 8.1 to 8.4, 8.8).
    #[serde(rename = "meta_programa")]
    ProgramTarget,
    #[serde(rename = "oportunidade")]
    Opportunity,
    #[serde(rename = "decisao_comite")]
    CommitteeDecision,
    #[serde(rename = "designacao")]
    Assignment,
    #[serde(rename = "ausencia")]
    Absence,
    /// Main entity of a belt's coaching log folder (restricted to the MBB and the belt).
    #[serde(rename = "diario_mentoria")]
    CoachingLog,
    #[serde(rename = "sessao_mentoria")]
    CoachingSession,
    #[serde(rename = "compromisso")]
    Commitment,
    #[serde(rename = "competencia")]
    Competence,
    #[serde(rename = "revisao_tollgate")]
    TollgateReview,
    #[serde(rename = "pergunta_revisao")]
    ReviewQuestion,
    /// A profile of a person in the program's role list (kept by the MBB).
    #[serde(rename = "perfil")]
    ProgramRole,
}

impl Kind {
    pub const ALL: [Kind; 43] = [
        Kind::Portfolio,
        Kind::Project,
        Kind::Phase,
        Kind::Deliverable,
        Kind::Tollgate,
        Kind::ChecklistItem,
        Kind::Person,
        Kind::Role,
        Kind::Action,
        Kind::Risk,
        Kind::Meeting,
        Kind::Saving,
        Kind::Dataset,
        Kind::Analysis,
        Kind::ModelLink,
        Kind::Study,
        Kind::Lesson,
        Kind::Attachment,
        Kind::Program,
        Kind::BusinessUnit,
        Kind::Idea,
        Kind::StrategicObjective,
        Kind::Criterion,
        Kind::CharterVersion,
        Kind::RaciCell,
        Kind::TollgateDecision,
        Kind::MinutesItem,
        Kind::Task,
        Kind::Holiday,
        Kind::SavingEntry,
        Kind::ToolItem,
        Kind::ProgramTarget,
        Kind::Opportunity,
        Kind::CommitteeDecision,
        Kind::Assignment,
        Kind::Absence,
        Kind::CoachingLog,
        Kind::CoachingSession,
        Kind::Commitment,
        Kind::Competence,
        Kind::TollgateReview,
        Kind::ReviewQuestion,
        Kind::ProgramRole,
    ];

    /// The key used in the files (`acao`, `projeto`...).
    pub fn key(self) -> &'static str {
        match self {
            Kind::Portfolio => "carteira",
            Kind::Project => "projeto",
            Kind::Phase => "fase",
            Kind::Deliverable => "entregavel",
            Kind::Tollgate => "tollgate",
            Kind::ChecklistItem => "item_checklist",
            Kind::Person => "pessoa",
            Kind::Role => "papel",
            Kind::Action => "acao",
            Kind::Risk => "risco",
            Kind::Meeting => "reuniao",
            Kind::Saving => "ganho",
            Kind::Dataset => "conjunto_de_dados",
            Kind::Analysis => "analise",
            Kind::ModelLink => "ligacao_modelo",
            Kind::Study => "estudo",
            Kind::Lesson => "licao",
            Kind::Attachment => "anexo",
            Kind::Program => "programa",
            Kind::BusinessUnit => "unidade_negocio",
            Kind::Idea => "ideia",
            Kind::StrategicObjective => "objetivo_estrategico",
            Kind::Criterion => "criterio",
            Kind::CharterVersion => "versao_termo",
            Kind::RaciCell => "celula_raci",
            Kind::TollgateDecision => "decisao_tollgate",
            Kind::MinutesItem => "item_ata",
            Kind::Task => "tarefa",
            Kind::Holiday => "feriado",
            Kind::SavingEntry => "lancamento_ganho",
            Kind::ToolItem => "item_ferramenta",
            Kind::ProgramTarget => "meta_programa",
            Kind::Opportunity => "oportunidade",
            Kind::CommitteeDecision => "decisao_comite",
            Kind::Assignment => "designacao",
            Kind::Absence => "ausencia",
            Kind::CoachingLog => "diario_mentoria",
            Kind::CoachingSession => "sessao_mentoria",
            Kind::Commitment => "compromisso",
            Kind::Competence => "competencia",
            Kind::TollgateReview => "revisao_tollgate",
            Kind::ReviewQuestion => "pergunta_revisao",
            Kind::ProgramRole => "perfil",
        }
    }
}

/// Which entity an operation touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    #[serde(rename = "tipo")]
    pub kind: Kind,
    pub id: Id,
}

impl EntityRef {
    pub fn new(kind: Kind, id: Id) -> Self {
        Self { kind, id }
    }
}

/// The total order of operations: `(relogio_lamport, autor, id)`. Every machine sorts the
/// same set of operations the same way, so the reconstruction is identical everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OrderKey {
    pub lamport: u64,
    #[serde(rename = "autor")]
    pub author: Id,
    pub id: Id,
}

/// One operation of the journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub id: Id,
    #[serde(rename = "autor")]
    pub author: Id,
    /// Wall-clock instant of the author's machine, Unix milliseconds. Informative only (the
    /// history shows it); the order comes from the Lamport clock.
    #[serde(rename = "instante")]
    pub at_ms: i64,
    #[serde(rename = "relogio_lamport")]
    pub lamport: u64,
    #[serde(rename = "entidade")]
    pub entity: EntityRef,
    #[serde(rename = "campo")]
    pub field: String,
    #[serde(rename = "valor")]
    pub value: Value,
    /// Version of the field the author saw: id of the operation that wrote it last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Id>,
}

/// Longest field name accepted.
pub const MAX_FIELD_LEN: usize = 64;

/// Field that marks an entity as deleted (`true`) or restored (`false`). Deleting is an
/// ordinary operation, so it can be undone by another one.
pub const FIELD_DELETED: &str = "excluido";

/// Meta field: dismisses the conflict whose replaced operation is the string value. It does
/// not become an entity field.
pub const FIELD_DISMISS_CONFLICT: &str = "$conflito_revisado";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpError {
    #[error("operação sem campo")]
    EmptyField,
    #[error("nome de campo longo demais ({0} caracteres; máximo {MAX_FIELD_LEN})")]
    LongField(usize),
    #[error("campo '{0}' com caracteres inválidos")]
    BadField(String),
    #[error("operação com relógio de Lamport zero")]
    ZeroClock,
    #[error("operação sem identificador ou sem autor")]
    NilId,
}

impl Op {
    pub fn key(&self) -> OrderKey {
        OrderKey {
            lamport: self.lamport,
            author: self.author,
            id: self.id,
        }
    }

    /// Structural checks done on every operation read from disk.
    pub fn validate(&self) -> Result<(), OpError> {
        if self.id.is_nil() || self.author.is_nil() || self.entity.id.is_nil() {
            return Err(OpError::NilId);
        }
        if self.lamport == 0 {
            return Err(OpError::ZeroClock);
        }
        validate_field(&self.field)
    }

    /// Whether this is a meta operation (field starting with `$`), which acts on the
    /// reconstruction instead of setting a field.
    pub fn is_meta(&self) -> bool {
        self.field.starts_with('$')
    }
}

pub fn validate_field(field: &str) -> Result<(), OpError> {
    if field.is_empty() {
        return Err(OpError::EmptyField);
    }
    if field.len() > MAX_FIELD_LEN {
        return Err(OpError::LongField(field.len()));
    }
    let ok = field.chars().enumerate().all(|(i, c)| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || (i == 0 && c == '$')
    });
    if !ok {
        return Err(OpError::BadField(field.to_string()));
    }
    Ok(())
}

/// An edit the interface wants to make: the field of an entity and its new value. The base
/// and the clock are filled when it becomes an [`Op`] ([`crate::state::State::prepare`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub entity: EntityRef,
    pub field: String,
    pub value: Value,
    /// The version of the field the person saw when they started editing, when the
    /// interface knows it (a text box keeps it from the moment it got the focus). `None`:
    /// the current version when the edit is committed.
    pub seen: Option<Option<Id>>,
}

impl Edit {
    pub fn new(entity: EntityRef, field: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            entity,
            field: field.into(),
            value: value.into(),
            seen: None,
        }
    }

    /// Sets the version the person saw (see [`Edit::seen`]).
    pub fn seen(mut self, version: Option<Id>) -> Self {
        self.seen = Some(version);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op() -> Op {
        Op {
            id: Id::v7(10, [1; 10]),
            author: Id::v7(1, [2; 10]),
            at_ms: 10,
            lamport: 3,
            entity: EntityRef::new(Kind::Action, Id::v7(5, [3; 10])),
            field: "titulo".into(),
            value: Value::from("Trocar o rolamento"),
            base: None,
        }
    }

    #[test]
    fn json_keys_are_portuguese() {
        let text = serde_json::to_string(&op()).unwrap();
        for key in [
            "\"id\"",
            "\"autor\"",
            "\"instante\"",
            "\"relogio_lamport\"",
            "\"entidade\"",
            "\"tipo\":\"acao\"",
            "\"campo\"",
            "\"valor\"",
        ] {
            assert!(text.contains(key), "{key} in {text}");
        }
        assert!(!text.contains("base"), "absent base is omitted");
        let back: Op = serde_json::from_str(&text).unwrap();
        assert_eq!(back, op());
    }

    #[test]
    fn kind_keys_match_serde() {
        for k in Kind::ALL {
            assert_eq!(serde_json::to_value(k).unwrap(), Value::from(k.key()));
        }
    }

    #[test]
    fn validation() {
        assert!(op().validate().is_ok());
        let mut o = op();
        o.lamport = 0;
        assert_eq!(o.validate(), Err(OpError::ZeroClock));
        let mut o = op();
        o.field = "Título".into();
        assert!(matches!(o.validate(), Err(OpError::BadField(_))));
        let mut o = op();
        o.field = "a".repeat(65);
        assert_eq!(o.validate(), Err(OpError::LongField(65)));
        let mut o = op();
        o.field = FIELD_DISMISS_CONFLICT.into();
        assert!(o.validate().is_ok());
        assert!(o.is_meta());
        let mut o = op();
        o.author = Id::NIL;
        assert_eq!(o.validate(), Err(OpError::NilId));
    }

    #[test]
    fn order_key_sorts_by_clock_then_author_then_id() {
        let a = Id::v7(1, [0; 10]);
        let b = Id::v7(2, [0; 10]);
        let k = |l, au, id| OrderKey {
            lamport: l,
            author: au,
            id,
        };
        assert!(k(1, b, b) < k(2, a, a));
        assert!(k(2, a, b) < k(2, b, a));
        assert!(k(2, a, a) < k(2, a, b));
    }
}
