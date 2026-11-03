//! Certification of the belts (`docs/plano_lean_seis_sigma.md`, 8.5; decisions MB4 and
//! MB5): the requirements engine, the registry with sequential numbers, the validity of
//! three years and the renewal.
//!
//! - **Requirements** of fixed kinds per level, with the company's parameters (entity
//!   `requisito_certificacao` of the program journal); [`default_requirements`] is the
//!   table of decision MB4. [`check`] confers each one on the data of the program and
//!   says `atende`, `não atende` or `ambíguo` with the reason, keeping the evidence (the id
//!   and the hash of the item that proves it). Savings count only when `validado`; a
//!   saving estimated by simulation never counts.
//! - The **registry** (`certificacao`): sequential number of the program, person, level,
//!   date, validity, the requirements met with their evidence and the issuer (an MBB).
//!   The MBB certification is recorded by hand ([`record_manual`]).
//! - **Validity and renewal** (MB5): a warning 90 days before the end; the renewal is
//!   conferred by the company's rules (a closed project, a green belt coached or hours of
//!   training given in the period); an expired certification shows as such.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::designacao::Assignment;
use crate::digest;
use crate::domain::{ProjectStatus, SavingStatus, enum_value};
use crate::formacao::{self, ClassSession, Nomination, NominationStatus, exams};
use crate::id::Id;
use crate::mentoria::{CompetenceMap, Level as SkillLevel};
use crate::op::{Edit, EntityRef, Kind};
use crate::programa::{self, BeltLevel, IdeaRecord, ProjectRecord};
use crate::savings::Category;
use crate::state::{Entity, State};

pub mod fields {
    pub const PROGRAM: &str = "programa";
    pub const LEVEL: &str = "nivel";
    pub const KIND: &str = "tipo";
    pub const COUNT: &str = "quantidade";
    pub const VALUE: &str = "valor";
    pub const TOOLS: &str = "ferramentas";
    pub const ACTIVE: &str = "ativo";
    // Certification.
    pub const NUMBER: &str = "numero";
    pub const PERSON: &str = "pessoa";
    pub const DATE: &str = "data";
    pub const VALID_UNTIL: &str = "valida_ate";
    pub const MET: &str = "requisitos_atendidos";
    pub const ISSUER: &str = "emissor";
    pub const MANUAL: &str = "manual";
    pub const NOTE: &str = "observacao";
    pub const RENEWAL_OF: &str = "renovacao_de";
    pub const REVOKED: &str = "revogada";
    /// The MBB's opinion on a candidate (entity `certificacao` before the issue).
    pub const OPINION: &str = "parecer";
    pub const OPINION_BY: &str = "parecer_por";
    pub const OPINION_ON: &str = "parecer_em";
    // Renewal rules of the program (fields of the `programa`).
    pub const RENEW_PROJECTS: &str = "renovacao_projetos";
    pub const RENEW_COACHED: &str = "renovacao_mentorias";
    pub const RENEW_HOURS: &str = "renovacao_horas_formacao";
}

/// Years of validity of a certification.
pub const VALIDITY_YEARS: i32 = 3;
/// Days before the end when the renewal warning shows.
pub const WARNING_DAYS: i64 = 90;
/// Default hours of training given that renew a certification.
pub const DEFAULT_RENEW_HOURS: f64 = 16.0;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CertError {
    #[error("os requisitos do nível não estão todos atendidos")]
    NotMet,
    #[error("só um MBB emite ou registra certificação")]
    NotMbb,
    #[error("escolha a pessoa")]
    NoPerson,
    #[error("a certificação a renovar já foi renovada ou está revogada")]
    NotRenewable,
    #[error("as regras de renovação da empresa não estão atendidas no período")]
    RenewalNotMet,
    #[error("escreva o parecer")]
    NoOpinion,
}

// --- Requirements --------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum RequirementKind {
    /// The training of the level concluded with the minimum attendance.
    #[default]
    #[serde(rename = "formacao")]
    Training,
    /// The exam passed.
    #[serde(rename = "prova")]
    Exam,
    /// `count` closed projects with every tollgate approved, as a member (yellow) or as
    /// the leader.
    #[serde(rename = "projetos_participacao")]
    ProjectsJoined,
    #[serde(rename = "projetos_lider")]
    ProjectsLed,
    /// Validated savings of the projects led ≥ `value`.
    #[serde(rename = "ganho_validado")]
    Savings,
    /// At least `count` tools at `aplicou` or `demonstrou` (any), or every tool of the list.
    #[serde(rename = "ferramentas")]
    Tools,
    /// `count` green belts coached until their project closed.
    #[serde(rename = "mentoria")]
    Coaching,
    /// The MBB's opinion and signature.
    #[serde(rename = "parecer_mbb")]
    MbbOpinion,
}

impl RequirementKind {
    pub const ALL: [RequirementKind; 8] = [
        RequirementKind::Training,
        RequirementKind::Exam,
        RequirementKind::ProjectsJoined,
        RequirementKind::ProjectsLed,
        RequirementKind::Savings,
        RequirementKind::Tools,
        RequirementKind::Coaching,
        RequirementKind::MbbOpinion,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RequirementKind::Training => "Formação do nível concluída com frequência mínima",
            RequirementKind::Exam => "Prova aprovada",
            RequirementKind::ProjectsJoined => {
                "Projetos encerrados com todos os tollgates aprovados (participação)"
            }
            RequirementKind::ProjectsLed => {
                "Projetos encerrados com todos os tollgates aprovados (como líder)"
            }
            RequirementKind::Savings => "Ganho validado pela controladoria nos projetos liderados",
            RequirementKind::Tools => "Ferramentas no nível aplicou ou demonstrou",
            RequirementKind::Coaching => {
                "Mentoria dada (green belts acompanhados até o encerramento)"
            }
            RequirementKind::MbbOpinion => "Parecer e assinatura do MBB",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    pub id: Id,
    pub level: BeltLevel,
    pub kind: RequirementKind,
    pub count: u32,
    pub value: f64,
    /// For [`RequirementKind::Tools`]: the minimum list (empty: any `count` tools).
    pub tools: Vec<String>,
    pub active: bool,
}

impl Requirement {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            level: e.typed(fields::LEVEL).unwrap_or_default(),
            kind: e.typed(fields::KIND).unwrap_or_default(),
            count: e
                .integer(fields::COUNT)
                .map_or(1, |n| n.clamp(0, 100) as u32),
            value: e.number(fields::VALUE).unwrap_or(0.0).max(0.0),
            tools: formacao::strings_in(e, fields::TOOLS),
            active: e
                .get(fields::ACTIVE)
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::CertificationRequirement, id)
    }
}

/// The requirements of a level in the program (the defaults of MB4 while the program has
/// none of the level).
pub fn requirements(state: &State, program: Id, level: BeltLevel) -> Vec<Requirement> {
    let key = program.to_string();
    let mut v: Vec<Requirement> = state
        .of_kind(Kind::CertificationRequirement)
        .filter(|(_, e)| !e.is_deleted() && e.text(fields::PROGRAM) == key)
        .map(|(id, e)| Requirement::read(id, e))
        .filter(|r| r.level == level)
        .collect();
    if v.is_empty() {
        return default_requirements(level)
            .into_iter()
            .enumerate()
            .map(|(i, (kind, count, value, tools))| Requirement {
                id: Id::from_bytes([
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0x70,
                    0,
                    0x80,
                    0,
                    0,
                    0,
                    0,
                    0,
                    level as u8,
                    i as u8,
                ]),
                level,
                kind,
                count,
                value,
                tools: tools.iter().map(|s| s.to_string()).collect(),
                active: true,
            })
            .collect();
    }
    v.retain(|r| r.active);
    v.sort_by_key(|r| (r.kind, r.id));
    v
}

/// The minimum tool list of green belt (deliverables a green belt applies in a DMAIC).
pub const GREEN_TOOLS: [&str; 5] = [
    "sipoc",
    "plano_coleta",
    "ishikawa",
    "fmea",
    "plano_controle",
];
/// The minimum tool list of black belt.
pub const BLACK_TOOLS: [&str; 8] = [
    "voc_ctq",
    "sipoc",
    "plano_coleta",
    "ishikawa",
    "matriz_ce",
    "fmea",
    "plano_controle",
    "plano_resposta",
];

/// The default table of decision MB4: `(kind, count, value, tools)`. The savings value is
/// the company's; the default asks for any validated saving (> 0).
pub fn default_requirements(
    level: BeltLevel,
) -> Vec<(RequirementKind, u32, f64, &'static [&'static str])> {
    use RequirementKind::*;
    match level {
        BeltLevel::Yellow => vec![
            (Training, 1, 0.0, &[]),
            (Exam, 1, 0.0, &[]),
            (ProjectsJoined, 1, 0.0, &[]),
            (Tools, 3, 0.0, &[]),
        ],
        BeltLevel::Green => vec![
            (Training, 1, 0.0, &[]),
            (Exam, 1, 0.0, &[]),
            (ProjectsLed, 1, 0.0, &[]),
            (Savings, 1, 0.0, &[]),
            (Tools, 0, 0.0, &GREEN_TOOLS),
            (MbbOpinion, 1, 0.0, &[]),
        ],
        BeltLevel::Black => vec![
            (Training, 1, 0.0, &[]),
            (Exam, 1, 0.0, &[]),
            (ProjectsLed, 2, 0.0, &[]),
            (Savings, 1, 0.0, &[]),
            (Tools, 0, 0.0, &BLACK_TOOLS),
            (Coaching, 1, 0.0, &[]),
            (MbbOpinion, 1, 0.0, &[]),
        ],
        // Recorded by hand (MB4).
        BeltLevel::MasterBlack => vec![],
    }
}

/// Edits that write the default table of a level in the program (so the company edits it).
pub fn install_defaults(program: Id, level: BeltLevel, mut ids: impl FnMut() -> Id) -> Vec<Edit> {
    let mut e = Vec::new();
    for (kind, count, value, tools) in default_requirements(level) {
        e.extend(set_requirement(
            program,
            ids(),
            level,
            kind,
            count,
            value,
            &tools.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        ));
    }
    e
}

pub fn set_requirement(
    program: Id,
    id: Id,
    level: BeltLevel,
    kind: RequirementKind,
    count: u32,
    value: f64,
    tools: &[String],
) -> Vec<Edit> {
    let r = Requirement::entity_ref(id);
    vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::LEVEL, enum_value(level)),
        Edit::new(r, fields::KIND, enum_value(kind)),
        Edit::new(r, fields::COUNT, count),
        Edit::new(r, fields::VALUE, value.max(0.0)),
        Edit::new(
            r,
            fields::TOOLS,
            Value::from(
                tools
                    .iter()
                    .map(|t| Value::from(t.as_str()))
                    .collect::<Vec<_>>(),
            ),
        ),
        Edit::new(r, fields::ACTIVE, true),
    ]
}

// --- The check ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Status {
    #[serde(rename = "atende")]
    Met,
    #[serde(rename = "nao_atende")]
    NotMet,
    /// The data point both ways (a saving only `previsto`, a tollgate changed after its
    /// approval): the MBB looks at it.
    #[serde(rename = "ambiguo")]
    Ambiguous,
}

/// The item that proves a requirement: its id and the hash of its content.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceRef {
    #[serde(rename = "id")]
    pub id: Id,
    #[serde(rename = "hash")]
    pub hash: String,
    #[serde(rename = "descricao")]
    pub text: String,
}

/// Hash of the canonical JSON of an evidence's content.
pub fn evidence_hash(content: &[(&str, Value)]) -> String {
    let m: BTreeMap<String, Value> = content
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    digest::hash_map(&m)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub requirement: Id,
    pub kind: RequirementKind,
    pub status: Status,
    /// Why, in words for the screen.
    pub reason: String,
    pub evidence: Vec<EvidenceRef>,
}

/// What the check reads (the program's data; the grades come from the grades folder).
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    pub classes: &'a [formacao::Class],
    pub sessions: &'a [ClassSession],
    pub nominations: &'a [Nomination],
    pub exams: &'a [exams::Exam],
    pub results: &'a [exams::ExamResult],
    pub projects: &'a [ProjectRecord],
    pub ideas: &'a [IdeaRecord],
    pub assignments: &'a [Assignment],
    pub map: &'a CompetenceMap,
    /// Opinions recorded by an MBB: `(person, level) → (certification entity, date)`.
    pub opinions: &'a BTreeMap<(Id, BeltLevel), (Id, Option<Date>)>,
}

fn money(x: f64) -> String {
    format!("R$ {:.2}", x).replace('.', ",")
}

/// A closed real project with every tollgate approved; `Ambiguous` when a tollgate changed
/// after its approval.
fn closed_status(p: &ProjectRecord) -> Option<Status> {
    if p.training || p.status != ProjectStatus::Closed {
        return None;
    }
    if p.tollgates.is_empty() || p.tollgates.iter().any(|t| t.approved_on.is_none()) {
        return None;
    }
    if p.tollgates.iter().any(|t| t.changed_after_approval) {
        return Some(Status::Ambiguous);
    }
    Some(Status::Met)
}

fn project_evidence(p: &ProjectRecord) -> EvidenceRef {
    let hashes: Vec<Value> = p
        .tollgates
        .iter()
        .map(|t| Value::from(t.frozen_hash.clone()))
        .collect();
    EvidenceRef {
        id: p.id,
        hash: evidence_hash(&[
            ("projeto", Value::from(p.id.to_string())),
            ("tollgates", Value::from(hashes)),
            (
                "encerrado_em",
                Value::from(p.closed_on.map(|d| d.to_string()).unwrap_or_default()),
            ),
        ]),
        text: p.title.clone(),
    }
}

/// Confers each requirement of `level` for `person`.
pub fn check(reqs: &[Requirement], person: Id, level: BeltLevel, ctx: &Context<'_>) -> Vec<Check> {
    reqs.iter()
        .filter(|r| r.level == level && r.active)
        .map(|r| check_one(r, person, ctx))
        .collect()
}

fn check_one(r: &Requirement, person: Id, ctx: &Context<'_>) -> Check {
    let mut c = Check {
        requirement: r.id,
        kind: r.kind,
        status: Status::NotMet,
        reason: String::new(),
        evidence: Vec::new(),
    };
    // The classes of the level where the person was selected.
    let my_classes: Vec<&formacao::Class> = ctx
        .classes
        .iter()
        .filter(|k| k.level == r.level)
        .filter(|k| formacao::students(ctx.nominations, k.id).contains(&person))
        .collect();
    match r.kind {
        RequirementKind::Training => {
            let best = my_classes
                .iter()
                .map(|k| (k, formacao::completion(k, ctx.sessions, person)))
                .max_by(|a, b| a.1.rate.total_cmp(&b.1.rate));
            match best {
                Some((k, comp)) if comp.ok => {
                    c.status = Status::Met;
                    c.reason = format!(
                        "frequência de {:.0} % na turma {} (mínimo {:.0} %)",
                        comp.rate * 100.0,
                        k.name,
                        comp.min * 100.0
                    );
                    c.evidence.push(EvidenceRef {
                        id: k.id,
                        hash: evidence_hash(&[
                            ("turma", Value::from(k.id.to_string())),
                            ("pessoa", Value::from(person.to_string())),
                            ("frequencia", Value::from(comp.rate)),
                            ("horas", Value::from(comp.hours_total)),
                        ]),
                        text: k.name.clone(),
                    });
                }
                Some((k, comp)) => {
                    c.reason = if comp.hours_total == 0.0 {
                        format!("a turma {} ainda não tem sessões com presença", k.name)
                    } else {
                        format!(
                            "frequência de {:.0} % na turma {}, abaixo do mínimo de {:.0} %",
                            comp.rate * 100.0,
                            k.name,
                            comp.min * 100.0
                        )
                    };
                }
                None => c.reason = "não foi selecionada em turma do nível".into(),
            }
        }
        RequirementKind::Exam => {
            let mut found = None;
            for k in &my_classes {
                let s = exams::standing(ctx.exams, ctx.results, k.id, person);
                if s.passed {
                    found = Some((k, s));
                    break;
                }
                if found.is_none() {
                    found = Some((k, s));
                }
            }
            match found {
                Some((k, s)) if s.passed => {
                    c.status = Status::Met;
                    c.reason = format!(
                        "aprovada na turma {} com {:.0} %",
                        k.name,
                        s.best.unwrap_or(0.0) * 100.0
                    );
                    if let Some(rid) = s.evidence {
                        if let Some(res) = ctx.results.iter().find(|x| x.id == rid) {
                            c.evidence.push(EvidenceRef {
                                id: rid,
                                hash: evidence_hash(&[
                                    (
                                        "prova",
                                        Value::from(
                                            res.exam.map(|e| e.to_string()).unwrap_or_default(),
                                        ),
                                    ),
                                    ("pessoa", Value::from(person.to_string())),
                                    ("nota", Value::from(res.score)),
                                ]),
                                text: format!("nota {:.0} %", res.score * 100.0),
                            });
                        }
                    }
                }
                Some((_, s)) if s.attempts > 0 => {
                    c.reason = format!(
                        "melhor nota {:.0} %, abaixo da mínima{}",
                        s.best.unwrap_or(0.0) * 100.0,
                        if s.may_retake {
                            "; cabe uma reaplicação"
                        } else {
                            ""
                        }
                    );
                }
                _ => c.reason = "sem prova corrigida".into(),
            }
        }
        RequirementKind::ProjectsJoined | RequirementKind::ProjectsLed => {
            let led = r.kind == RequirementKind::ProjectsLed;
            let mine: Vec<(&ProjectRecord, Status)> = ctx
                .projects
                .iter()
                .filter(|p| {
                    if led {
                        p.belts.contains(&person)
                    } else {
                        p.members.contains(&person) || p.belts.contains(&person)
                    }
                })
                .filter_map(|p| closed_status(p).map(|s| (p, s)))
                .collect();
            let ok = mine.iter().filter(|(_, s)| *s == Status::Met).count() as u32;
            let amb = mine.iter().filter(|(_, s)| *s == Status::Ambiguous).count() as u32;
            for (p, _) in &mine {
                c.evidence.push(project_evidence(p));
            }
            let what = if led {
                "como líder"
            } else {
                "com participação"
            };
            if ok >= r.count {
                c.status = Status::Met;
                c.reason = format!("{ok} projeto(s) encerrado(s) {what} (pedido: {})", r.count);
            } else if ok + amb >= r.count {
                c.status = Status::Ambiguous;
                c.reason = format!(
                    "{amb} projeto(s) com entregáveis alterados depois da aprovação do tollgate; o MBB confere"
                );
            } else {
                c.reason = format!(
                    "{ok} projeto(s) encerrado(s) {what} com todos os tollgates aprovados; pedido: {}",
                    r.count
                );
            }
        }
        RequirementKind::Savings => {
            let led: Vec<&ProjectRecord> = ctx
                .projects
                .iter()
                .filter(|p| !p.training && p.belts.contains(&person))
                .collect();
            let sum = |st: SavingStatus| -> f64 {
                led.iter()
                    .map(|p| {
                        [Category::Hard, Category::Soft]
                            .iter()
                            .map(|cat| p.totals.by.get(&(*cat, st)).copied().unwrap_or(0.0))
                            .sum::<f64>()
                    })
                    .sum()
            };
            let validated = sum(SavingStatus::Validated);
            let pending = sum(SavingStatus::Forecast) + sum(SavingStatus::Realized);
            let need_ok = |x: f64| {
                if r.value > 0.0 {
                    x + 1e-9 >= r.value
                } else {
                    x > 0.0
                }
            };
            for p in &led {
                let v: f64 = [Category::Hard, Category::Soft]
                    .iter()
                    .map(|cat| {
                        p.totals
                            .by
                            .get(&(*cat, SavingStatus::Validated))
                            .copied()
                            .unwrap_or(0.0)
                    })
                    .sum();
                if v > 0.0 {
                    c.evidence.push(EvidenceRef {
                        id: p.id,
                        hash: evidence_hash(&[
                            ("projeto", Value::from(p.id.to_string())),
                            ("validado", Value::from(v)),
                        ]),
                        text: format!("{}: {}", p.title, money(v)),
                    });
                }
            }
            let target = if r.value > 0.0 {
                money(r.value)
            } else {
                "algum valor".into()
            };
            if need_ok(validated) {
                c.status = Status::Met;
                c.reason = format!("{} validado (pedido: {target})", money(validated));
            } else if need_ok(validated + pending) {
                c.status = Status::Ambiguous;
                c.reason = format!(
                    "{} validado e {} ainda previsto ou realizado sem validação: só o validado conta",
                    money(validated),
                    money(pending)
                );
            } else {
                c.reason = format!("{} validado; pedido: {target}", money(validated));
            }
        }
        RequirementKind::Tools => {
            let have: Vec<String> = ctx
                .map
                .cells
                .iter()
                .filter(|((p, _), cell)| *p == person && cell.level >= Some(SkillLevel::Applied))
                .map(|((_, t), _)| t.clone())
                .collect();
            for t in &have {
                if let Some(cell) = ctx.map.cells.get(&(person, t.clone())) {
                    c.evidence.push(EvidenceRef {
                        id: person,
                        hash: evidence_hash(&[
                            ("ferramenta", Value::from(t.as_str())),
                            ("nivel", enum_value(cell.level.unwrap_or_default())),
                            ("evidencias", Value::from(cell.evidence.len())),
                        ]),
                        text: t.clone(),
                    });
                }
            }
            if r.tools.is_empty() {
                let n = have.len() as u32;
                if n >= r.count {
                    c.status = Status::Met;
                    c.reason = format!(
                        "{n} ferramenta(s) aplicadas ou demonstradas (pedido: {})",
                        r.count
                    );
                } else {
                    c.reason = format!(
                        "{n} ferramenta(s) aplicadas ou demonstradas; pedido: {}",
                        r.count
                    );
                }
            } else {
                let missing: Vec<&String> = r.tools.iter().filter(|t| !have.contains(t)).collect();
                if missing.is_empty() {
                    c.status = Status::Met;
                    c.reason =
                        "todas as ferramentas da lista mínima aplicadas ou demonstradas".into();
                } else {
                    c.reason = format!(
                        "faltam: {}",
                        missing
                            .iter()
                            .map(|t| t.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
        RequirementKind::Coaching => {
            let done: Vec<&ProjectRecord> = coached_to_close(person, ctx);
            for p in &done {
                c.evidence.push(project_evidence(p));
            }
            let n = done.len() as u32;
            if n >= r.count {
                c.status = Status::Met;
                c.reason = format!("{n} green belt(s) acompanhado(s) até o encerramento");
            } else {
                c.reason = format!(
                    "{n} green belt(s) acompanhado(s) até o encerramento; pedido: {}",
                    r.count
                );
            }
        }
        RequirementKind::MbbOpinion => match ctx.opinions.get(&(person, r.level)) {
            Some((id, date)) => {
                c.status = Status::Met;
                c.reason = format!(
                    "parecer do MBB{}",
                    date.map(|d| format!(" em {}", d.format_br()))
                        .unwrap_or_default()
                );
                c.evidence.push(EvidenceRef {
                    id: *id,
                    hash: evidence_hash(&[("parecer", Value::from(id.to_string()))]),
                    text: "parecer".into(),
                });
            }
            None => c.reason = "falta o parecer do MBB".into(),
        },
    }
    c
}

/// The closed projects of green belts the person coached (the assignment's mentor).
fn coached_to_close<'a>(person: Id, ctx: &Context<'a>) -> Vec<&'a ProjectRecord> {
    let mut v: Vec<&ProjectRecord> = ctx
        .assignments
        .iter()
        .filter(|a| a.mentor == Some(person))
        .filter_map(|a| {
            let idea = a.idea.and_then(|i| ctx.ideas.iter().find(|x| x.id == i))?;
            let p = idea
                .project
                .and_then(|p| ctx.projects.iter().find(|r| r.id == p))?;
            (closed_status(p) == Some(Status::Met)).then_some(p)
        })
        .collect();
    v.sort_by_key(|p| p.id);
    v.dedup_by_key(|p| p.id);
    v
}

/// Every requirement met.
pub fn all_met(checks: &[Check]) -> bool {
    !checks.is_empty() && checks.iter().all(|c| c.status == Status::Met)
}

// --- The registry ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetRequirement {
    #[serde(rename = "tipo")]
    pub kind: RequirementKind,
    #[serde(rename = "motivo")]
    pub reason: String,
    #[serde(rename = "evidencias")]
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Certification {
    pub id: Id,
    /// `None` while it is only the MBB's opinion on a candidate.
    pub number: Option<u32>,
    pub person: Option<Id>,
    pub level: BeltLevel,
    pub date: Option<Date>,
    pub valid_until: Option<Date>,
    pub met: Vec<MetRequirement>,
    pub issuer: Option<Id>,
    pub manual: bool,
    pub note: String,
    pub renewal_of: Option<Id>,
    pub revoked: bool,
    pub opinion: String,
    pub opinion_by: Option<Id>,
    pub opinion_on: Option<Date>,
}

impl Certification {
    pub fn read(id: Id, e: &Entity) -> Self {
        let d = |f| e.opt_text(f).and_then(|s| s.parse::<Date>().ok());
        Self {
            id,
            number: e
                .integer(fields::NUMBER)
                .filter(|n| *n > 0)
                .map(|n| n as u32),
            person: e.id(fields::PERSON),
            level: e.typed(fields::LEVEL).unwrap_or_default(),
            date: d(fields::DATE),
            valid_until: d(fields::VALID_UNTIL),
            met: e.typed(fields::MET).unwrap_or_default(),
            issuer: e.id(fields::ISSUER),
            manual: e.boolean(fields::MANUAL),
            note: e.text(fields::NOTE),
            renewal_of: e.id(fields::RENEWAL_OF),
            revoked: e.boolean(fields::REVOKED),
            opinion: e.text(fields::OPINION),
            opinion_by: e.id(fields::OPINION_BY),
            opinion_on: d(fields::OPINION_ON),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Certification, id)
    }

    pub fn issued(&self) -> bool {
        self.number.is_some() && !self.revoked
    }
}

pub fn certifications(state: &State, program: Id) -> Vec<Certification> {
    let key = program.to_string();
    let mut v: Vec<Certification> = state
        .of_kind(Kind::Certification)
        .filter(|(_, e)| !e.is_deleted() && e.text(fields::PROGRAM) == key)
        .map(|(id, e)| Certification::read(id, e))
        .collect();
    v.sort_by_key(|c| (c.number.unwrap_or(u32::MAX), c.id));
    v
}

/// The MBB opinions on candidates, for [`Context::opinions`]: only those written by an MBB.
pub fn opinions(
    certs: &[Certification],
    is_mbb: impl Fn(Id) -> bool,
) -> BTreeMap<(Id, BeltLevel), (Id, Option<Date>)> {
    certs
        .iter()
        .filter(|c| !c.opinion.trim().is_empty() && c.opinion_by.is_some_and(&is_mbb))
        .filter_map(|c| Some(((c.person?, c.level), (c.id, c.opinion_on))))
        .collect()
}

/// Edits of the MBB's opinion on a candidate (a new `certificacao` entity without number,
/// or the existing one).
pub fn record_opinion(
    program: Id,
    id: Id,
    person: Id,
    level: BeltLevel,
    text: &str,
    by: Id,
    date: Date,
) -> Result<Vec<Edit>, CertError> {
    if text.trim().is_empty() {
        return Err(CertError::NoOpinion);
    }
    let r = Certification::entity_ref(id);
    Ok(vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::PERSON, person.to_string()),
        Edit::new(r, fields::LEVEL, enum_value(level)),
        Edit::new(r, fields::OPINION, text.trim()),
        Edit::new(r, fields::OPINION_BY, by.to_string()),
        Edit::new(r, fields::OPINION_ON, date.to_string()),
    ])
}

/// The next sequential number of the program.
pub fn next_number(certs: &[Certification]) -> u32 {
    certs.iter().filter_map(|c| c.number).max().unwrap_or(0) + 1
}

/// The end of the validity: three years after `from`, the day before the anniversary
/// (29 February goes to 28 February).
pub fn valid_until(from: Date) -> Date {
    let y = from.year + VALIDITY_YEARS;
    let d = Date::new(y, from.month, from.day)
        .unwrap_or_else(|| Date::new(y, from.month, 28).expect("valid day"));
    d.add_days(-1)
}

fn person_edits(person: Id, level: BeltLevel, date: Date) -> Vec<Edit> {
    let r = EntityRef::new(Kind::Person, person);
    vec![
        Edit::new(r, programa::fields::LEVEL, enum_value(level)),
        Edit::new(r, programa::fields::CERTIFIED_ON, date.to_string()),
        Edit::new(r, programa::fields::NEEDS_CERTIFICATION, false),
    ]
}

/// Issues a certification when every requirement is met (`entity`: the opinion's entity,
/// when there is one, keeps the number). The person's level and date follow.
#[allow(clippy::too_many_arguments)]
pub fn issue(
    program: Id,
    entity: Id,
    person: Id,
    level: BeltLevel,
    checks: &[Check],
    certs: &[Certification],
    by: Id,
    by_is_mbb: bool,
    date: Date,
) -> Result<Vec<Edit>, CertError> {
    if !by_is_mbb {
        return Err(CertError::NotMbb);
    }
    if !all_met(checks) {
        return Err(CertError::NotMet);
    }
    let met: Vec<MetRequirement> = checks
        .iter()
        .map(|c| MetRequirement {
            kind: c.kind,
            reason: c.reason.clone(),
            evidence: c.evidence.clone(),
        })
        .collect();
    let mut e = registry_edits(
        program,
        entity,
        person,
        level,
        next_number(certs),
        &met,
        by,
        date,
    );
    e.extend(person_edits(person, level, date));
    Ok(e)
}

#[allow(clippy::too_many_arguments)]
fn registry_edits(
    program: Id,
    entity: Id,
    person: Id,
    level: BeltLevel,
    number: u32,
    met: &[MetRequirement],
    by: Id,
    date: Date,
) -> Vec<Edit> {
    let r = Certification::entity_ref(entity);
    vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::PERSON, person.to_string()),
        Edit::new(r, fields::LEVEL, enum_value(level)),
        Edit::new(r, fields::NUMBER, number),
        Edit::new(r, fields::DATE, date.to_string()),
        Edit::new(r, fields::VALID_UNTIL, valid_until(date).to_string()),
        Edit::new(r, fields::MET, serde_json::to_value(met).expect("met")),
        Edit::new(r, fields::ISSUER, by.to_string()),
    ]
}

/// Records by hand a certification the app does not confer: the MBB level, or one earned
/// before the program (MB4). The note says on what it rests.
#[allow(clippy::too_many_arguments)]
pub fn record_manual(
    program: Id,
    id: Id,
    person: Id,
    level: BeltLevel,
    note: &str,
    certs: &[Certification],
    by: Id,
    by_is_mbb: bool,
    date: Date,
) -> Result<Vec<Edit>, CertError> {
    if !by_is_mbb {
        return Err(CertError::NotMbb);
    }
    if note.trim().is_empty() {
        return Err(CertError::NoOpinion);
    }
    let mut e = registry_edits(
        program,
        id,
        person,
        level,
        next_number(certs),
        &[],
        by,
        date,
    );
    let r = Certification::entity_ref(id);
    e.push(Edit::new(r, fields::MANUAL, true));
    e.push(Edit::new(r, fields::NOTE, note.trim()));
    e.extend(person_edits(person, level, date));
    Ok(e)
}

pub fn revoke(cert: &Certification, reason: &str) -> Result<Vec<Edit>, CertError> {
    if reason.trim().is_empty() {
        return Err(CertError::NoOpinion);
    }
    let r = Certification::entity_ref(cert.id);
    Ok(vec![
        Edit::new(r, fields::REVOKED, true),
        Edit::new(r, fields::NOTE, reason.trim()),
    ])
}

// --- Validity ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Validity {
    Valid,
    /// Within 90 days of the end: `days` left.
    Expiring {
        days: i64,
    },
    Expired,
    Revoked,
    /// Renewed by a later certification.
    Renewed,
}

pub fn validity(cert: &Certification, certs: &[Certification], today: Date) -> Validity {
    if cert.revoked {
        return Validity::Revoked;
    }
    if certs
        .iter()
        .any(|c| c.renewal_of == Some(cert.id) && c.issued())
    {
        return Validity::Renewed;
    }
    match cert.valid_until {
        Some(u) if today > u => Validity::Expired,
        Some(u) if today.days_until(u) <= WARNING_DAYS => Validity::Expiring {
            days: today.days_until(u),
        },
        _ => Validity::Valid,
    }
}

/// The current certification of a person: the highest level still valid or about to
/// expire, else the last one (expired).
pub fn current(
    certs: &[Certification],
    person: Id,
    today: Date,
) -> Option<(&Certification, Validity)> {
    let mine: Vec<(&Certification, Validity)> = certs
        .iter()
        .filter(|c| c.person == Some(person) && c.issued())
        .map(|c| (c, validity(c, certs, today)))
        .filter(|(_, v)| *v != Validity::Renewed)
        .collect();
    let live = mine
        .iter()
        .filter(|(_, v)| matches!(v, Validity::Valid | Validity::Expiring { .. }))
        .max_by_key(|(c, _)| (c.level, c.date));
    live.or_else(|| mine.iter().max_by_key(|(c, _)| (c.date, c.level)))
        .copied()
}

/// Certifications of the program expiring within 90 days or expired, for the warnings.
pub fn to_renew(certs: &[Certification], today: Date) -> Vec<(&Certification, Validity)> {
    certs
        .iter()
        .filter(|c| c.issued())
        .map(|c| (c, validity(c, certs, today)))
        .filter(|(_, v)| matches!(v, Validity::Expiring { .. } | Validity::Expired))
        .collect()
}

/// The renewal rules of the company: any one of them in the validity period renews.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenewalRules {
    pub projects: u32,
    pub coached: u32,
    pub hours: f64,
}

impl RenewalRules {
    pub fn read(state: &State, program: Id) -> Self {
        let e = state.entity(program);
        let n = |f: &str, d: u32| {
            e.and_then(|e| e.integer(f))
                .map_or(d, |x| x.clamp(0, 100) as u32)
        };
        Self {
            projects: n(fields::RENEW_PROJECTS, 1),
            coached: n(fields::RENEW_COACHED, 1),
            hours: e
                .and_then(|e| e.number(fields::RENEW_HOURS))
                .unwrap_or(DEFAULT_RENEW_HOURS)
                .max(0.0),
        }
    }
}

/// What renews a certification in its period, by the rules.
#[derive(Debug, Clone, PartialEq)]
pub struct RenewalCheck {
    pub projects: Vec<Id>,
    pub coached: Vec<Id>,
    pub hours: f64,
    pub ok: bool,
    pub reason: String,
}

pub fn renewal_check(
    cert: &Certification,
    rules: &RenewalRules,
    ctx: &Context<'_>,
) -> RenewalCheck {
    let (Some(person), Some(from), Some(to)) = (cert.person, cert.date, cert.valid_until) else {
        return RenewalCheck {
            projects: vec![],
            coached: vec![],
            hours: 0.0,
            ok: false,
            reason: "certificação sem pessoa ou datas".into(),
        };
    };
    let in_period = |d: Option<Date>| d.is_some_and(|d| d >= from && d <= to);
    let projects: Vec<Id> = ctx
        .projects
        .iter()
        .filter(|p| p.belts.contains(&person) && in_period(p.closed_on))
        .filter(|p| closed_status(p) == Some(Status::Met))
        .map(|p| p.id)
        .collect();
    let coached: Vec<Id> = coached_to_close(person, ctx)
        .into_iter()
        .filter(|p| in_period(p.closed_on))
        .map(|p| p.id)
        .collect();
    let hours = formacao::hours_given(ctx.sessions, person, from, to);
    let ok = (rules.projects > 0 && projects.len() as u32 >= rules.projects)
        || (rules.coached > 0 && coached.len() as u32 >= rules.coached)
        || (rules.hours > 0.0 && hours + 1e-9 >= rules.hours);
    let reason = format!(
        "no período: {} projeto(s) encerrado(s) (regra: {}), {} mentoria(s) até o encerramento (regra: {}), {:.1} h de formação dadas (regra: {:.1} h)",
        projects.len(),
        rules.projects,
        coached.len(),
        rules.coached,
        hours,
        rules.hours
    );
    RenewalCheck {
        projects,
        coached,
        hours,
        ok,
        reason,
    }
}

/// Renews: a new certification with the next number, valid three years from the day after
/// the old one ends (or from today, if it already expired).
#[allow(clippy::too_many_arguments)]
pub fn renew(
    program: Id,
    id: Id,
    old: &Certification,
    check: &RenewalCheck,
    certs: &[Certification],
    by: Id,
    by_is_mbb: bool,
    today: Date,
) -> Result<Vec<Edit>, CertError> {
    if !by_is_mbb {
        return Err(CertError::NotMbb);
    }
    if !old.issued()
        || certs
            .iter()
            .any(|c| c.renewal_of == Some(old.id) && c.issued())
    {
        return Err(CertError::NotRenewable);
    }
    if !check.ok {
        return Err(CertError::RenewalNotMet);
    }
    let person = old.person.ok_or(CertError::NoPerson)?;
    let start = match old.valid_until {
        Some(u) if u >= today => u.add_days(1),
        _ => today,
    };
    let met = vec![MetRequirement {
        kind: RequirementKind::MbbOpinion,
        reason: format!("renovação: {}", check.reason),
        evidence: check
            .projects
            .iter()
            .chain(check.coached.iter())
            .map(|p| EvidenceRef {
                id: *p,
                hash: evidence_hash(&[("projeto", Value::from(p.to_string()))]),
                text: "projeto".into(),
            })
            .collect(),
    }];
    let mut e = registry_edits(
        program,
        id,
        person,
        old.level,
        next_number(certs),
        &met,
        by,
        start,
    );
    e.push(Edit::new(
        Certification::entity_ref(id),
        fields::RENEWAL_OF,
        old.id.to_string(),
    ));
    Ok(e)
}

// --- The certification funnel ------------------------------------------------------------------

/// The stages of the certification funnel (section 8.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    Nominated,
    ManagerApproved,
    Selected,
    Trained,
    WithProject,
    Certified,
}

impl Stage {
    pub const ALL: [Stage; 6] = [
        Stage::Nominated,
        Stage::ManagerApproved,
        Stage::Selected,
        Stage::Trained,
        Stage::WithProject,
        Stage::Certified,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Stage::Nominated => "Indicados",
            Stage::ManagerApproved => "Aprovados pelo gestor",
            Stage::Selected => "Selecionados",
            Stage::Trained => "Formados",
            Stage::WithProject => "Com projeto",
            Stage::Certified => "Certificados",
        }
    }
}

/// The funnel of a wave and level: how many reached each stage and why the others left.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Funnel {
    pub counts: BTreeMap<Stage, usize>,
    /// `(stage they did not pass, reason)` → people.
    pub exits: BTreeMap<(Stage, String), usize>,
}

pub fn funnel(
    wave: Option<Id>,
    level: BeltLevel,
    ctx: &Context<'_>,
    certs: &[Certification],
) -> Funnel {
    let mut f = Funnel::default();
    let mut add = |s: Stage| *f.counts.entry(s).or_default() += 1;
    let mut exits: BTreeMap<(Stage, String), usize> = BTreeMap::new();
    let mut exit = |s: Stage, r: &str| *exits.entry((s, r.to_string())).or_default() += 1;
    for n in ctx
        .nominations
        .iter()
        .filter(|n| n.level == level && (wave.is_none() || n.wave == wave))
    {
        let Some(p) = n.person else { continue };
        add(Stage::Nominated);
        if n.manager_approval != Some(true) {
            let why = match n.manager_approval {
                Some(false) => format!("recusado pelo gestor: {}", n.manager_reason),
                _ if n.status == NominationStatus::Withdrawn => {
                    format!("desistiu: {}", n.status_reason)
                }
                _ => "aguardando o gestor".into(),
            };
            exit(Stage::ManagerApproved, &why);
            continue;
        }
        add(Stage::ManagerApproved);
        if n.status != NominationStatus::Selected {
            let why = match n.status {
                NominationStatus::Waiting => "lista de espera".to_string(),
                NominationStatus::Refused => format!("não selecionado: {}", n.status_reason),
                NominationStatus::Withdrawn => format!("desistiu: {}", n.status_reason),
                _ => "aguardando a seleção".into(),
            };
            exit(Stage::Selected, &why);
            continue;
        }
        add(Stage::Selected);
        let class = n.class.and_then(|c| ctx.classes.iter().find(|k| k.id == c));
        let trained = class.is_some_and(|k| formacao::completion(k, ctx.sessions, p).ok);
        if !trained {
            exit(
                Stage::Trained,
                "frequência abaixo do mínimo ou turma em curso",
            );
            continue;
        }
        add(Stage::Trained);
        let has_project = ctx.projects.iter().any(|r| {
            !r.training
                && (r.belts.contains(&p) || (level == BeltLevel::Yellow && r.members.contains(&p)))
        }) || ctx.assignments.iter().any(|a| a.person == Some(p));
        if !has_project {
            exit(Stage::WithProject, "sem projeto de certificação");
            continue;
        }
        add(Stage::WithProject);
        let certified = certs
            .iter()
            .any(|c| c.person == Some(p) && c.level == level && c.issued());
        if certified {
            add(Stage::Certified);
        } else {
            exit(Stage::Certified, "requisitos em andamento");
        }
    }
    f.exits = exits;
    f
}

#[cfg(test)]
mod tests;
