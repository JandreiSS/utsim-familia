//! The quick kaizen event in full (`docs/plano_lean_seis_sigma.md`, 1.5, etapa P6): the
//! pre-event checklist (scope, data, team, logistics), the agenda of the 3 to 5 days (one
//! theme per slot: understand, measure, analyse, improve, standardise, present), the list
//! of open items sent to the action board (origin kaizen) and the follow-up at 30, 60 and
//! 90 days after the event.

use serde_json::Value;

use super::{ItemRef, item_ref, items, new_item};
use crate::date::Date;
use crate::domain::{Action, Column, Origin, enum_value, fields as df};
use crate::id::Id;
use crate::kanban;
use crate::op::Edit;
use crate::state::State;

pub const PREP: &str = "preparo";
pub const SLOT: &str = "pauta";
pub const OPEN_ITEM: &str = "pendencia";

pub mod fields {
    /// On the deliverable: first day of the event (`AAAA-MM-DD`) and number of days.
    pub const START: &str = "inicio";
    pub const DAYS: &str = "dias";
    /// On the deliverable: what was seen at each follow-up (`revisao_30`...).
    pub fn review(days: u16) -> String {
        format!("revisao_{days}")
    }
    pub const TEXT: &str = "texto";
    pub const DONE: &str = "feito";
    pub const DAY: &str = "dia";
    pub const THEME: &str = "tema";
    pub const WHO: &str = "responsavel";
    pub const DUE: &str = "prazo";
    /// The action card made from an open item.
    pub const CARD: &str = "cartao";
}

/// The themes of the event, in order.
pub const THEMES: [&str; 6] = [
    "Entender o processo e o problema",
    "Medir a situação atual",
    "Analisar as causas",
    "Melhorar: testar as mudanças no posto",
    "Padronizar o que funcionou",
    "Apresentar os resultados à liderança",
];

/// The pre-event checklist.
pub const PREP_ITEMS: [&str; 4] = [
    "Escopo e meta do evento acordados com o patrocinador",
    "Dados da situação atual levantados",
    "Equipe convidada e liberada das tarefas nos dias do evento",
    "Logística: sala, material, acesso à área e às máquinas",
];

/// The follow-ups after the event, in days.
pub const FOLLOW_UPS: [u16; 3] = [30, 60, 90];

/// Allowed event lengths.
pub const DAYS: std::ops::RangeInclusive<u8> = 3..=5;

/// The day (1 based) of each theme in an event of `days` days: the six themes spread in
/// order (3 days: two per day).
pub fn agenda(days: u8) -> [u8; 6] {
    let n = days.clamp(*DAYS.start(), *DAYS.end()) as usize;
    std::array::from_fn(|i| (i * n / THEMES.len()) as u8 + 1)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub start: Option<Date>,
    pub days: u8,
    /// The last day of the event.
    pub end: Option<Date>,
}

pub fn event(state: &State, del: Id) -> Event {
    let e = state.entity(del);
    let start = e.and_then(|e| e.text(fields::START).parse().ok());
    let days = e
        .and_then(|e| e.integer(fields::DAYS))
        .and_then(|d| u8::try_from(d).ok())
        .filter(|d| DAYS.contains(d))
        .unwrap_or(*DAYS.end());
    Event {
        start,
        days,
        end: start.map(|s: Date| s.add_days(i64::from(days) - 1)),
    }
}

/// The checklist items or agenda slots that are not there yet (by text), as new items.
fn fill(
    state: &State,
    project: Id,
    del: Id,
    kind: &str,
    rows: Vec<(String, Option<u8>)>,
    new_id: &mut impl FnMut() -> Id,
) -> Vec<Edit> {
    let have: Vec<String> = items(state, del, kind)
        .iter()
        .map(|i| i.text(fields::TEXT).trim().to_lowercase())
        .collect();
    let mut out = Vec::new();
    let mut n = 0.0;
    for (text, day) in rows {
        if have.contains(&text.to_lowercase()) {
            continue;
        }
        let mut extra = vec![(fields::TEXT, Value::from(text))];
        if let Some(d) = day {
            extra.push((fields::DAY, Value::from(d)));
        }
        let mut e = new_item(state, project, del, kind, new_id(), None, extra);
        if let Some(o) = e.iter_mut().find(|e| e.field == super::fields::item::ORDER) {
            o.value = Value::from(o.value.as_f64().unwrap_or(1.0) + n);
        }
        n += 1.0;
        out.extend(e);
    }
    out
}

/// The pre-event checklist and the agenda of the event's length, without repeating what is
/// already there.
pub fn template_edits(
    state: &State,
    project: Id,
    del: Id,
    mut new_id: impl FnMut() -> Id,
) -> Vec<Edit> {
    let ev = event(state, del);
    let days = agenda(ev.days);
    let mut out = fill(
        state,
        project,
        del,
        PREP,
        PREP_ITEMS.iter().map(|t| (t.to_string(), None)).collect(),
        &mut new_id,
    );
    out.extend(fill(
        state,
        project,
        del,
        SLOT,
        THEMES
            .iter()
            .zip(days)
            .map(|(t, d)| (t.to_string(), Some(d)))
            .collect(),
        &mut new_id,
    ));
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenItem {
    pub id: Id,
    pub text: String,
    pub who: String,
    pub due: Option<Date>,
    pub card: Option<Id>,
    /// Done on the item or its card in the "Feito" column.
    pub done: bool,
}

pub fn open_items(state: &State, del: Id) -> Vec<OpenItem> {
    items(state, del, OPEN_ITEM)
        .iter()
        .map(|i| {
            let card = i
                .entity
                .id(fields::CARD)
                .filter(|c| state.entity(*c).is_some_and(|e| !e.is_deleted()));
            let card_done = card
                .and_then(|c| state.entity(c).map(|e| Action::read(c, e)))
                .is_some_and(|a| a.column == Column::Done);
            OpenItem {
                id: i.id,
                text: i.text(fields::TEXT),
                who: i.text(fields::WHO),
                due: i.text(fields::DUE).parse().ok(),
                card,
                done: i.entity.boolean(fields::DONE) || card_done,
            }
        })
        .collect()
}

/// The card of an open item on the action board (origin kaizen, linked to the item).
pub fn card_edits(state: &State, project: Id, item: &ItemRef, card: Id) -> Vec<Edit> {
    let mut edits = kanban::new_card(state, project, card, &item.text(fields::TEXT), Column::Todo);
    edits.retain(|e| e.field != df::action::ORIGIN);
    let c = Action::entity_ref(card);
    edits.push(Edit::new(c, df::action::ORIGIN, enum_value(Origin::Kaizen)));
    edits.push(Edit::new(c, df::action::LINK, item.id.to_string()));
    if let Ok(d) = item.text(fields::DUE).parse::<Date>() {
        edits.push(Edit::new(c, df::action::DUE, d.to_string()));
    }
    let who = item.text(fields::WHO);
    if !who.trim().is_empty() {
        edits.push(Edit::new(c, df::action::HOW, format!("Responsável: {who}")));
    }
    edits.push(Edit::new(item_ref(item.id), fields::CARD, card.to_string()));
    edits
}

/// One follow-up after the event.
#[derive(Debug, Clone, PartialEq)]
pub struct FollowUp {
    pub days: u16,
    pub date: Option<Date>,
    /// The date has come.
    pub due: bool,
    /// Something was written about it.
    pub noted: bool,
    /// Open items due by this date and not done, once the date has come.
    pub late: usize,
}

pub fn follow_ups(state: &State, del: Id, today: Date) -> Vec<FollowUp> {
    let ev = event(state, del);
    let opens = open_items(state, del);
    let e = state.entity(del);
    FOLLOW_UPS
        .iter()
        .map(|d| {
            let date = ev.end.map(|end| end.add_days(i64::from(*d)));
            let due = date.is_some_and(|x| x <= today);
            FollowUp {
                days: *d,
                date,
                due,
                noted: e.is_some_and(|e| !e.text(&fields::review(*d)).trim().is_empty()),
                late: opens
                    .iter()
                    .filter(|_| due)
                    .filter(|o| !o.done)
                    .filter(|o| match (o.due, date) {
                        (Some(due), Some(at)) => due <= at && due < today,
                        _ => false,
                    })
                    .count(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    #[test]
    fn agenda_spreads_the_themes() {
        assert_eq!(agenda(3), [1, 1, 2, 2, 3, 3]);
        assert_eq!(agenda(4), [1, 1, 2, 3, 3, 4]);
        assert_eq!(agenda(5), [1, 1, 2, 3, 4, 5]);
        assert_eq!(agenda(9), agenda(5), "clamped");
    }

    #[test]
    fn template_cards_and_follow_ups() {
        let mut fx = Fixture::new();
        let d = fx.deliverable(Tool::KaizenEvent);
        fx.set(d, fields::START, "2026-11-16");
        fx.set(d, fields::DAYS, 3);
        let mut n = 9000;
        let e = template_edits(&fx.state, fx.project, d, || {
            n += 1;
            Id::v7(n, [3; 10])
        });
        fx.commit(e);
        assert_eq!(items(&fx.state, d, PREP).len(), 4);
        let slots = items(&fx.state, d, SLOT);
        assert_eq!(slots.len(), 6);
        assert_eq!(slots[5].entity.integer(fields::DAY), Some(3));
        assert!(
            template_edits(&fx.state, fx.project, d, || Id::v7(1, [4; 10])).is_empty(),
            "nothing repeated"
        );
        let ev = event(&fx.state, d);
        assert_eq!(ev.end, "2026-11-18".parse().ok());

        let a = fx.item(
            d,
            OPEN_ITEM,
            None,
            vec![
                (fields::TEXT, "Pintar as faixas".into()),
                (fields::WHO, "Caio".into()),
                (fields::DUE, "2026-12-01".into()),
            ],
        );
        fx.item(
            d,
            OPEN_ITEM,
            None,
            vec![
                (fields::TEXT, "Treinar o segundo turno".into()),
                (fields::DUE, "2027-02-01".into()),
            ],
        );
        let card = fx.id();
        let e = {
            let ent = fx.state.entity(a).unwrap();
            card_edits(&fx.state, fx.project, &ItemRef { id: a, entity: ent }, card)
        };
        fx.commit(e);
        let c = Action::read(card, fx.state.entity(card).unwrap());
        assert_eq!(c.origin, Origin::Kaizen);
        assert_eq!(c.link, Some(a));
        assert_eq!(c.due, "2026-12-01".parse().ok());
        let opens = open_items(&fx.state, d);
        assert_eq!(opens[0].card, Some(card));
        assert!(!opens[0].done);

        // On 2026-12-20: the 30-day follow-up (2026-12-18) is due with one late item.
        let today: Date = "2026-12-20".parse().unwrap();
        let f = follow_ups(&fx.state, d, today);
        assert_eq!(f[0].date, "2026-12-18".parse().ok());
        assert!(f[0].due && !f[0].noted);
        assert_eq!(f[0].late, 1);
        assert!(!f[1].due);
        assert_eq!(f[1].late, 0, "not counted before its date");
        // The card done closes the item.
        let e = kanban::move_card(&fx.state, fx.project, card, Column::Done, 0);
        fx.commit(e);
        fx.set(
            d,
            fields::review(30).as_str(),
            "Faixas pintadas; falta o treino",
        );
        let f = follow_ups(&fx.state, d, today);
        assert_eq!(f[0].late, 0);
        assert!(f[0].noted);
    }
}
