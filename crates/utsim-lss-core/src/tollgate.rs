//! Tollgates (`docs/plano_lean_seis_sigma.md`, 1.4): the checklist of a phase, the
//! submission, the approvers' decisions and the frozen hash of the deliverables.
//!
//! - **Checklist**: each item is `manual` (the belt marks it and writes the evidence) or
//!   `verificado` (the app checks the project state: "termo aprovado", "existe FMEA", "toda
//!   ação da fase concluída"...). Required items block the submission.
//! - **Decisions**: each approver records `aprovar`, `aprovar com ressalvas` (each
//!   reservation becomes a card on the board), `refazer`, `suspender` or `cancelar`. The rule
//!   (all, majority, sponsor required) turns the decisions of the round into an outcome, and
//!   the outcome changes the phase and the project.
//! - **Freezing**: every decision stores the content of the phase's deliverables and its
//!   SHA-256; after the approval, a change to that content marks the tollgate as "alterado
//!   depois da aprovação", with the differences, until a new round approves it again.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::board::{self, CardSpec};
use crate::date::Date;
use crate::digest::{self, Change};
use crate::domain::{
    Approach, Column, Origin, Phase, PhaseStatus, Project, ProjectStatus, RoleKind, SavingStatus,
    enum_value, fields as dfields, of_project, phases,
};
use crate::id::Id;
use crate::op::{Edit, EntityRef, FIELD_DELETED, Kind};
use crate::roles;
use crate::state::{Entity, State};

pub mod fields {
    // Tollgate (besides `fase`, `estado`, `hash_congelado` of stage L0).
    pub const RULE: &str = "regra";
    /// Array of person ids; empty: anyone may decide.
    pub const APPROVERS: &str = "aprovadores";
    pub const ROUND: &str = "rodada";
    pub const SUBMITTED_ON: &str = "submetido_em";
    pub const FINAL_DECISION: &str = "decisao_final";
    pub const PLANNED: &str = "data_planejada";
    // Checklist item (besides `tollgate`, `texto`, `obrigatorio`, `feito`, `ordem`).
    pub const ITEM_TYPE: &str = "tipo";
    pub const CHECK: &str = "verificacao";
    pub const EVIDENCE: &str = "evidencia";
    // Decision.
    pub const TOLLGATE: &str = "tollgate";
    pub const APPROVER: &str = "aprovador";
    pub const DECISION: &str = "decisao";
    pub const RESERVATIONS: &str = "ressalvas";
    pub const COMMENT: &str = "comentario";
    pub const DATE: &str = "data";
    pub const HASH: &str = "hash";
    pub const CONTENT: &str = "conteudo";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TollgateStatus {
    #[default]
    #[serde(rename = "aberto")]
    Open,
    #[serde(rename = "em_revisao")]
    Submitted,
    #[serde(rename = "aprovado")]
    Approved,
    #[serde(rename = "refazer")]
    Redo,
    #[serde(rename = "suspenso")]
    Suspended,
    #[serde(rename = "cancelado")]
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ApprovalRule {
    #[serde(rename = "todos")]
    All,
    #[serde(rename = "maioria")]
    Majority,
    /// A sponsor of the project must approve.
    #[default]
    #[serde(rename = "patrocinador")]
    SponsorRequired,
}

impl ApprovalRule {
    pub const ALL: [ApprovalRule; 3] = [
        ApprovalRule::All,
        ApprovalRule::Majority,
        ApprovalRule::SponsorRequired,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Decision {
    #[serde(rename = "aprovar")]
    Approve,
    #[serde(rename = "aprovar_com_ressalvas")]
    ApproveWithReservations,
    #[serde(rename = "refazer")]
    Redo,
    #[serde(rename = "suspender")]
    Suspend,
    #[serde(rename = "cancelar")]
    Cancel,
}

impl Decision {
    pub const ALL: [Decision; 5] = [
        Decision::Approve,
        Decision::ApproveWithReservations,
        Decision::Redo,
        Decision::Suspend,
        Decision::Cancel,
    ];

    pub fn approves(self) -> bool {
        matches!(self, Decision::Approve | Decision::ApproveWithReservations)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ItemType {
    #[default]
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "verificado")]
    Verified,
}

/// What a verified item checks on the project state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Check {
    /// The required charter fields are filled.
    CharterComplete,
    /// The latest charter version is approved and the goal and scope did not change since.
    CharterApproved,
    /// A sponsor and a belt (black or green) are defined.
    TeamDefined,
    /// The schedule has at least one task and no cycle.
    ScheduleDefined,
    /// At least one risk, and every open risk has an owner.
    RisksAssessed,
    /// No open high risk (level ≥ 15) without an owner.
    HighRisksTreated,
    /// At least one saving recorded.
    SavingEstimated,
    /// Every saving is at least forecast (validated by the controller for the forecast).
    SavingsForecast,
    /// At least one saving validated by the controller.
    SavingsValidated,
    /// Every action of the phase is done.
    PhaseActionsDone,
    /// At least one lesson learned.
    LessonsRecorded,
    /// The process owner accepted the transfer.
    OwnerAccepted,
    /// A deliverable of this tool exists in the project (an entity whose kind key starts with
    /// it, or a `entregavel` whose `ferramenta` is it): `fmea`, `sipoc`, `ishikawa`...
    DeliverableExists(String),
}

impl Check {
    /// Key written in the item (`termo_completo`, `entregavel:fmea`...).
    pub fn key(&self) -> String {
        match self {
            Check::CharterComplete => "termo_completo".into(),
            Check::CharterApproved => "termo_aprovado".into(),
            Check::TeamDefined => "equipe_definida".into(),
            Check::ScheduleDefined => "cronograma_definido".into(),
            Check::RisksAssessed => "riscos_avaliados".into(),
            Check::HighRisksTreated => "riscos_altos_tratados".into(),
            Check::SavingEstimated => "ganho_estimado".into(),
            Check::SavingsForecast => "ganhos_previstos".into(),
            Check::SavingsValidated => "ganhos_validados".into(),
            Check::PhaseActionsDone => "acoes_da_fase_concluidas".into(),
            Check::LessonsRecorded => "licoes_registradas".into(),
            Check::OwnerAccepted => "aceite_do_dono".into(),
            Check::DeliverableExists(t) => format!("entregavel:{t}"),
        }
    }

    pub fn from_key(s: &str) -> Option<Check> {
        Some(match s {
            "termo_completo" => Check::CharterComplete,
            "termo_aprovado" => Check::CharterApproved,
            "equipe_definida" => Check::TeamDefined,
            "cronograma_definido" => Check::ScheduleDefined,
            "riscos_avaliados" => Check::RisksAssessed,
            "riscos_altos_tratados" => Check::HighRisksTreated,
            "ganho_estimado" => Check::SavingEstimated,
            "ganhos_previstos" => Check::SavingsForecast,
            "ganhos_validados" => Check::SavingsValidated,
            "acoes_da_fase_concluidas" => Check::PhaseActionsDone,
            "licoes_registradas" => Check::LessonsRecorded,
            "aceite_do_dono" => Check::OwnerAccepted,
            _ => {
                let t = s.strip_prefix("entregavel:")?.trim();
                if t.is_empty() {
                    return None;
                }
                Check::DeliverableExists(t.to_string())
            }
        })
    }
}

/// Does the project pass this check now? `phase` is the phase of the tollgate.
pub fn verify(state: &State, project: Id, phase: Id, check: &Check) -> bool {
    use crate::domain::{Risk, RiskStatus, Saving};
    match check {
        Check::CharterComplete => crate::charter::missing_fields(state, project).is_empty(),
        Check::CharterApproved => matches!(
            crate::charter::status(state, project),
            crate::charter::CharterStatus::Approved { .. }
        ),
        Check::TeamDefined => {
            let rs = roles::roles(state, project);
            rs.iter()
                .any(|r| r.role == RoleKind::Sponsor && r.person.is_some())
                && rs
                    .iter()
                    .any(|r| roles::is_belt(r.role) && r.person.is_some())
        }
        Check::ScheduleDefined => {
            let p = crate::schedule::plan(state, project, Date::new(2000, 1, 1).expect("date"));
            !p.tasks.is_empty() && crate::schedule::compute(&p).is_ok()
        }
        Check::RisksAssessed => {
            let rs = of_project(state, Kind::Risk, project, Risk::read);
            !rs.is_empty()
                && rs
                    .iter()
                    .filter(|r| r.status == RiskStatus::Open)
                    .all(|r| r.owner.is_some())
        }
        Check::HighRisksTreated => of_project(state, Kind::Risk, project, Risk::read)
            .iter()
            .filter(|r| r.status == RiskStatus::Open && r.is_high())
            .all(|r| r.owner.is_some()),
        Check::SavingEstimated => {
            !of_project(state, Kind::Saving, project, Saving::read).is_empty()
        }
        Check::SavingsForecast => {
            let s = of_project(state, Kind::Saving, project, Saving::read);
            !s.is_empty() && s.iter().all(|x| x.status >= SavingStatus::Forecast)
        }
        Check::SavingsValidated => of_project(state, Kind::Saving, project, Saving::read)
            .iter()
            .any(|x| x.status == SavingStatus::Validated),
        Check::PhaseActionsDone => {
            let key = phase.to_string();
            of_project(state, Kind::Action, project, |id, e| (id, e.clone()))
                .iter()
                .filter(|(_, e)| e.text(dfields::action::PHASE) == key)
                .all(|(_, e)| e.typed::<Column>(dfields::action::COLUMN) == Some(Column::Done))
        }
        Check::LessonsRecorded => !of_project(state, Kind::Lesson, project, |id, _| id).is_empty(),
        Check::OwnerAccepted => state
            .entity(project)
            .is_some_and(|e| e.boolean(crate::closure::fields::OWNER_ACCEPTED)),
        Check::DeliverableExists(tool) => deliverable_exists(state, project, tool),
    }
}

/// A deliverable of the tool (stage L2, `crate::tools`) with at least one row or node, in
/// any phase of the project (the FMEA of Analisar is the one revised in Melhorar). A key
/// that is not an L2 tool falls back to any entity whose kind key starts with it.
fn deliverable_exists(state: &State, project: Id, tool: &str) -> bool {
    if let Ok(t) = serde_json::from_value::<crate::tools::Tool>(Value::from(tool)) {
        return crate::tools::of_tool(state, project, t)
            .iter()
            .any(|d| !crate::tools::all_items(state, d.id).is_empty());
    }
    let key = project.to_string();
    let phase_ids: BTreeSet<String> = phases(state, project)
        .iter()
        .map(|p| p.id.to_string())
        .collect();
    state.entities.values().any(|e| {
        if e.is_deleted() {
            return false;
        }
        if e.kind == Kind::Deliverable {
            return e.text(dfields::deliverable::TOOL) == tool
                && phase_ids.contains(&e.text(dfields::deliverable::PHASE));
        }
        e.kind.key().starts_with(tool)
            && (e.text("projeto") == key || phase_ids.contains(&e.text("fase")))
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tollgate {
    pub id: Id,
    pub phase: Id,
    pub status: TollgateStatus,
    pub frozen_hash: String,
    pub rule: ApprovalRule,
    pub approvers: Vec<Id>,
    pub round: i64,
    pub submitted_on: Option<Date>,
    pub final_decision: Option<Id>,
}

impl Tollgate {
    pub fn read(id: Id, e: &Entity) -> Self {
        use dfields::tollgate as t;
        Self {
            id,
            phase: e.id(t::PHASE).unwrap_or(Id::NIL),
            status: e.typed(t::STATUS).unwrap_or_default(),
            frozen_hash: e.text(t::FROZEN_HASH),
            rule: e.typed(fields::RULE).unwrap_or_default(),
            approvers: e
                .get(fields::APPROVERS)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str()?.parse().ok()).collect())
                .unwrap_or_default(),
            round: e.integer(fields::ROUND).unwrap_or(0),
            submitted_on: e
                .opt_text(fields::SUBMITTED_ON)
                .and_then(|s| s.parse().ok()),
            final_decision: e.id(fields::FINAL_DECISION),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Tollgate, id)
    }
}

/// The tollgate of a phase (the oldest, if two were created at once).
pub fn of_phase(state: &State, phase: Id) -> Option<Tollgate> {
    state
        .of_kind(Kind::Tollgate)
        .find(|(_, e)| e.id(dfields::tollgate::PHASE) == Some(phase))
        .map(|(id, e)| Tollgate::read(id, e))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: Id,
    pub text: String,
    pub required: bool,
    pub done: bool,
    pub order: f64,
    pub item_type: ItemType,
    pub check: Option<Check>,
    pub evidence: String,
}

impl Item {
    pub fn read(id: Id, e: &Entity) -> Self {
        use dfields::checklist_item as c;
        Self {
            id,
            text: e.text(c::TEXT),
            required: e.boolean(c::REQUIRED),
            done: e.boolean(c::DONE),
            order: e.number(c::ORDER).unwrap_or(0.0),
            item_type: e.typed(fields::ITEM_TYPE).unwrap_or_default(),
            check: Check::from_key(&e.text(fields::CHECK)),
            evidence: e.text(fields::EVIDENCE),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::ChecklistItem, id)
    }
}

pub fn items(state: &State, tollgate: Id) -> Vec<Item> {
    let mut v: Vec<Item> = state
        .of_kind(Kind::ChecklistItem)
        .filter(|(_, e)| e.id(dfields::checklist_item::TOLLGATE) == Some(tollgate))
        .map(|(id, e)| Item::read(id, e))
        .collect();
    v.sort_by(|a, b| a.order.total_cmp(&b.order).then(a.id.cmp(&b.id)));
    v
}

/// An item and whether it is satisfied now.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemResult {
    pub item: Item,
    pub ok: bool,
}

pub fn evaluate(state: &State, project: Id, tollgate: &Tollgate) -> Vec<ItemResult> {
    items(state, tollgate.id)
        .into_iter()
        .map(|item| {
            let ok = match (&item.item_type, &item.check) {
                (ItemType::Verified, Some(c)) => verify(state, project, tollgate.phase, c),
                _ => item.done,
            };
            ItemResult { item, ok }
        })
        .collect()
}

/// A default checklist line: text, required, check (verified) or manual.
pub type Template = (&'static str, bool, Option<Check>);

fn dmaic_define() -> Vec<Template> {
    vec![
        (
            "Termo de abertura completo",
            true,
            Some(Check::CharterComplete),
        ),
        (
            "Termo de abertura aprovado pelo patrocinador",
            true,
            Some(Check::CharterApproved),
        ),
        (
            "Equipe e papéis definidos (patrocinador e belt)",
            true,
            Some(Check::TeamDefined),
        ),
        (
            "VOC e árvore CTQ",
            false,
            Some(Check::DeliverableExists("voc_ctq".into())),
        ),
        (
            "SIPOC do processo",
            true,
            Some(Check::DeliverableExists("sipoc".into())),
        ),
        ("Mapa de processo de alto nível", false, None),
        (
            "Cronograma com os tollgates",
            true,
            Some(Check::ScheduleDefined),
        ),
        (
            "Riscos iniciais avaliados, cada um com dono",
            true,
            Some(Check::RisksAssessed),
        ),
        (
            "Ganho estimado registrado",
            true,
            Some(Check::SavingEstimated),
        ),
    ]
}

/// The default checklist of a phase of an approach (section 1.4 for DMAIC, 1.5 for the
/// others). The company template of section 4.7 will replace this.
pub fn default_checklist(approach: Approach, phase_index: usize) -> Vec<Template> {
    let fmea = || Some(Check::DeliverableExists("fmea".into()));
    let control_plan = || Some(Check::DeliverableExists("plano_controle".into()));
    let common_end = || -> Vec<Template> {
        vec![
            (
                "Transferência aceita pelo dono do processo",
                true,
                Some(Check::OwnerAccepted),
            ),
            (
                "Lições aprendidas registradas",
                true,
                Some(Check::LessonsRecorded),
            ),
        ]
    };
    match (approach, phase_index) {
        (Approach::Dmaic | Approach::Dmadv, 0) => dmaic_define(),
        (Approach::Dmaic, 1) => vec![
            ("Definição operacional do Y", true, None),
            (
                "Plano de coleta de dados",
                true,
                Some(Check::DeliverableExists("plano_coleta".into())),
            ),
            (
                "MSA aprovado (%GRR ≤ 30 % ou justificativa registrada)",
                true,
                None,
            ),
            (
                "Linha de base de estabilidade e capacidade (dado de origem Real)",
                true,
                None,
            ),
            ("Mapa detalhado do processo e VSM atual", false, None),
            ("Meta confirmada com o patrocinador", true, None),
        ],
        (Approach::Dmaic, 2) => vec![
            (
                "Causas potenciais (Ishikawa ou matriz C&E)",
                true,
                Some(Check::DeliverableExists("ishikawa".into())),
            ),
            ("Causas raiz verificadas com dados", true, None),
            ("Gargalo e desperdícios quantificados", false, None),
            ("FMEA do processo atual", true, fmea()),
        ],
        (Approach::Dmaic, 3) => vec![
            ("Soluções geradas e selecionadas", true, None),
            (
                "Piloto com comparação estatística antes × depois",
                true,
                None,
            ),
            ("FMEA revisado", true, fmea()),
            (
                "Ações da fase concluídas ou transferidas",
                true,
                Some(Check::PhaseActionsDone),
            ),
            (
                "Ganho previsto validado pela controladoria",
                true,
                Some(Check::SavingsForecast),
            ),
            (
                "Nenhum risco alto sem dono",
                false,
                Some(Check::HighRisksTreated),
            ),
            ("Plano de implantação", false, None),
        ],
        (Approach::Dmaic, 4) => {
            let mut v = vec![
                ("Plano de controle e de resposta", true, control_plan()),
                ("Cartas de controle em operação", true, None),
                ("Capacidade final comparada com a linha de base", true, None),
                ("Procedimentos atualizados", false, None),
                (
                    "Ganhos validados pela controladoria",
                    true,
                    Some(Check::SavingsValidated),
                ),
            ];
            v.extend(common_end());
            v
        }
        (Approach::Dmadv, 1) => vec![
            ("VOC, CTQs e casa da qualidade", true, None),
            (
                "Plano de coleta de dados",
                false,
                Some(Check::DeliverableExists("plano_coleta".into())),
            ),
        ],
        (Approach::Dmadv, 2) => vec![
            (
                "Conceitos gerados e escolhidos (matriz de Pugh)",
                true,
                None,
            ),
            ("Análise funcional", false, None),
        ],
        (Approach::Dmadv, 3) => vec![
            ("Projeto detalhado e tolerâncias", true, None),
            ("FMEA de projeto", true, fmea()),
            ("Processo novo testado no simulador", false, None),
        ],
        (Approach::Dmadv, 4) => {
            let mut v = vec![
                ("Piloto com capacidade verificada", true, None),
                ("Plano de controle", true, control_plan()),
            ];
            v.extend(common_end());
            v
        }
        (Approach::Kaizen, 0) => vec![
            ("Escopo, dados, equipe e logística do evento", true, None),
            (
                "Termo de abertura completo",
                true,
                Some(Check::CharterComplete),
            ),
        ],
        (Approach::Kaizen, 1) => vec![
            (
                "Pauta dos dias cumprida (entender, medir, analisar, melhorar, padronizar)",
                true,
                None,
            ),
            ("Padrão novo apresentado à liderança", true, None),
        ],
        (Approach::Kaizen, _) => vec![
            (
                "Pendências de 30, 60 e 90 dias concluídas",
                true,
                Some(Check::PhaseActionsDone),
            ),
            (
                "Lições aprendidas registradas",
                true,
                Some(Check::LessonsRecorded),
            ),
        ],
        _ => vec![("Entregáveis da fase concluídos", true, None)],
    }
}

/// Edits that create the tollgate of a phase with its default checklist and approvers
/// (the sponsors and champions of the project). Nothing if the phase already has one.
pub fn prepare(state: &State, project: Id, phase: Id, mut ids: impl FnMut() -> Id) -> Vec<Edit> {
    if of_phase(state, phase).is_some() {
        return Vec::new();
    }
    let approach = state
        .entity(project)
        .map(|e| Project::read(project, e).approach)
        .unwrap_or_default();
    let index = phases(state, project)
        .iter()
        .position(|p| p.id == phase)
        .unwrap_or(0);
    let t = Tollgate::entity_ref(ids());
    let mut approvers = roles::holders(state, project, RoleKind::Sponsor);
    approvers.extend(roles::holders(state, project, RoleKind::Champion));
    approvers.sort();
    approvers.dedup();
    let mut edits = vec![
        Edit::new(t, dfields::tollgate::PHASE, phase.to_string()),
        Edit::new(
            t,
            dfields::tollgate::STATUS,
            enum_value(TollgateStatus::Open),
        ),
        Edit::new(t, fields::RULE, enum_value(ApprovalRule::default())),
        Edit::new(t, fields::APPROVERS, ids_value(&approvers)),
        Edit::new(t, fields::ROUND, 0),
    ];
    for (i, (text, required, check)) in default_checklist(approach, index).into_iter().enumerate() {
        edits.extend(item_edits(
            t.id,
            ids(),
            text,
            required,
            check,
            i as f64 + 1.0,
        ));
    }
    edits
}

fn ids_value(v: &[Id]) -> Value {
    Value::from(
        v.iter()
            .map(|i| Value::from(i.to_string()))
            .collect::<Vec<_>>(),
    )
}

/// Edits that create one checklist item.
pub fn item_edits(
    tollgate: Id,
    id: Id,
    text: &str,
    required: bool,
    check: Option<Check>,
    order: f64,
) -> Vec<Edit> {
    use dfields::checklist_item as c;
    let r = Item::entity_ref(id);
    let kind = if check.is_some() {
        ItemType::Verified
    } else {
        ItemType::Manual
    };
    let mut e = vec![
        Edit::new(r, c::TOLLGATE, tollgate.to_string()),
        Edit::new(r, c::TEXT, text),
        Edit::new(r, c::REQUIRED, required),
        Edit::new(r, c::ORDER, order),
        Edit::new(r, fields::ITEM_TYPE, enum_value(kind)),
    ];
    if let Some(ch) = check {
        e.push(Edit::new(r, fields::CHECK, ch.key()));
    }
    e
}

/// Edit that sets the approvers of a tollgate.
pub fn set_approvers(tollgate: Id, approvers: &[Id]) -> Edit {
    let mut v = approvers.to_vec();
    v.sort();
    v.dedup();
    Edit::new(
        Tollgate::entity_ref(tollgate),
        fields::APPROVERS,
        ids_value(&v),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TollgateError {
    #[error("tollgate não encontrado")]
    NotFound,
    #[error("faltam {0} itens obrigatórios do checklist")]
    Blocked(usize),
    #[error("o tollgate não está em revisão")]
    NotSubmitted,
    #[error("o tollgate já está em revisão ou encerrado")]
    CannotSubmit,
    #[error("você não está entre os aprovadores deste tollgate")]
    NotApprover,
    #[error("escreva ao menos uma ressalva")]
    NoReservations,
    #[error("o projeto não está suspenso")]
    NotSuspended,
}

/// Content of the phase's deliverables: `tipo/id/campo → valor` for every entity of the
/// phase that is a deliverable (an `entregavel` or a tool entity of a later stage), plus
/// the checklist items' text, mark and evidence. Deleted entities count (their
/// `excluido`), so removing a deliverable is a change too.
pub fn content(state: &State, tollgate: &Tollgate) -> BTreeMap<String, Value> {
    const NOT_DELIVERABLES: [Kind; 30] = [
        Kind::Portfolio,
        Kind::Project,
        Kind::Phase,
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
    ];
    let phase = tollgate.phase.to_string();
    // Deliverables of this phase, whose rows and nodes (stage L2) count too.
    let of_phase: BTreeSet<String> = state
        .entities
        .iter()
        .filter(|(_, e)| e.kind == Kind::Deliverable && e.text("fase") == phase)
        .map(|(id, _)| id.to_string())
        .collect();
    let mut out = BTreeMap::new();
    for (id, e) in &state.entities {
        if e.kind == Kind::ToolItem
            && of_phase.contains(&e.text(crate::tools::fields::item::DELIVERABLE))
        {
            for (f, v) in &e.fields {
                out.insert(format!("{}/{id}/{f}", e.kind.key()), v.value.clone());
            }
            continue;
        }
        let item_of_this = e.kind == Kind::ChecklistItem
            && e.id(dfields::checklist_item::TOLLGATE) == Some(tollgate.id);
        if item_of_this {
            use dfields::checklist_item as c;
            for f in [c::TEXT, c::DONE, fields::EVIDENCE, FIELD_DELETED] {
                if let Some(v) = e.get(f) {
                    out.insert(format!("{}/{id}/{f}", e.kind.key()), v.clone());
                }
            }
            continue;
        }
        if NOT_DELIVERABLES.contains(&e.kind) || e.text("fase") != phase {
            continue;
        }
        for (f, v) in &e.fields {
            out.insert(format!("{}/{id}/{f}", e.kind.key()), v.value.clone());
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionRecord {
    pub id: Id,
    pub tollgate: Id,
    pub approver: Option<Id>,
    pub decision: Option<Decision>,
    pub reservations: Vec<String>,
    pub comment: String,
    pub date: Option<Date>,
    pub round: i64,
    pub hash: String,
    pub content: BTreeMap<String, Value>,
}

impl DecisionRecord {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields as f;
        Self {
            id,
            tollgate: e.id(f::TOLLGATE).unwrap_or(Id::NIL),
            approver: e.id(f::APPROVER),
            decision: e.typed(f::DECISION),
            reservations: e
                .get(f::RESERVATIONS)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            comment: e.text(f::COMMENT),
            date: e.opt_text(f::DATE).and_then(|s| s.parse().ok()),
            round: e.integer(f::ROUND).unwrap_or(0),
            hash: e.text(f::HASH),
            content: digest::as_map(e.get(f::CONTENT)),
        }
    }
}

/// Every decision of a tollgate, oldest first.
pub fn decisions(state: &State, tollgate: Id) -> Vec<DecisionRecord> {
    state
        .of_kind(Kind::TollgateDecision)
        .filter(|(_, e)| e.id(fields::TOLLGATE) == Some(tollgate))
        .map(|(id, e)| DecisionRecord::read(id, e))
        .collect()
}

/// What the decisions of the current round add up to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pending { approvals: usize, voters: usize },
    Approved { with_reservations: bool },
    Redo,
    Suspended,
    Cancelled,
}

/// The outcome of the latest decision of each approver under `rule`. `approvers` empty:
/// whoever decided votes. `sponsors`: the project's sponsors (for the sponsor rule).
pub fn resolve(
    rule: ApprovalRule,
    approvers: &[Id],
    sponsors: &[Id],
    latest: &BTreeMap<Id, Decision>,
) -> Outcome {
    let votes: Vec<(Id, Decision)> = latest
        .iter()
        .filter(|(who, _)| approvers.is_empty() || approvers.contains(who))
        .map(|(w, d)| (*w, *d))
        .collect();
    if votes.iter().any(|(_, d)| *d == Decision::Cancel) {
        return Outcome::Cancelled;
    }
    if votes.iter().any(|(_, d)| *d == Decision::Suspend) {
        return Outcome::Suspended;
    }
    let voters = if approvers.is_empty() {
        votes.len()
    } else {
        approvers.len()
    };
    let approvals = votes.iter().filter(|(_, d)| d.approves()).count();
    let redos = votes.iter().filter(|(_, d)| *d == Decision::Redo).count();
    let sponsor_says =
        |pred: fn(Decision) -> bool| votes.iter().any(|(w, d)| sponsors.contains(w) && pred(*d));
    let approved = match rule {
        ApprovalRule::All => voters > 0 && approvals == voters,
        ApprovalRule::Majority => voters > 0 && approvals * 2 > voters,
        ApprovalRule::SponsorRequired => sponsor_says(Decision::approves),
    };
    if approved {
        let with_reservations = votes
            .iter()
            .any(|(_, d)| *d == Decision::ApproveWithReservations);
        return Outcome::Approved { with_reservations };
    }
    let redo = match rule {
        ApprovalRule::All => redos > 0,
        ApprovalRule::Majority => redos * 2 >= voters && redos > 0,
        ApprovalRule::SponsorRequired => sponsor_says(|d| d == Decision::Redo),
    };
    if redo {
        Outcome::Redo
    } else {
        Outcome::Pending { approvals, voters }
    }
}

fn latest_of_round(state: &State, tg: &Tollgate) -> BTreeMap<Id, Decision> {
    let mut latest = BTreeMap::new();
    for d in decisions(state, tg.id)
        .into_iter()
        .filter(|d| d.round == tg.round)
    {
        if let (Some(w), Some(dec)) = (d.approver, d.decision) {
            latest.insert(w, dec);
        }
    }
    latest
}

/// The outcome of the current round.
pub fn outcome(state: &State, project: Id, tg: &Tollgate) -> Outcome {
    let sponsors = roles::holders(state, project, RoleKind::Sponsor);
    resolve(
        tg.rule,
        &tg.approvers,
        &sponsors,
        &latest_of_round(state, tg),
    )
}

fn find(state: &State, tollgate: Id) -> Result<Tollgate, TollgateError> {
    state
        .entity(tollgate)
        .filter(|e| e.kind == Kind::Tollgate && !e.is_deleted())
        .map(|e| Tollgate::read(tollgate, e))
        .ok_or(TollgateError::NotFound)
}

/// Edits that submit the tollgate for review (a new round): every required item must be
/// satisfied. An approved tollgate changed after the approval can be submitted again.
pub fn submit(
    state: &State,
    project: Id,
    tollgate: Id,
    today: Date,
) -> Result<Vec<Edit>, TollgateError> {
    let tg = find(state, tollgate)?;
    if matches!(
        tg.status,
        TollgateStatus::Submitted | TollgateStatus::Cancelled | TollgateStatus::Suspended
    ) {
        return Err(TollgateError::CannotSubmit);
    }
    let missing = evaluate(state, project, &tg)
        .iter()
        .filter(|r| r.item.required && !r.ok)
        .count();
    if missing > 0 {
        return Err(TollgateError::Blocked(missing));
    }
    let t = Tollgate::entity_ref(tollgate);
    let mut e = vec![
        Edit::new(
            t,
            dfields::tollgate::STATUS,
            enum_value(TollgateStatus::Submitted),
        ),
        Edit::new(t, fields::ROUND, tg.round + 1),
        Edit::new(t, fields::SUBMITTED_ON, today.to_string()),
    ];
    let phase_approved = phase_status(state, tg.phase) == PhaseStatus::Approved;
    if !phase_approved {
        e.push(phase_edit(tg.phase, PhaseStatus::InTollgate));
    }
    Ok(e)
}

fn phase_status(state: &State, phase: Id) -> PhaseStatus {
    state
        .entity(phase)
        .map(|e| Phase::read(phase, e).status)
        .unwrap_or_default()
}

fn phase_edit(phase: Id, status: PhaseStatus) -> Edit {
    Edit::new(
        EntityRef::new(Kind::Phase, phase),
        dfields::phase::STATUS,
        enum_value(status),
    )
}

/// What one decision asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionInput {
    pub approver: Id,
    pub decision: Decision,
    pub reservations: Vec<String>,
    pub comment: String,
    pub date: Date,
}

/// Edits that record a decision and, when it resolves the round, its effect:
///
/// - approved: tollgate `aprovado` with the frozen hash, phase approved, the next phase in
///   progress and current;
/// - redo: tollgate `refazer`, phase back in progress;
/// - suspended: tollgate `suspenso`, project `suspensa`;
/// - cancelled: tollgate `cancelado`, project `cancelada`.
///
/// Each reservation of an "aprovar com ressalvas" becomes a card linked to the decision
/// (in the next phase, when there is one).
pub fn decide(
    state: &State,
    project: Id,
    tollgate: Id,
    input: &DecisionInput,
    mut ids: impl FnMut() -> Id,
) -> Result<Vec<Edit>, TollgateError> {
    let tg = find(state, tollgate)?;
    if tg.status != TollgateStatus::Submitted {
        return Err(TollgateError::NotSubmitted);
    }
    if !tg.approvers.is_empty() && !tg.approvers.contains(&input.approver) {
        return Err(TollgateError::NotApprover);
    }
    let reservations: Vec<String> = input
        .reservations
        .iter()
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .collect();
    if input.decision == Decision::ApproveWithReservations && reservations.is_empty() {
        return Err(TollgateError::NoReservations);
    }
    let frozen = content(state, &tg);
    let hash = digest::hash_map(&frozen);
    let did = ids();
    let d = EntityRef::new(Kind::TollgateDecision, did);
    let mut edits = vec![
        Edit::new(d, fields::TOLLGATE, tollgate.to_string()),
        Edit::new(d, fields::APPROVER, input.approver.to_string()),
        Edit::new(d, fields::DECISION, enum_value(input.decision)),
        Edit::new(
            d,
            fields::RESERVATIONS,
            Value::from(
                reservations
                    .iter()
                    .map(|r| Value::from(r.as_str()))
                    .collect::<Vec<_>>(),
            ),
        ),
        Edit::new(d, fields::COMMENT, input.comment.trim()),
        Edit::new(d, fields::DATE, input.date.to_string()),
        Edit::new(d, fields::ROUND, tg.round),
        Edit::new(d, fields::HASH, hash.as_str()),
        Edit::new(d, fields::CONTENT, digest::to_value(&frozen)),
    ];
    let all = phases(state, project);
    let pos = all.iter().position(|p| p.id == tg.phase);
    let next = pos.and_then(|i| all.get(i + 1)).map(|p| p.id);
    // Reservations become cards on the board.
    let mut board_state = state.clone();
    for r in &reservations {
        let spec = CardSpec {
            title: r.clone(),
            phase: next.or(Some(tg.phase)),
            ..CardSpec::default()
        };
        let card = board::linked_card(&board_state, project, ids(), &spec, Origin::Tollgate, d);
        // Apply locally so the next card is ordered after this one.
        for op in board_state.prepare(input.approver, 0, card.clone(), &mut ids) {
            let _ = board_state.apply(&op);
        }
        edits.extend(card);
    }
    // The outcome with this decision counted.
    let mut latest = latest_of_round(state, &tg);
    latest.insert(input.approver, input.decision);
    let sponsors = roles::holders(state, project, RoleKind::Sponsor);
    let t = Tollgate::entity_ref(tollgate);
    let p = Project::entity_ref(project);
    match resolve(tg.rule, &tg.approvers, &sponsors, &latest) {
        Outcome::Pending { .. } => {}
        Outcome::Approved { .. } => {
            edits.push(Edit::new(
                t,
                dfields::tollgate::STATUS,
                enum_value(TollgateStatus::Approved),
            ));
            edits.push(Edit::new(t, dfields::tollgate::FROZEN_HASH, hash.as_str()));
            edits.push(Edit::new(t, fields::FINAL_DECISION, did.to_string()));
            if phase_status(state, tg.phase) != PhaseStatus::Approved {
                edits.push(phase_edit(tg.phase, PhaseStatus::Approved));
                if let Some(n) = next {
                    edits.push(phase_edit(n, PhaseStatus::InProgress));
                    edits.push(Edit::new(p, dfields::project::CURRENT_PHASE, n.to_string()));
                }
            }
        }
        Outcome::Redo => {
            edits.push(Edit::new(
                t,
                dfields::tollgate::STATUS,
                enum_value(TollgateStatus::Redo),
            ));
            if phase_status(state, tg.phase) != PhaseStatus::Approved {
                edits.push(phase_edit(tg.phase, PhaseStatus::InProgress));
            }
        }
        Outcome::Suspended => {
            edits.push(Edit::new(
                t,
                dfields::tollgate::STATUS,
                enum_value(TollgateStatus::Suspended),
            ));
            edits.push(Edit::new(
                p,
                dfields::project::STATUS,
                enum_value(ProjectStatus::Suspended),
            ));
        }
        Outcome::Cancelled => {
            edits.push(Edit::new(
                t,
                dfields::tollgate::STATUS,
                enum_value(TollgateStatus::Cancelled),
            ));
            edits.push(Edit::new(
                p,
                dfields::project::STATUS,
                enum_value(ProjectStatus::Cancelled),
            ));
        }
    }
    Ok(edits)
}

/// Edits that resume a suspended project: the project runs again and the tollgate goes
/// back to review in a new round.
pub fn resume(state: &State, project: Id, tollgate: Id) -> Result<Vec<Edit>, TollgateError> {
    let tg = find(state, tollgate)?;
    if tg.status != TollgateStatus::Suspended {
        return Err(TollgateError::NotSuspended);
    }
    let t = Tollgate::entity_ref(tollgate);
    Ok(vec![
        Edit::new(
            t,
            dfields::tollgate::STATUS,
            enum_value(TollgateStatus::Submitted),
        ),
        Edit::new(t, fields::ROUND, tg.round + 1),
        Edit::new(
            Project::entity_ref(project),
            dfields::project::STATUS,
            enum_value(ProjectStatus::Running),
        ),
    ])
}

/// For an approved tollgate: the changes to the deliverables since the approval (empty:
/// unchanged). `None` when the tollgate is not approved.
pub fn changed_after_approval(state: &State, tg: &Tollgate) -> Option<Vec<Change>> {
    if tg.status != TollgateStatus::Approved {
        return None;
    }
    let frozen = tg
        .final_decision
        .and_then(|d| state.entity(d))
        .map(|e| digest::as_map(e.get(fields::CONTENT)))
        .unwrap_or_default();
    let now = content(state, tg);
    if digest::hash_map(&now) == tg.frozen_hash {
        return Some(Vec::new());
    }
    Some(digest::diff(&frozen, &now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Action, fields as df};
    use crate::kanban;

    struct T {
        s: State,
        n: u64,
        p: Id,
        belt: Id,
        sponsor: Id,
        champion: Id,
    }

    impl T {
        fn new(approach: Approach) -> Self {
            let mut t = T {
                s: State::new(),
                n: 0,
                p: Id::NIL,
                belt: Id::NIL,
                sponsor: Id::NIL,
                champion: Id::NIL,
            };
            t.p = t.id();
            t.belt = t.id();
            t.sponsor = t.id();
            t.champion = t.id();
            let mut n = t.n;
            let e = Project::create(t.p, "Refugo", approach, || {
                n += 1;
                Id::v7(n, [n as u8; 10])
            });
            t.n = n;
            t.commit(e);
            for (who, k) in [
                (t.belt, RoleKind::GreenBelt),
                (t.sponsor, RoleKind::Sponsor),
                (t.champion, RoleKind::Champion),
            ] {
                let rid = t.id();
                let e = roles::add_role(&t.s, t.p, rid, who, k);
                t.commit(e);
            }
            t
        }
        fn id(&mut self) -> Id {
            self.n += 1;
            Id::v7(self.n, [self.n as u8; 10])
        }
        fn ids(&mut self) -> impl FnMut() -> Id + '_ {
            move || {
                self.n += 1;
                Id::v7(self.n, [self.n as u8; 10])
            }
        }
        fn commit(&mut self, e: Vec<Edit>) {
            let mut n = self.n;
            let ops = self.s.prepare(self.belt, 0, e, || {
                n += 1;
                Id::v7(n, [n as u8; 10])
            });
            self.n = n;
            for op in ops {
                self.s.apply(&op).unwrap();
            }
        }
        fn phase(&self, i: usize) -> Phase {
            phases(&self.s, self.p)[i].clone()
        }
        fn tollgate(&mut self, i: usize) -> Tollgate {
            let ph = self.phase(i).id;
            let s = self.s.clone();
            let p = self.p;
            let e = prepare(&s, p, ph, self.ids());
            self.commit(e);
            of_phase(&self.s, ph).unwrap()
        }
        fn decide(
            &mut self,
            tg: Id,
            who: Id,
            d: Decision,
            res: &[&str],
        ) -> Result<(), TollgateError> {
            let input = DecisionInput {
                approver: who,
                decision: d,
                reservations: res.iter().map(|s| s.to_string()).collect(),
                comment: String::new(),
                date: Date::new(2026, 11, 3).unwrap(),
            };
            let s = self.s.clone();
            let p = self.p;
            let e = decide(&s, p, tg, &input, self.ids())?;
            self.commit(e);
            Ok(())
        }
        fn mark_all_manual(&mut self, tg: Id) {
            let e: Vec<Edit> = items(&self.s, tg)
                .iter()
                .filter(|i| i.item_type == ItemType::Manual)
                .map(|i| Edit::new(Item::entity_ref(i.id), df::checklist_item::DONE, true))
                .collect();
            self.commit(e);
        }
    }

    fn today() -> Date {
        Date::new(2026, 11, 3).unwrap()
    }

    #[test]
    fn default_checklists_cover_every_phase() {
        for a in Approach::ALL {
            for i in 0..a.phases().len() {
                let c = default_checklist(a, i);
                assert!(!c.is_empty(), "{a:?} {i}");
                assert!(c.iter().any(|x| x.1), "{a:?} {i}: a required item");
            }
        }
        assert_eq!(default_checklist(Approach::Dmaic, 0).len(), 9);
        for c in default_checklist(Approach::Dmaic, 4)
            .iter()
            .filter_map(|x| x.2.clone())
        {
            assert_eq!(Check::from_key(&c.key()), Some(c));
        }
        assert_eq!(Check::from_key("entregavel:"), None);
        assert_eq!(Check::from_key("nada"), None);
    }

    #[test]
    fn verified_items_query_the_project_state() {
        let mut t = T::new(Approach::Dmaic);
        let tg = t.tollgate(2); // Analisar: Ishikawa and FMEA are verified.
        assert!(
            prepare(&t.s, t.p, tg.phase, || Id::NIL).is_empty(),
            "one per phase"
        );
        let fmea_item = |t: &T| {
            evaluate(&t.s, t.p, &tg)
                .into_iter()
                .find(|r| r.item.check == Some(Check::DeliverableExists("fmea".into())))
                .unwrap()
        };
        assert!(!fmea_item(&t).ok);
        // An L2 FMEA deliverable passes only once it has a row; a bare deliverable of
        // another tool does not count.
        use crate::tools::{self, Tool};
        let (sid, fid) = (t.id(), t.id());
        t.commit(tools::Deliverable::create(
            t.p,
            Some(tg.phase),
            Tool::Sipoc,
            sid,
            "SIPOC",
        ));
        t.commit(tools::Deliverable::create(
            t.p,
            Some(tg.phase),
            Tool::Fmea,
            fid,
            "FMEA do processo",
        ));
        assert!(!fmea_item(&t).ok, "FMEA without rows");
        let row = t.id();
        let e = tools::new_item(&t.s, t.p, fid, tools::fmea::ITEM, row, None, vec![]);
        t.commit(e);
        assert!(fmea_item(&t).ok);
        assert!(verify(
            &t.s,
            t.p,
            tg.phase,
            &Check::DeliverableExists("fmea".into())
        ));
        assert!(!verify(
            &t.s,
            t.p,
            tg.phase,
            &Check::DeliverableExists("ishikawa".into())
        ));
        // The rows of the phase's deliverables are part of the frozen content.
        assert!(
            content(&t.s, &tg)
                .keys()
                .any(|k| k.starts_with("item_ferramenta/"))
        );
        // Blocked submission while required items are missing.
        assert!(matches!(
            submit(&t.s, t.p, tg.id, today()),
            Err(TollgateError::Blocked(_))
        ));

        // Team, risks, savings and actions of the phase.
        let p = t.p;
        let ph = tg.phase;
        assert!(verify(&t.s, p, ph, &Check::TeamDefined));
        assert!(!verify(&t.s, p, ph, &Check::RisksAssessed));
        let r = EntityRef::new(Kind::Risk, t.id());
        t.commit(vec![Edit::new(r, df::risk::PROJECT, p.to_string())]);
        assert!(!verify(&t.s, p, ph, &Check::RisksAssessed), "no owner");
        t.commit(vec![Edit::new(r, df::risk::OWNER, t.belt.to_string())]);
        assert!(verify(&t.s, p, ph, &Check::RisksAssessed));
        assert!(verify(&t.s, p, ph, &Check::PhaseActionsDone), "no actions");
        let a = t.id();
        let mut e = kanban::new_card(&t.s, p, a, "Ação", Column::Todo);
        e.push(Edit::new(
            Action::entity_ref(a),
            df::action::PHASE,
            ph.to_string(),
        ));
        t.commit(e);
        assert!(!verify(&t.s, p, ph, &Check::PhaseActionsDone));
        let e = kanban::move_card(&t.s, p, a, Column::Done, 0);
        t.commit(e);
        assert!(verify(&t.s, p, ph, &Check::PhaseActionsDone));
        let sv = EntityRef::new(Kind::Saving, t.id());
        t.commit(vec![Edit::new(sv, df::saving::PROJECT, p.to_string())]);
        assert!(verify(&t.s, p, ph, &Check::SavingEstimated));
        assert!(!verify(&t.s, p, ph, &Check::SavingsForecast));
        t.commit(vec![Edit::new(
            sv,
            df::saving::STATUS,
            enum_value(SavingStatus::Validated),
        )]);
        assert!(verify(&t.s, p, ph, &Check::SavingsForecast));
        assert!(verify(&t.s, p, ph, &Check::SavingsValidated));
    }

    #[test]
    fn approval_freezes_and_advances_and_detects_later_changes() {
        let mut t = T::new(Approach::Pdca);
        let tg = t.tollgate(0);
        assert_eq!(tg.approvers, {
            let mut v = vec![t.sponsor, t.champion];
            v.sort();
            v
        });
        assert_eq!(
            t.decide(tg.id, t.sponsor, Decision::Approve, &[]),
            Err(TollgateError::NotSubmitted)
        );
        t.mark_all_manual(tg.id);
        let e = submit(&t.s, t.p, tg.id, today()).unwrap();
        t.commit(e);
        assert_eq!(t.phase(0).status, PhaseStatus::InTollgate);
        assert_eq!(
            submit(&t.s, t.p, tg.id, today()),
            Err(TollgateError::CannotSubmit)
        );
        assert_eq!(
            t.decide(tg.id, t.belt, Decision::Approve, &[]),
            Err(TollgateError::NotApprover)
        );
        // The champion alone does not approve under the sponsor rule.
        t.decide(tg.id, t.champion, Decision::Approve, &[]).unwrap();
        let tgr = of_phase(&t.s, tg.phase).unwrap();
        assert_eq!(
            outcome(&t.s, t.p, &tgr),
            Outcome::Pending {
                approvals: 1,
                voters: 2
            }
        );
        t.decide(tg.id, t.sponsor, Decision::Approve, &[]).unwrap();
        let tgr = of_phase(&t.s, tg.phase).unwrap();
        assert_eq!(tgr.status, TollgateStatus::Approved);
        assert_eq!(tgr.frozen_hash.len(), 64);
        assert_eq!(t.phase(0).status, PhaseStatus::Approved);
        assert_eq!(t.phase(1).status, PhaseStatus::InProgress);
        let proj = Project::read(t.p, t.s.entity(t.p).unwrap());
        assert_eq!(proj.current_phase, Some(t.phase(1).id));
        assert_eq!(changed_after_approval(&t.s, &tgr), Some(vec![]));
        // Editing an approved deliverable marks the tollgate as changed.
        let item = items(&t.s, tg.id)[0].id;
        t.commit(vec![Edit::new(
            Item::entity_ref(item),
            fields::EVIDENCE,
            "ata de 03/11",
        )]);
        let changes = changed_after_approval(&t.s, &tgr).unwrap();
        assert_eq!(changes.len(), 1);
        assert!(changes[0].key.ends_with("/evidencia"));
        // A new round re-approves it without moving the phases again.
        let e = submit(&t.s, t.p, tg.id, today()).unwrap();
        t.commit(e);
        assert_eq!(t.phase(0).status, PhaseStatus::Approved);
        t.decide(tg.id, t.sponsor, Decision::Approve, &[]).unwrap();
        let tgr = of_phase(&t.s, tg.phase).unwrap();
        assert_eq!(tgr.round, 2);
        assert_eq!(changed_after_approval(&t.s, &tgr), Some(vec![]));
        assert_eq!(t.phase(1).status, PhaseStatus::InProgress);
        assert_eq!(decisions(&t.s, tg.id).len(), 3);
    }

    #[test]
    fn reservations_become_cards_and_redo_suspend_cancel() {
        let mut t = T::new(Approach::Pdca);
        let tg = t.tollgate(0);
        t.mark_all_manual(tg.id);
        let e = submit(&t.s, t.p, tg.id, today()).unwrap();
        t.commit(e);
        assert_eq!(
            t.decide(tg.id, t.sponsor, Decision::ApproveWithReservations, &["  "]),
            Err(TollgateError::NoReservations)
        );
        t.decide(
            tg.id,
            t.sponsor,
            Decision::ApproveWithReservations,
            &["Refazer o MSA com 3 avaliadores", "Anexar a ata"],
        )
        .unwrap();
        let d = decisions(&t.s, tg.id).pop().unwrap();
        assert_eq!(d.reservations.len(), 2);
        let cards = board::cards_linked_to(&t.s, t.p, d.id);
        assert_eq!(cards.len(), 2);
        assert!(cards.iter().all(|c| c.origin == Origin::Tollgate));
        assert!(cards.iter().all(|c| c.phase == Some(t.phase(1).id)));
        let titles: Vec<_> = kanban::board(&t.s, t.p)[&Column::Todo]
            .iter()
            .map(|c| c.title.clone())
            .collect();
        assert_eq!(titles, ["Refazer o MSA com 3 avaliadores", "Anexar a ata"]);
        assert_eq!(
            of_phase(&t.s, tg.phase).unwrap().status,
            TollgateStatus::Approved
        );

        // Next phase: redo, then suspend and resume, then cancel.
        let tg2 = t.tollgate(1);
        t.mark_all_manual(tg2.id);
        let e = submit(&t.s, t.p, tg2.id, today()).unwrap();
        t.commit(e);
        t.decide(tg2.id, t.sponsor, Decision::Redo, &[]).unwrap();
        assert_eq!(
            of_phase(&t.s, tg2.phase).unwrap().status,
            TollgateStatus::Redo
        );
        assert_eq!(t.phase(1).status, PhaseStatus::InProgress);
        let e = submit(&t.s, t.p, tg2.id, today()).unwrap();
        t.commit(e);
        t.decide(tg2.id, t.champion, Decision::Suspend, &[])
            .unwrap();
        let st = |t: &T| Project::read(t.p, t.s.entity(t.p).unwrap()).status;
        assert_eq!(st(&t), ProjectStatus::Suspended);
        assert_eq!(resume(&t.s, t.p, tg.id), Err(TollgateError::NotSuspended));
        let e = resume(&t.s, t.p, tg2.id).unwrap();
        t.commit(e);
        assert_eq!(st(&t), ProjectStatus::Running);
        let tgr = of_phase(&t.s, tg2.phase).unwrap();
        assert_eq!(
            outcome(&t.s, t.p, &tgr),
            Outcome::Pending {
                approvals: 0,
                voters: 2
            },
            "new round"
        );
        t.decide(tg2.id, t.sponsor, Decision::Cancel, &[]).unwrap();
        assert_eq!(st(&t), ProjectStatus::Cancelled);
    }

    #[test]
    fn approval_rules() {
        let (a, b, c) = (Id::v7(1, [1; 10]), Id::v7(2, [2; 10]), Id::v7(3, [3; 10]));
        let m = |v: &[(Id, Decision)]| v.iter().copied().collect::<BTreeMap<_, _>>();
        use Decision::*;
        let voters = [a, b, c];
        assert_eq!(
            resolve(
                ApprovalRule::All,
                &voters,
                &[],
                &m(&[(a, Approve), (b, Approve)])
            ),
            Outcome::Pending {
                approvals: 2,
                voters: 3
            }
        );
        assert_eq!(
            resolve(
                ApprovalRule::All,
                &voters,
                &[],
                &m(&[(a, Approve), (b, Approve), (c, ApproveWithReservations)])
            ),
            Outcome::Approved {
                with_reservations: true
            }
        );
        assert_eq!(
            resolve(ApprovalRule::All, &voters, &[], &m(&[(a, Redo)])),
            Outcome::Redo
        );
        assert_eq!(
            resolve(
                ApprovalRule::Majority,
                &voters,
                &[],
                &m(&[(a, Approve), (b, Approve), (c, Redo)])
            ),
            Outcome::Approved {
                with_reservations: false
            }
        );
        assert_eq!(
            resolve(
                ApprovalRule::Majority,
                &voters,
                &[],
                &m(&[(a, Approve), (b, Redo)])
            ),
            Outcome::Pending {
                approvals: 1,
                voters: 3
            }
        );
        assert_eq!(
            resolve(
                ApprovalRule::Majority,
                &voters,
                &[],
                &m(&[(a, Redo), (b, Redo)])
            ),
            Outcome::Redo
        );
        assert_eq!(
            resolve(
                ApprovalRule::SponsorRequired,
                &voters,
                &[c],
                &m(&[(a, Approve), (b, Approve)])
            ),
            Outcome::Pending {
                approvals: 2,
                voters: 3
            }
        );
        assert_eq!(
            resolve(
                ApprovalRule::SponsorRequired,
                &voters,
                &[c],
                &m(&[(c, Approve)])
            ),
            Outcome::Approved {
                with_reservations: false
            }
        );
        assert_eq!(
            resolve(
                ApprovalRule::SponsorRequired,
                &[],
                &[c],
                &m(&[(a, Approve), (c, Suspend)])
            ),
            Outcome::Suspended
        );
        assert_eq!(
            resolve(
                ApprovalRule::Majority,
                &[],
                &[],
                &m(&[(a, Cancel), (b, Suspend)])
            ),
            Outcome::Cancelled
        );
        // Votes of people outside the approvers list are ignored.
        assert_eq!(
            resolve(
                ApprovalRule::All,
                &[a],
                &[],
                &m(&[(b, Cancel), (a, Approve)])
            ),
            Outcome::Approved {
                with_reservations: false
            }
        );
    }

    #[test]
    fn analisar_passes_its_verified_items_with_l2_fmea_and_ishikawa() {
        use crate::tools::{self, Tool};
        let mut t = T::new(Approach::Dmaic);
        let tg = t.tollgate(2);
        let verified_ok = |t: &T| {
            evaluate(&t.s, t.p, &tg)
                .into_iter()
                .filter(|r| r.item.item_type == ItemType::Verified)
                .all(|r| r.ok)
        };
        assert!(!verified_ok(&t));
        for (tool, kind, name) in [
            (Tool::Fmea, tools::fmea::ITEM, "FMEA do processo atual"),
            (Tool::Ishikawa, tools::ishikawa::ITEM, "Ishikawa do refugo"),
        ] {
            let d = t.id();
            let e = tools::Deliverable::create(
                t.p,
                tools::default_phase(&t.s, t.p, tool),
                tool,
                d,
                name,
            );
            t.commit(e);
            let row = t.id();
            let e = tools::new_item(&t.s, t.p, d, kind, row, None, vec![]);
            t.commit(e);
        }
        assert!(
            verified_ok(&t),
            "the L2 FMEA and Ishikawa satisfy the Analisar checks"
        );
        // With the manual items marked, the tollgate can be submitted.
        t.mark_all_manual(tg.id);
        assert!(submit(&t.s, t.p, tg.id, today()).is_ok());
    }
}
