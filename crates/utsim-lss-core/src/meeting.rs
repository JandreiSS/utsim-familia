//! Meetings and minutes (`docs/plano_lean_seis_sigma.md`, 1.8): the agenda generated from
//! the open items of the project, who was there, notes, decisions, and the actions written
//! in the minutes turned into cards with one click (linked to the meeting).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::board::{self, CardSpec};
use crate::date::Date;
use crate::domain::{Meeting, Origin, enum_value, fields as dfields, of_project};
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};
use crate::tollgate::{self, TollgateStatus};

pub mod fields {
    /// On the meeting: arrays of person ids.
    pub const PRESENT: &str = "presentes";
    pub const ABSENT: &str = "ausentes";
    pub const KIND: &str = "tipo";
    // Minutes item.
    pub const MEETING: &str = "reuniao";
    pub const TEXT: &str = "texto";
    pub const ITEM_KIND: &str = "tipo";
    pub const WHO: &str = "responsavel";
    pub const DUE: &str = "prazo";
    /// The card created from the item.
    pub const ACTION: &str = "acao";
    pub const ORDER: &str = "ordem";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MeetingKind {
    #[default]
    #[serde(rename = "equipe")]
    Team,
    #[serde(rename = "tollgate")]
    Tollgate,
    #[serde(rename = "carteira")]
    Portfolio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ItemKind {
    #[default]
    #[serde(rename = "anotacao")]
    Note,
    #[serde(rename = "decisao")]
    Decision,
    #[serde(rename = "acao")]
    Action,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MinutesItem {
    pub id: Id,
    pub meeting: Option<Id>,
    pub text: String,
    pub kind: ItemKind,
    pub who: Option<Id>,
    pub due: Option<Date>,
    pub action: Option<Id>,
    pub order: f64,
}

impl MinutesItem {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields as f;
        Self {
            id,
            meeting: e.id(f::MEETING),
            text: e.text(f::TEXT),
            kind: e.typed(f::ITEM_KIND).unwrap_or_default(),
            who: e.id(f::WHO),
            due: e.opt_text(f::DUE).and_then(|s| s.parse().ok()),
            action: e.id(f::ACTION),
            order: e.number(f::ORDER).unwrap_or(0.0),
        }
    }
}

/// Meetings of a project, latest date first.
pub fn meetings(state: &State, project: Id) -> Vec<Meeting> {
    let mut v = of_project(state, Kind::Meeting, project, Meeting::read);
    v.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
    v
}

pub fn items(state: &State, meeting: Id) -> Vec<MinutesItem> {
    let mut v: Vec<MinutesItem> = state
        .of_kind(Kind::MinutesItem)
        .filter(|(_, e)| e.id(fields::MEETING) == Some(meeting))
        .map(|(id, e)| MinutesItem::read(id, e))
        .collect();
    v.sort_by(|a, b| a.order.total_cmp(&b.order).then(a.id.cmp(&b.id)));
    v
}

/// People marked present.
pub fn present(state: &State, meeting: Id) -> Vec<Id> {
    id_list(state.entity(meeting), fields::PRESENT)
}

fn id_list(e: Option<&Entity>, field: &str) -> Vec<Id> {
    e.and_then(|e| e.get(field))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str()?.parse().ok()).collect())
        .unwrap_or_default()
}

/// Edit that marks a person present (or not).
pub fn set_present(state: &State, meeting: Id, person: Id, yes: bool) -> Option<Edit> {
    let mut v = present(state, meeting);
    let had = v.contains(&person);
    if had == yes {
        return None;
    }
    if yes {
        v.push(person);
        v.sort();
    } else {
        v.retain(|p| *p != person);
    }
    let arr: Vec<Value> = v.iter().map(|i| Value::from(i.to_string())).collect();
    Some(Edit::new(
        EntityRef::new(Kind::Meeting, meeting),
        fields::PRESENT,
        Value::from(arr),
    ))
}

/// Edits that create a meeting.
pub fn new_meeting(project: Id, id: Id, title: &str, date: Date, kind: MeetingKind) -> Vec<Edit> {
    use dfields::meeting as f;
    let r = EntityRef::new(Kind::Meeting, id);
    vec![
        Edit::new(r, f::PROJECT, project.to_string()),
        Edit::new(r, f::TITLE, title.trim()),
        Edit::new(r, f::DATE, date.to_string()),
        Edit::new(r, fields::KIND, enum_value(kind)),
    ]
}

/// Edits that add an item to the minutes, at the end.
pub fn add_item(state: &State, meeting: Id, id: Id, kind: ItemKind, text: &str) -> Vec<Edit> {
    let order = items(state, meeting)
        .iter()
        .map(|i| i.order)
        .fold(0.0, f64::max)
        + 1.0;
    let r = EntityRef::new(Kind::MinutesItem, id);
    vec![
        Edit::new(r, fields::MEETING, meeting.to_string()),
        Edit::new(r, fields::ITEM_KIND, enum_value(kind)),
        Edit::new(r, fields::TEXT, text.trim()),
        Edit::new(r, fields::ORDER, order),
    ]
}

/// What the agenda brings up.
#[derive(Debug, Clone, PartialEq)]
pub enum AgendaItem {
    /// An action past its due date.
    OverdueAction { card: Id, title: String, due: Date },
    /// An action due within the next 7 days.
    ActionDueSoon { card: Id, title: String, due: Date },
    /// An open high risk.
    HighRisk {
        risk: Id,
        description: String,
        level: u8,
    },
    /// A risk whose review date has come.
    RiskReview { risk: Id, description: String },
    /// A tollgate waiting for the approvers.
    TollgateInReview { tollgate: Id, phase: String },
    /// An action item of an earlier meeting not turned into a card yet.
    PendingMinutesAction { item: Id, text: String },
    /// Conflicts to review.
    Conflicts(usize),
}

/// The agenda of a meeting on `day`, from the open items of the project.
pub fn agenda(state: &State, project: Id, day: Date) -> Vec<AgendaItem> {
    use crate::domain::{Action, Column, RiskStatus};
    let mut v = Vec::new();
    let mut cards = of_project(state, Kind::Action, project, Action::read);
    cards.sort_by_key(|a| (a.due, a.id));
    for a in cards.iter().filter(|a| a.column != Column::Done) {
        let Some(due) = a.due else { continue };
        if due < day {
            v.push(AgendaItem::OverdueAction {
                card: a.id,
                title: a.title.clone(),
                due,
            });
        } else if due <= day.add_days(7) {
            v.push(AgendaItem::ActionDueSoon {
                card: a.id,
                title: a.title.clone(),
                due,
            });
        }
    }
    let risks = crate::risk::risks(state, project);
    for r in risks
        .iter()
        .filter(|r| r.status == RiskStatus::Open && r.is_high())
    {
        v.push(AgendaItem::HighRisk {
            risk: r.id,
            description: r.description.clone(),
            level: r.level(),
        });
    }
    for r in crate::risk::due_for_review(state, project, day) {
        if !r.is_high() {
            v.push(AgendaItem::RiskReview {
                risk: r.id,
                description: r.description,
            });
        }
    }
    for ph in crate::domain::phases(state, project) {
        if let Some(tg) = tollgate::of_phase(state, ph.id) {
            if tg.status == TollgateStatus::Submitted {
                v.push(AgendaItem::TollgateInReview {
                    tollgate: tg.id,
                    phase: ph.name.clone(),
                });
            }
        }
    }
    for m in meetings(state, project)
        .iter()
        .filter(|m| m.date.is_some_and(|d| d < day))
    {
        for i in items(state, m.id) {
            if i.kind == ItemKind::Action && i.action.is_none() {
                v.push(AgendaItem::PendingMinutesAction {
                    item: i.id,
                    text: i.text,
                });
            }
        }
    }
    if !state.conflicts.is_empty() {
        v.push(AgendaItem::Conflicts(state.conflicts.len()));
    }
    v
}

/// Edits that turn every action item of the minutes not yet converted into a card on the
/// board (title, owner and due date carried, origin "ata", linked to the meeting), and
/// record the card in the item. Running it again creates nothing.
pub fn actions_to_cards(
    state: &State,
    project: Id,
    meeting: Id,
    author: Id,
    mut ids: impl FnMut() -> Id,
) -> Vec<Edit> {
    let link = EntityRef::new(Kind::Meeting, meeting);
    let mut local = state.clone();
    let mut edits = Vec::new();
    for item in items(state, meeting) {
        if item.kind != ItemKind::Action || item.action.is_some() || item.text.trim().is_empty() {
            continue;
        }
        let card = ids();
        let spec = CardSpec {
            title: item.text.clone(),
            who: item.who,
            due: item.due,
            ..CardSpec::default()
        };
        let mut e = board::linked_card(&local, project, card, &spec, Origin::Minutes, link);
        e.push(Edit::new(
            EntityRef::new(Kind::MinutesItem, item.id),
            fields::ACTION,
            card.to_string(),
        ));
        for op in local.prepare(author, 0, e.clone(), &mut ids) {
            let _ = local.apply(&op);
        }
        edits.extend(e);
    }
    edits
}

/// The text of a generated agenda, for the meeting's "pauta" field. `line` formats each
/// item (the screen's catalogue).
pub fn agenda_text(items: &[AgendaItem], line: impl Fn(&AgendaItem) -> String) -> String {
    items
        .iter()
        .map(|i| format!("- {}", line(i)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Approach, Column, Project};
    use crate::kanban;

    #[test]
    fn agenda_and_minutes_to_cards() {
        let mut n = 0u64;
        let mut ids = || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        };
        let (me, p) = (ids(), ids());
        let mut s = State::replay(&State::new().prepare(
            me,
            0,
            Project::create(p, "P", Approach::Dmaic, &mut ids),
            &mut ids,
        ));
        let mut counter = 1000u64;
        let mut next = move || {
            counter += 1;
            Id::v7(counter, [(counter % 251) as u8; 10])
        };
        let commit = |s: &mut State, e: Vec<Edit>, next: &mut dyn FnMut() -> Id| {
            for op in s.prepare(me, 0, e, next) {
                s.apply(&op).unwrap();
            }
        };
        let day = Date::new(2026, 11, 10).unwrap();
        // One overdue card, one due soon, one far, one done.
        for (t, due, done) in [
            ("atrasada", -2, false),
            ("logo", 3, false),
            ("longe", 30, false),
            ("feita", -5, true),
        ] {
            let id = next();
            let mut e =
                kanban::new_card(&s, p, id, t, if done { Column::Done } else { Column::Todo });
            e.push(Edit::new(
                EntityRef::new(Kind::Action, id),
                dfields::action::DUE,
                day.add_days(due).to_string(),
            ));
            commit(&mut s, e, &mut next);
        }
        let rid = next();
        commit(
            &mut s,
            crate::risk::new_risk(p, rid, "Greve", 4, 4),
            &mut next,
        );
        let low = next();
        let mut e = crate::risk::new_risk(p, low, "Chuva", 1, 2);
        e.push(Edit::new(
            EntityRef::new(Kind::Risk, low),
            crate::risk::fields::REVIEW,
            day.to_string(),
        ));
        commit(&mut s, e, &mut next);

        // An earlier meeting with an action item not converted.
        let m0 = next();
        commit(
            &mut s,
            new_meeting(p, m0, "Semanal 1", day.add_days(-7), MeetingKind::Team),
            &mut next,
        );
        let it0 = next();
        let e = add_item(&s, m0, it0, ItemKind::Action, "Pedir dados ao PCP");
        commit(&mut s, e, &mut next);

        let a = agenda(&s, p, day);
        assert!(matches!(a[0], AgendaItem::OverdueAction { ref title, .. } if title == "atrasada"));
        assert!(matches!(a[1], AgendaItem::ActionDueSoon { ref title, .. } if title == "logo"));
        assert!(matches!(a[2], AgendaItem::HighRisk { level: 16, .. }));
        assert!(matches!(a[3], AgendaItem::RiskReview { .. }));
        assert!(matches!(a[4], AgendaItem::PendingMinutesAction { .. }));
        assert_eq!(a.len(), 5);
        let txt = agenda_text(&a, |_| "x".into());
        assert_eq!(txt.lines().count(), 5);

        // This week's meeting: attendance and minutes.
        let m = next();
        commit(
            &mut s,
            new_meeting(p, m, "Semanal 2", day, MeetingKind::Team),
            &mut next,
        );
        let e = set_present(&s, m, me, true).unwrap();
        commit(&mut s, vec![e], &mut next);
        assert!(set_present(&s, m, me, true).is_none());
        assert_eq!(present(&s, m), vec![me]);
        for (k, t) in [
            (ItemKind::Decision, "Seguir com o piloto"),
            (ItemKind::Action, "Treinar o turno B"),
            (ItemKind::Action, "Comprar gabarito"),
        ] {
            let id = next();
            let e = add_item(&s, m, id, k, t);
            commit(&mut s, e, &mut next);
        }
        let first_action = items(&s, m)[1].id;
        commit(
            &mut s,
            vec![Edit::new(
                EntityRef::new(Kind::MinutesItem, first_action),
                fields::WHO,
                me.to_string(),
            )],
            &mut next,
        );
        let e = actions_to_cards(&s, p, m, me, &mut next);
        commit(&mut s, e, &mut next);
        let cards = board::cards_linked_to(&s, p, m);
        assert_eq!(cards.len(), 2);
        assert!(cards.iter().all(|c| c.origin == Origin::Minutes));
        assert!(
            cards
                .iter()
                .any(|c| c.title == "Treinar o turno B" && c.who == Some(me))
        );
        assert!(
            items(&s, m)
                .iter()
                .filter(|i| i.kind == ItemKind::Action)
                .all(|i| i.action.is_some())
        );
        // Idempotent.
        assert!(actions_to_cards(&s, p, m, me, &mut next).is_empty());
        let titles: Vec<String> = kanban::board(&s, p)[&Column::Todo]
            .iter()
            .map(|c| c.title.clone())
            .collect();
        assert_eq!(
            &titles[titles.len() - 2..],
            ["Treinar o turno B", "Comprar gabarito"]
        );
        assert_eq!(meetings(&s, p)[0].id, m, "latest first");
        let e = set_present(&s, m, me, false).unwrap();
        commit(&mut s, vec![e], &mut next);
        assert!(present(&s, m).is_empty());
    }
}
