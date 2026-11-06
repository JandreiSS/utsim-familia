//! The plans (`docs/plano_lean_seis_sigma.md`, 2.2 and 2.5): data collection plan (Medir),
//! control plan and response plan (Controlar). Mostly structure; the response plan turns a
//! signal into a card on the board. The rules are in `utsim_ferramentas::planos` (decision
//! G3); this module reads the journal and writes the card.

use super::{ItemRef, items};
use crate::domain::{Action, Origin, enum_value, fields as df};
use crate::id::Id;
use crate::kanban;
use crate::op::Edit;
use crate::state::State;

pub use utsim_ferramentas::planos::{
    COLLECTION_ITEM, CONTROL_ITEM, CollectionIssue, DATA_TYPES, RESPONSE_ITEM, fields,
};

/// A control plan line, with the project's ids.
pub type ControlLine = utsim_ferramentas::planos::ControlLine<Id>;

/// Reads a control plan line from its item.
pub fn read_control_line(i: &ItemRef) -> ControlLine {
    ControlLine {
        id: i.id,
        process: i.text(fields::PROCESS),
        characteristic: i.text(fields::CHARACTERISTIC),
        fmea_row: i.entity.id(fields::FMEA_ROW),
        critical: i.entity.boolean(fields::CRITICAL),
    }
}

pub fn control_lines(state: &State, deliverable: Id) -> Vec<ControlLine> {
    items(state, deliverable, CONTROL_ITEM)
        .iter()
        .map(read_control_line)
        .collect()
}

/// What a measure of the data collection plan is missing to be collected the same way by
/// everyone: name, operational definition, unit (continuous data) and the sampling.
pub fn collection_issues(i: &ItemRef) -> Vec<CollectionIssue> {
    utsim_ferramentas::planos::collection_issues(|f| i.text(f))
}

/// Edits that open a card for a signal of the response plan: title "signal: immediate
/// action", the containment and who to call in the 5W2H, the deadline as the card's due
/// date when it is a date, origin "plano de resposta" and a link to the signal.
pub fn signal_card(state: &State, project: Id, signal: Id, card: Id) -> Vec<Edit> {
    let Some(e) = state.entity(signal).filter(|e| !e.is_deleted()) else {
        return Vec::new();
    };
    let i = ItemRef {
        id: signal,
        entity: e,
    };
    let task = utsim_ferramentas::planos::signal_task(|f| i.text(f));
    let title = task.title;
    let mut edits = kanban::new_card(state, project, card, &title, Default::default());
    edits.retain(|e| e.field != df::action::ORIGIN);
    let c = Action::entity_ref(card);
    edits.push(Edit::new(
        c,
        df::action::ORIGIN,
        enum_value(Origin::ResponsePlan),
    ));
    edits.push(Edit::new(c, df::action::LINK, signal.to_string()));
    if let Some(how) = task.how {
        edits.push(Edit::new(c, df::action::HOW, how));
    }
    if let Some(call) = task.call {
        edits.push(Edit::new(c, df::action::WHERE, call));
    }
    if let Ok(d) = task.deadline.parse::<crate::date::Date>() {
        edits.push(Edit::new(c, df::action::DUE, d.to_string()));
    }
    edits.push(Edit::new(
        super::item_ref(signal),
        fields::CARD,
        card.to_string(),
    ));
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Column;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    #[test]
    fn synthetic_signal_opens_the_expected_card() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::ResponsePlan);
        let s = f.item(
            d,
            RESPONSE_ITEM,
            None,
            vec![
                (fields::SIGNAL, "Ponto fora do limite na carta X̄-R".into()),
                (fields::IMMEDIATE, "Parar a máquina e separar o lote".into()),
                (fields::CONTAINMENT, "Inspeção 100 % do lote".into()),
                (fields::CALL, "Supervisor do turno".into()),
                (fields::DEADLINE, "2026-11-10".into()),
            ],
        );
        let card = f.id();
        let e = signal_card(&f.state, f.project, s, card);
        f.commit(e);
        let a = Action::read(card, f.state.entity(card).unwrap());
        assert_eq!(
            a.title,
            "Ponto fora do limite na carta X̄-R: Parar a máquina e separar o lote"
        );
        assert_eq!(a.origin, Origin::ResponsePlan);
        assert_eq!(a.link, Some(s));
        assert_eq!(a.how, "Inspeção 100 % do lote");
        assert_eq!(a.place, "Acionar: Supervisor do turno");
        assert_eq!(a.due.map(|d| d.to_string()).as_deref(), Some("2026-11-10"));
        assert_eq!(a.column, Column::Todo);
        assert_eq!(f.state.entity(s).unwrap().id(fields::CARD), Some(card));
        let gone = f.id();
        assert!(signal_card(&f.state, f.project, gone, gone).is_empty());
    }

    #[test]
    fn collection_plan_checks() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::DataCollection);
        f.item(
            d,
            COLLECTION_ITEM,
            None,
            vec![(fields::NAME, "Diâmetro".into())],
        );
        f.item(
            d,
            COLLECTION_ITEM,
            None,
            vec![
                (fields::NAME, "Defeitos".into()),
                (fields::DATA_TYPE, "contagem".into()),
                (fields::DEFINITION, "Peças com risco visível a 30 cm".into()),
                (fields::SAMPLE_SIZE, "50".into()),
                (fields::FREQUENCY, "por turno".into()),
            ],
        );
        let it = items(&f.state, d, COLLECTION_ITEM);
        assert_eq!(
            collection_issues(&it[0]),
            [
                CollectionIssue::NoDefinition,
                CollectionIssue::NoUnit,
                CollectionIssue::NoSampling
            ]
        );
        assert!(
            collection_issues(&it[1]).is_empty(),
            "count data needs no unit"
        );
    }
}
