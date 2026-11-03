//! The plans (`docs/plano_lean_seis_sigma.md`, 2.2 and 2.5): data collection plan (Medir),
//! control plan and response plan (Controlar). Mostly structure; the response plan turns a
//! signal into a card on the board.

use super::{ItemRef, items};
use crate::domain::{Action, Origin, enum_value, fields as df};
use crate::id::Id;
use crate::kanban;
use crate::op::Edit;
use crate::state::State;

/// A measure of the data collection plan.
pub const COLLECTION_ITEM: &str = "medida";
/// A characteristic of the control plan.
pub const CONTROL_ITEM: &str = "caracteristica";
/// A signal of the response plan.
pub const RESPONSE_ITEM: &str = "sinal";

pub mod fields {
    // Data collection plan.
    /// `y` or `x`.
    pub const ROLE: &str = "papel";
    pub const NAME: &str = "nome";
    pub const DEFINITION: &str = "definicao_operacional";
    /// `continuo`, `atributo` or `contagem`.
    pub const DATA_TYPE: &str = "tipo_dado";
    pub const UNIT: &str = "unidade";
    pub const STRATIFY: &str = "estratificacao";
    pub const SAMPLE_SIZE: &str = "tamanho_amostra";
    pub const FREQUENCY: &str = "frequencia";
    pub const SUBGROUP: &str = "subgrupo";
    pub const WHO: &str = "quem";
    pub const WHERE: &str = "onde";
    pub const FORM: &str = "formulario";

    // Control plan.
    pub const PROCESS: &str = "processo";
    pub const CHARACTERISTIC: &str = "caracteristica";
    /// `produto` or `processo`.
    pub const CHAR_KIND: &str = "tipo_caracteristica";
    pub const SPEC: &str = "especificacao";
    pub const METHOD: &str = "metodo_medicao";
    pub const CONTROL: &str = "metodo_controle";
    pub const OWNER: &str = "responsavel";
    pub const REACTION: &str = "plano_reacao";
    /// The FMEA row this line controls.
    pub const FMEA_ROW: &str = "linha_fmea";
    pub const CRITICAL: &str = "critica";

    // Response plan.
    pub const SIGNAL: &str = "sinal";
    pub const IMMEDIATE: &str = "acao_imediata";
    pub const CONTAINMENT: &str = "contencao";
    pub const CALL: &str = "acionar";
    pub const DEADLINE: &str = "prazo";
    /// Cards made from this signal (the last one).
    pub const CARD: &str = "cartao";
}

/// Data types of a measure.
pub const DATA_TYPES: [&str; 3] = ["continuo", "atributo", "contagem"];

/// A control plan line, read.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlLine {
    pub id: Id,
    pub process: String,
    pub characteristic: String,
    pub fmea_row: Option<Id>,
    pub critical: bool,
}

impl ControlLine {
    pub fn read(i: &ItemRef) -> Self {
        Self {
            id: i.id,
            process: i.text(fields::PROCESS),
            characteristic: i.text(fields::CHARACTERISTIC),
            fmea_row: i.entity.id(fields::FMEA_ROW),
            critical: i.entity.boolean(fields::CRITICAL),
        }
    }
}

pub fn control_lines(state: &State, deliverable: Id) -> Vec<ControlLine> {
    items(state, deliverable, CONTROL_ITEM)
        .iter()
        .map(ControlLine::read)
        .collect()
}

/// A problem in a data collection plan line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionIssue {
    NoName,
    NoDefinition,
    NoUnit,
    NoSampling,
}

/// What a measure of the data collection plan is missing to be collected the same way by
/// everyone: name, operational definition, unit (continuous data) and the sampling.
pub fn collection_issues(i: &ItemRef) -> Vec<CollectionIssue> {
    let mut out = Vec::new();
    let empty = |f: &str| i.text(f).trim().is_empty();
    if empty(fields::NAME) {
        out.push(CollectionIssue::NoName);
    }
    if empty(fields::DEFINITION) {
        out.push(CollectionIssue::NoDefinition);
    }
    let continuous = matches!(i.text(fields::DATA_TYPE).as_str(), "" | "continuo");
    if continuous && empty(fields::UNIT) {
        out.push(CollectionIssue::NoUnit);
    }
    if empty(fields::SAMPLE_SIZE) || empty(fields::FREQUENCY) {
        out.push(CollectionIssue::NoSampling);
    }
    out
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
    let what = i.text(fields::SIGNAL);
    let act = i.text(fields::IMMEDIATE);
    let title = match (what.trim(), act.trim()) {
        ("", "") => "Plano de resposta".to_string(),
        (w, "") => w.to_string(),
        ("", a) => a.to_string(),
        (w, a) => format!("{w}: {a}"),
    };
    let mut edits = kanban::new_card(state, project, card, &title, Default::default());
    edits.retain(|e| e.field != df::action::ORIGIN);
    let c = Action::entity_ref(card);
    edits.push(Edit::new(
        c,
        df::action::ORIGIN,
        enum_value(Origin::ResponsePlan),
    ));
    edits.push(Edit::new(c, df::action::LINK, signal.to_string()));
    let how = i.text(fields::CONTAINMENT);
    if !how.trim().is_empty() {
        edits.push(Edit::new(c, df::action::HOW, how.trim()));
    }
    let call = i.text(fields::CALL);
    if !call.trim().is_empty() {
        edits.push(Edit::new(
            c,
            df::action::WHERE,
            format!("Acionar: {}", call.trim()),
        ));
    }
    if let Ok(d) = i.text(fields::DEADLINE).trim().parse::<crate::date::Date>() {
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
