//! Tollgate reviews across the program (`docs/plano_lean_seis_sigma.md`, 8.4).
//!
//! - **Queue**: every tollgate of the program waiting for the MBB's opinion (submitted,
//!   current round not reviewed yet), with planned date, days late, unit, belt and phase.
//!   Order: late ones first (most days late first), then the longest waiting, then the
//!   planned date, then the title.
//! - **Batch review**: the queue grouped by day or by unit; the review meeting writes the
//!   minutes with one decision line per project.
//! - **Standard work**: standard questions per phase, a target time per review, the
//!   verified checklist items shown already checked, and the **structured opinion**: data
//!   sufficient, adequate tool, correct statistics, supported conclusions and risks, each
//!   `ok`, `ressalva` or `refazer` with a comment (required when not `ok`).
//! - **Quality of the reviews**: per phase and per belt, the rate of `refazer`, the
//!   reservations that recur and the days between the request and the opinion.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::date::Date;
use crate::domain::enum_value;
use crate::id::Id;
use crate::meeting::{self, ItemKind, MeetingKind};
use crate::op::{Edit, EntityRef, Kind};
use crate::programa::ProjectRecord;
use crate::state::{Entity, State};
use crate::tollgate::TollgateStatus;

pub mod fields {
    pub const PROGRAM: &str = "programa";
    pub const PROJECT: &str = "projeto";
    pub const TOLLGATE: &str = "tollgate";
    pub const ROUND: &str = "rodada";
    pub const PHASE: &str = "fase";
    pub const UNIT: &str = "unidade";
    pub const BELT: &str = "belt";
    pub const REVIEWER: &str = "revisor";
    pub const REQUESTED_ON: &str = "pedido_em";
    pub const DATE: &str = "data";
    pub const MINUTES: &str = "duracao_min";
    pub const SUMMARY: &str = "conclusao";
    /// `item_<criterion>` and `comentario_<criterion>`.
    pub const MARK_PREFIX: &str = "item_";
    pub const COMMENT_PREFIX: &str = "comentario_";
    /// `resposta_<n>`: answer to the n-th standard question (true: yes).
    pub const ANSWER_PREFIX: &str = "resposta_";
    // Company question.
    pub const TEXT: &str = "texto";
    /// On the program: target minutes per review.
    pub const TARGET_MINUTES: &str = "tempo_alvo_revisao";
}

/// Default time for a review, minutes.
pub const DEFAULT_TARGET_MINUTES: f64 = 45.0;

/// A tollgate in the queue.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewItem {
    pub project: Id,
    pub title: String,
    pub unit: Option<Id>,
    pub belts: Vec<Id>,
    pub tollgate: Id,
    pub phase: String,
    pub phase_index: usize,
    pub round: i64,
    pub planned: Option<Date>,
    pub submitted_on: Option<Date>,
    pub days_waiting: i64,
    pub days_late: i64,
    pub verified: Vec<(String, bool)>,
}

impl ReviewItem {
    pub fn overdue(&self) -> bool {
        self.days_late > 0
    }
}

fn item(p: &ProjectRecord, t: &crate::programa::TollgateRecord, today: Date) -> ReviewItem {
    ReviewItem {
        project: p.id,
        title: p.title.clone(),
        unit: p.unit,
        belts: p.belts.clone(),
        tollgate: t.id,
        phase: t.phase_name.clone(),
        phase_index: t.phase_index,
        round: t.round,
        planned: t.planned,
        submitted_on: t.submitted_on,
        days_waiting: t.submitted_on.map_or(0, |s| s.days_until(today).max(0)),
        days_late: t.planned.map_or(0, |pl| pl.days_until(today).max(0)),
        verified: t.verified.clone(),
    }
}

fn sort(v: &mut [ReviewItem]) {
    v.sort_by(|a, b| {
        b.overdue()
            .cmp(&a.overdue())
            .then(b.days_late.cmp(&a.days_late))
            .then(b.days_waiting.cmp(&a.days_waiting))
            .then(a.planned.cmp(&b.planned))
            .then(a.title.cmp(&b.title))
            .then(a.tollgate.cmp(&b.tollgate))
    });
}

/// The review queue of the program: submitted tollgates of real projects whose current
/// round has no opinion of the MBB yet.
pub fn queue(projects: &[ProjectRecord], reviews: &[Review], today: Date) -> Vec<ReviewItem> {
    let mut v: Vec<ReviewItem> = projects
        .iter()
        .filter(|p| !p.training)
        .flat_map(|p| {
            p.tollgates
                .iter()
                .filter(|t| t.status == TollgateStatus::Submitted)
                .filter(|t| {
                    !reviews
                        .iter()
                        .any(|r| r.tollgate == Some(t.id) && r.round == t.round)
                })
                .map(move |t| item(p, t, today))
        })
        .collect();
    sort(&mut v);
    v
}

/// Tollgates past their planned date and not approved (nor cancelled), of real projects.
pub fn overdue_tollgates(projects: &[ProjectRecord], today: Date) -> Vec<ReviewItem> {
    let mut v: Vec<ReviewItem> = projects
        .iter()
        .filter(|p| !p.training && p.is_running())
        .flat_map(|p| {
            p.tollgates
                .iter()
                .filter(|t| {
                    !matches!(
                        t.status,
                        TollgateStatus::Approved | TollgateStatus::Cancelled
                    )
                })
                .filter(|t| t.planned.is_some_and(|d| d < today))
                .map(move |t| item(p, t, today))
        })
        .collect();
    sort(&mut v);
    v
}

/// How the batch review groups the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupBy {
    /// By planned date (no date: one group at the end).
    #[default]
    Day,
    Unit,
}

/// A group of the batch review.
#[derive(Debug, Clone, PartialEq)]
pub enum BatchKey {
    Day(Option<Date>),
    Unit(Option<Id>),
}

/// The queue in groups, each keeping the queue order.
pub fn batches(items: &[ReviewItem], by: GroupBy) -> Vec<(BatchKey, Vec<ReviewItem>)> {
    let mut out: Vec<(BatchKey, Vec<ReviewItem>)> = Vec::new();
    for i in items {
        let key = match by {
            GroupBy::Day => BatchKey::Day(i.planned),
            GroupBy::Unit => BatchKey::Unit(i.unit),
        };
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(i.clone()),
            None => out.push((key, vec![i.clone()])),
        }
    }
    out.sort_by(|a, b| match (&a.0, &b.0) {
        (BatchKey::Day(x), BatchKey::Day(y)) => match (x, y) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        },
        (BatchKey::Unit(x), BatchKey::Unit(y)) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    });
    out
}

/// Standard questions of a phase (DMAIC and DMADV names; the other approaches get the
/// common ones). The company adds its own ([`company_questions`]).
pub fn standard_questions(phase: &str) -> &'static [&'static str] {
    match phase {
        "Definir" => &[
            "O problema e a meta estão em números, com prazo?",
            "O escopo dentro e fora está claro e foi aceito pelo patrocinador?",
            "O ganho estimado tem memória de cálculo?",
            "A voz do cliente foi ouvida e traduzida em CTQs?",
        ],
        "Medir" => &[
            "A definição operacional do Y é inequívoca?",
            "O MSA cobre o sistema de medição usado na linha de base?",
            "A linha de base usa dado de origem Real?",
            "A amostra e a estratificação permitem achar as causas?",
        ],
        "Analisar" => &[
            "As causas raiz foram confirmadas com dado, e não só por opinião?",
            "Os pressupostos dos testes foram conferidos?",
            "As causas descartadas têm o motivo registrado?",
        ],
        "Melhorar" | "Desenhar" => &[
            "As soluções atacam as causas confirmadas?",
            "O piloto mostrou a melhora com dado e com a mesma medição?",
            "Os riscos da solução foram avaliados (FMEA)?",
        ],
        "Controlar" | "Verificar" => &[
            "O plano de controle cobre os X críticos com reação definida?",
            "O dono do processo aceitou a transferência?",
            "O ganho realizado está a caminho da validação pela controladoria?",
        ],
        _ => &[
            "O entregável da fase responde ao problema do termo?",
            "As decisões da fase têm dado que as sustente?",
        ],
    }
}

/// Questions the company added for a phase.
pub fn company_questions(state: &State, phase: &str) -> Vec<(Id, String)> {
    let mut v: Vec<(Id, String)> = state
        .of_kind(Kind::ReviewQuestion)
        .filter(|(_, e)| e.text(fields::PHASE) == phase)
        .map(|(id, e)| (id, e.text(fields::TEXT)))
        .filter(|(_, t)| !t.trim().is_empty())
        .collect();
    v.sort();
    v
}

pub fn add_question(program: Id, id: Id, phase: &str, text: &str) -> Vec<Edit> {
    let r = EntityRef::new(Kind::ReviewQuestion, id);
    vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::PHASE, phase.trim()),
        Edit::new(r, fields::TEXT, text.trim()),
    ]
}

/// Target minutes per review (the program's, or the default).
pub fn target_minutes(state: &State, program: Id) -> f64 {
    state
        .entity(program)
        .and_then(|e| e.number(fields::TARGET_MINUTES))
        .filter(|m| *m > 0.0)
        .unwrap_or(DEFAULT_TARGET_MINUTES)
}

/// The items of the structured opinion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Criterion {
    #[serde(rename = "dado_suficiente")]
    Data,
    #[serde(rename = "ferramenta_adequada")]
    Tool,
    #[serde(rename = "estatistica_correta")]
    Statistics,
    #[serde(rename = "conclusoes_suportadas")]
    Conclusions,
    #[serde(rename = "riscos")]
    Risks,
}

impl Criterion {
    pub const ALL: [Criterion; 5] = [
        Criterion::Data,
        Criterion::Tool,
        Criterion::Statistics,
        Criterion::Conclusions,
        Criterion::Risks,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Criterion::Data => "dado_suficiente",
            Criterion::Tool => "ferramenta_adequada",
            Criterion::Statistics => "estatistica_correta",
            Criterion::Conclusions => "conclusoes_suportadas",
            Criterion::Risks => "riscos",
        }
    }

    pub fn mark_field(self) -> String {
        format!("{}{}", fields::MARK_PREFIX, self.key())
    }

    pub fn comment_field(self) -> String {
        format!("{}{}", fields::COMMENT_PREFIX, self.key())
    }
}

/// Mark of an item, from best to worst.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Mark {
    #[default]
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "ressalva")]
    Reservation,
    #[serde(rename = "refazer")]
    Redo,
}

impl Mark {
    pub const ALL: [Mark; 3] = [Mark::Ok, Mark::Reservation, Mark::Redo];
}

/// An opinion of the MBB on a round of a tollgate.
#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    pub id: Id,
    pub project: Option<Id>,
    pub tollgate: Option<Id>,
    pub round: i64,
    pub phase: String,
    pub unit: Option<Id>,
    pub belt: Option<Id>,
    pub reviewer: Option<Id>,
    pub requested_on: Option<Date>,
    pub date: Option<Date>,
    pub minutes: f64,
    pub marks: BTreeMap<Criterion, (Mark, String)>,
    pub answers: BTreeMap<usize, bool>,
    pub summary: String,
}

impl Review {
    pub fn read(id: Id, e: &Entity) -> Self {
        let d = |f| e.opt_text(f).and_then(|s| s.parse::<Date>().ok());
        let marks = Criterion::ALL
            .iter()
            .filter_map(|c| {
                let m: Mark = e.typed(&c.mark_field())?;
                Some((*c, (m, e.text(&c.comment_field()))))
            })
            .collect();
        let answers = e
            .fields
            .iter()
            .filter_map(|(k, v)| {
                let n: usize = k.strip_prefix(fields::ANSWER_PREFIX)?.parse().ok()?;
                Some((n, v.value.as_bool()?))
            })
            .collect();
        Self {
            id,
            project: e.id(fields::PROJECT),
            tollgate: e.id(fields::TOLLGATE),
            round: e.integer(fields::ROUND).unwrap_or(0),
            phase: e.text(fields::PHASE),
            unit: e.id(fields::UNIT),
            belt: e.id(fields::BELT),
            reviewer: e.id(fields::REVIEWER),
            requested_on: d(fields::REQUESTED_ON),
            date: d(fields::DATE),
            minutes: e.number(fields::MINUTES).unwrap_or(0.0).max(0.0),
            marks,
            answers,
            summary: e.text(fields::SUMMARY),
        }
    }

    /// The worst mark (`None` while an item is missing).
    pub fn verdict(&self) -> Option<Mark> {
        if self.marks.len() < Criterion::ALL.len() {
            return None;
        }
        self.marks.values().map(|(m, _)| *m).max()
    }

    /// Days between the request (submission) and the opinion.
    pub fn lead_days(&self) -> Option<i64> {
        Some(self.requested_on?.days_until(self.date?).max(0))
    }
}

pub fn reviews(state: &State) -> Vec<Review> {
    let mut v: Vec<Review> = state
        .of_kind(Kind::TollgateReview)
        .map(|(id, e)| Review::read(id, e))
        .collect();
    v.sort_by_key(|r| (r.date, r.id));
    v
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReviewError {
    #[error("marque os cinco itens do parecer (ok, ressalva ou refazer)")]
    Incomplete,
    #[error("escreva o comentário dos itens com ressalva ou refazer")]
    NoComment,
}

/// What the MBB fills in a review.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReviewInput {
    pub marks: BTreeMap<Criterion, (Mark, String)>,
    pub answers: BTreeMap<usize, bool>,
    pub summary: String,
    pub minutes: f64,
}

/// Edits of the MBB's opinion on the current round of a queue item.
pub fn record(
    program: Id,
    id: Id,
    item: &ReviewItem,
    reviewer: Id,
    date: Date,
    input: &ReviewInput,
) -> Result<Vec<Edit>, ReviewError> {
    if Criterion::ALL.iter().any(|c| !input.marks.contains_key(c)) {
        return Err(ReviewError::Incomplete);
    }
    if input
        .marks
        .values()
        .any(|(m, c)| *m != Mark::Ok && c.trim().is_empty())
    {
        return Err(ReviewError::NoComment);
    }
    let r = EntityRef::new(Kind::TollgateReview, id);
    let mut e = vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::PROJECT, item.project.to_string()),
        Edit::new(r, fields::TOLLGATE, item.tollgate.to_string()),
        Edit::new(r, fields::ROUND, item.round),
        Edit::new(r, fields::PHASE, item.phase.clone()),
        Edit::new(r, fields::REVIEWER, reviewer.to_string()),
        Edit::new(r, fields::DATE, date.to_string()),
        Edit::new(r, fields::MINUTES, input.minutes.max(0.0)),
        Edit::new(r, fields::SUMMARY, input.summary.trim()),
    ];
    if let Some(u) = item.unit {
        e.push(Edit::new(r, fields::UNIT, u.to_string()));
    }
    if let Some(b) = item.belts.first() {
        e.push(Edit::new(r, fields::BELT, b.to_string()));
    }
    if let Some(s) = item.submitted_on {
        e.push(Edit::new(r, fields::REQUESTED_ON, s.to_string()));
    }
    for (c, (m, text)) in &input.marks {
        e.push(Edit::new(r, c.mark_field(), enum_value(*m)));
        e.push(Edit::new(r, c.comment_field(), text.trim()));
    }
    for (n, a) in &input.answers {
        e.push(Edit::new(
            r,
            format!("{}{n}", fields::ANSWER_PREFIX),
            Value::from(*a),
        ));
    }
    Ok(e)
}

/// The review meeting of a batch: a program meeting and one decision line per reviewed
/// project (`(title, phase, verdict)`).
pub fn batch_minutes(
    program: Id,
    meeting_id: Id,
    title: &str,
    date: Date,
    lines: &[(String, String, Option<Mark>)],
    mut ids: impl FnMut() -> Id,
) -> Vec<Edit> {
    let mut e = meeting::new_meeting(program, meeting_id, title, date, MeetingKind::Tollgate);
    // Order of the items: their position in the batch.
    for (i, (project, phase, verdict)) in lines.iter().enumerate() {
        let word = match verdict {
            Some(Mark::Ok) => "parecer ok",
            Some(Mark::Reservation) => "parecer com ressalvas",
            Some(Mark::Redo) => "refazer",
            None => "sem parecer",
        };
        let id = ids();
        let r = EntityRef::new(Kind::MinutesItem, id);
        e.push(Edit::new(
            r,
            meeting::fields::MEETING,
            meeting_id.to_string(),
        ));
        e.push(Edit::new(
            r,
            meeting::fields::ITEM_KIND,
            enum_value(ItemKind::Decision),
        ));
        e.push(Edit::new(
            r,
            meeting::fields::TEXT,
            format!("{project}, {phase}: {word}"),
        ));
        e.push(Edit::new(r, meeting::fields::ORDER, i as f64 + 1.0));
    }
    e
}

/// Indicators of a group of reviews.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stats {
    pub reviews: usize,
    pub redo: usize,
    pub reservations: usize,
    pub lead_days: Vec<i64>,
    pub minutes: Vec<f64>,
}

impl Stats {
    pub fn redo_rate(&self) -> f64 {
        if self.reviews == 0 {
            0.0
        } else {
            self.redo as f64 / self.reviews as f64
        }
    }

    /// Median days between the request and the opinion.
    pub fn median_lead(&self) -> Option<f64> {
        let v: Vec<f64> = self.lead_days.iter().map(|d| *d as f64).collect();
        crate::painel::percentile(&v, 0.5)
    }

    fn add(&mut self, r: &Review) {
        self.reviews += 1;
        match r.verdict() {
            Some(Mark::Redo) => self.redo += 1,
            Some(Mark::Reservation) => self.reservations += 1,
            _ => {}
        }
        if let Some(d) = r.lead_days() {
            self.lead_days.push(d);
        }
        self.minutes.push(r.minutes);
    }
}

/// The quality of the reviews.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Quality {
    pub by_phase: BTreeMap<String, Stats>,
    pub by_belt: BTreeMap<Id, Stats>,
    /// `(belt, criterion, times)`: an item not `ok` in two or more reviews of the belt.
    pub recurring: Vec<(Id, Criterion, usize)>,
    pub all: Stats,
}

pub fn quality(reviews: &[Review]) -> Quality {
    let mut q = Quality::default();
    let mut counts: BTreeMap<(Id, Criterion), usize> = BTreeMap::new();
    for r in reviews {
        q.all.add(r);
        q.by_phase.entry(r.phase.clone()).or_default().add(r);
        if let Some(b) = r.belt {
            q.by_belt.entry(b).or_default().add(r);
            for (c, (m, _)) in &r.marks {
                if *m != Mark::Ok {
                    *counts.entry((b, *c)).or_default() += 1;
                }
            }
        }
    }
    q.recurring = counts
        .into_iter()
        .filter(|(_, n)| *n >= 2)
        .map(|((b, c), n)| (b, c, n))
        .collect();
    q
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ProjectStatus;
    use crate::programa::TollgateRecord;
    use crate::programa::testkit::*;

    fn tg(
        n: u64,
        status: TollgateStatus,
        planned: Option<Date>,
        submitted: Option<Date>,
    ) -> TollgateRecord {
        TollgateRecord {
            id: Id::v7(n, [n as u8; 10]),
            phase: Id::v7(n + 1000, [1; 10]),
            phase_name: "Medir".into(),
            phase_index: 1,
            status,
            round: 1,
            planned,
            submitted_on: submitted,
            approved_on: None,
            verified: vec![("Existe MSA".into(), true)],
            frozen_hash: String::new(),
            changed_after_approval: false,
        }
    }

    fn input(m: Mark, comment: &str) -> ReviewInput {
        ReviewInput {
            marks: Criterion::ALL
                .iter()
                .map(|c| (*c, (m, comment.to_string())))
                .collect(),
            answers: BTreeMap::from([(0, true)]),
            summary: "segue".into(),
            minutes: 40.0,
        }
    }

    #[test]
    fn queue_order_overdue_first_then_waiting() {
        let today = d(2026, 11, 3);
        let u = Id::v7(77, [7; 10]);
        let mut a = project(Id::v7(1, [1; 10]), Some(u), ProjectStatus::Running);
        a.title = "A".into();
        a.tollgates = vec![
            tg(
                10,
                TollgateStatus::Submitted,
                Some(d(2026, 11, 10)),
                Some(d(2026, 10, 1)),
            ),
            tg(11, TollgateStatus::Open, Some(d(2026, 10, 1)), None),
        ];
        let mut b = project(Id::v7(2, [2; 10]), None, ProjectStatus::Running);
        b.title = "B".into();
        b.tollgates = vec![tg(
            20,
            TollgateStatus::Submitted,
            Some(d(2026, 10, 30)),
            Some(d(2026, 10, 29)),
        )];
        let mut c = project(Id::v7(3, [3; 10]), None, ProjectStatus::Running);
        c.title = "C".into();
        c.tollgates = vec![tg(
            30,
            TollgateStatus::Submitted,
            Some(d(2026, 10, 20)),
            Some(d(2026, 10, 30)),
        )];
        let mut tr = project(Id::v7(4, [4; 10]), None, ProjectStatus::Running);
        tr.training = true;
        tr.tollgates = vec![tg(
            40,
            TollgateStatus::Submitted,
            Some(d(2026, 1, 1)),
            Some(d(2026, 1, 1)),
        )];
        let ps = vec![a, b, c, tr];
        let q = queue(&ps, &[], today);
        let order: Vec<&str> = q.iter().map(|i| i.title.as_str()).collect();
        // C is 14 days late, B 4 days late, A not late (waiting 33 days); training out.
        assert_eq!(order, ["C", "B", "A"]);
        assert_eq!(q[0].days_late, 14);
        assert_eq!(q[2].days_waiting, 33);
        assert!(!q[2].overdue());
        let late = overdue_tollgates(&ps, today);
        assert_eq!(late.len(), 3, "A's open Medir and the two late submitted");
        // A review of the current round takes it out of the queue.
        let mut t = T::new();
        let id = t.id();
        let e = record(t.program, id, &q[0], t.me, today, &input(Mark::Ok, "")).unwrap();
        t.commit(e);
        let rs = reviews(&t.s);
        assert_eq!(queue(&ps, &rs, today).len(), 2);
        // Batches by day and unit keep the queue order inside.
        let by_day = batches(&q, GroupBy::Day);
        assert_eq!(by_day.len(), 3);
        assert_eq!(by_day[0].0, BatchKey::Day(Some(d(2026, 10, 20))));
        let by_unit = batches(&q, GroupBy::Unit);
        assert_eq!(by_unit.len(), 2);
        assert_eq!(
            by_unit[0]
                .1
                .iter()
                .map(|i| i.title.as_str())
                .collect::<Vec<_>>(),
            ["C", "B"]
        );
    }

    #[test]
    fn structured_opinion_and_quality() {
        let mut t = T::new();
        let p = t.program;
        let belt = t.id();
        let mut pr = project(t.id(), None, ProjectStatus::Running);
        pr.belts = vec![belt];
        pr.tollgates = vec![tg(5, TollgateStatus::Submitted, None, Some(d(2026, 10, 1)))];
        let q = queue(&[pr], &[], d(2026, 10, 8));
        let it = &q[0];
        let mut partial = input(Mark::Ok, "");
        partial.marks.remove(&Criterion::Risks);
        assert_eq!(
            record(p, t.id(), it, t.me, d(2026, 10, 8), &partial),
            Err(ReviewError::Incomplete)
        );
        assert_eq!(
            record(p, t.id(), it, t.me, d(2026, 10, 8), &input(Mark::Redo, " ")),
            Err(ReviewError::NoComment)
        );
        let mut mixed = input(Mark::Ok, "");
        mixed.marks.insert(
            Criterion::Statistics,
            (Mark::Redo, "teste t sem conferir a normalidade".into()),
        );
        mixed.marks.insert(
            Criterion::Data,
            (Mark::Reservation, "amostra pequena".into()),
        );
        let id = t.id();
        t.commit(record(p, id, it, t.me, d(2026, 10, 8), &mixed).unwrap());
        let mut second = it.clone();
        second.round = 2;
        second.submitted_on = Some(d(2026, 10, 20));
        let mut res = input(Mark::Ok, "");
        res.marks.insert(
            Criterion::Statistics,
            (Mark::Reservation, "intervalo de confiança".into()),
        );
        let id = t.id();
        t.commit(record(p, id, &second, t.me, d(2026, 10, 22), &res).unwrap());
        let rs = reviews(&t.s);
        assert_eq!(rs[0].verdict(), Some(Mark::Redo));
        assert_eq!(rs[1].verdict(), Some(Mark::Reservation));
        assert!(rs[0].answers[&0]);
        let ql = quality(&rs);
        assert_eq!(ql.all.reviews, 2);
        assert!((ql.by_phase["Medir"].redo_rate() - 0.5).abs() < 1e-12);
        assert_eq!(ql.by_belt[&belt].reservations, 1);
        assert_eq!(ql.recurring, vec![(belt, Criterion::Statistics, 2)]);
        assert_eq!(ql.all.median_lead(), Some(4.5));
        assert_eq!(standard_questions("Medir").len(), 4);
        assert!(!standard_questions("Evento").is_empty());
        let qid = t.id();
        t.commit(add_question(
            p,
            qid,
            "Medir",
            "O Gage R&R está abaixo de 10 %?",
        ));
        assert_eq!(company_questions(&t.s, "Medir").len(), 1);
        assert_eq!(target_minutes(&t.s, p), DEFAULT_TARGET_MINUTES);
        // Minutes of a batch: one decision per project.
        let m = t.id();
        let mut k = 5000;
        let e = batch_minutes(
            p,
            m,
            "Revisões de novembro",
            d(2026, 11, 3),
            &[
                ("A".into(), "Medir".into(), Some(Mark::Redo)),
                ("B".into(), "Definir".into(), None),
            ],
            || {
                k += 1;
                Id::v7(k, [8; 10])
            },
        );
        t.commit(e);
        let items = meeting::items(&t.s, m);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "A, Medir: refazer");
    }
}
