//! Process FMEA (`docs/plano_lean_seis_sigma.md`, 2.3): rows with step, failure mode,
//! effect, severity S, cause, occurrence O, prevention and detection controls, detection D;
//! RPN = S × O × D and the action priority from the project's table ([`super::ap`]); the
//! recommended action becomes a card on the board (linked both ways); S, O and D revised
//! after the action, with RPN and AP before and after; critical characteristics (high S)
//! checked against the control plan.

use serde_json::Value;

use super::ap::{Ap, ApTable};
use super::{Deliverable, ItemRef, Tool, item_ref, items, of_tool, plans, rating};
use crate::domain::{Origin, Project, enum_value, fields as df};
use crate::id::Id;
use crate::kanban;
use crate::op::Edit;
use crate::state::State;

pub const ITEM: &str = "linha_fmea";

pub mod fields {
    pub const STEP: &str = "etapa";
    pub const MODE: &str = "modo";
    pub const EFFECT: &str = "efeito";
    pub const S: &str = "s";
    pub const CAUSE: &str = "causa";
    pub const O: &str = "o";
    pub const PREVENTION: &str = "prevencao";
    pub const DETECTION: &str = "deteccao";
    pub const D: &str = "d";
    pub const ACTION: &str = "acao";
    /// The board card made from the action.
    pub const CARD: &str = "cartao";
    pub const ACTION_TAKEN: &str = "acao_tomada";
    pub const S2: &str = "s_revista";
    pub const O2: &str = "o_revista";
    pub const D2: &str = "d_revista";
    /// Deliverable: severity from which a row is a critical characteristic (default 9).
    pub const CRITICAL_FROM: &str = "severidade_critica";
}

/// Severity from which a characteristic is critical when the deliverable does not say.
pub const CRITICAL_FROM: u8 = 9;

/// The three company scales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    S,
    O,
    D,
}

impl Scale {
    pub const ALL: [Scale; 3] = [Scale::S, Scale::O, Scale::D];

    /// Deliverable field holding the description of `level` (1 to 10): `escala_s_7`.
    pub fn field(self, level: u8) -> String {
        let k = match self {
            Scale::S => 's',
            Scale::O => 'o',
            Scale::D => 'd',
        };
        format!("escala_{k}_{level}")
    }
}

/// The descriptions of a scale written by the company (empty where not written).
pub fn scale(state: &State, deliverable: Id, which: Scale) -> [String; 10] {
    let e = state.entity(deliverable);
    std::array::from_fn(|i| {
        e.map(|e| e.text(&which.field(i as u8 + 1)))
            .unwrap_or_default()
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: Id,
    pub step: String,
    pub mode: String,
    pub effect: String,
    pub s: Option<u8>,
    pub cause: String,
    pub o: Option<u8>,
    pub prevention: String,
    pub detection: String,
    pub d: Option<u8>,
    pub action: String,
    pub card: Option<Id>,
    pub action_taken: String,
    pub s2: Option<u8>,
    pub o2: Option<u8>,
    pub d2: Option<u8>,
}

/// RPN and AP before and after the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revision {
    pub rpn_before: u32,
    pub rpn_after: u32,
    pub ap_before: Ap,
    pub ap_after: Ap,
}

impl Revision {
    pub fn improved(&self) -> bool {
        self.rpn_after < self.rpn_before || self.ap_after < self.ap_before
    }
}

impl Row {
    pub fn read(i: &ItemRef) -> Self {
        use fields as f;
        let e = i.entity;
        Self {
            id: i.id,
            step: e.text(f::STEP),
            mode: e.text(f::MODE),
            effect: e.text(f::EFFECT),
            s: rating(e, f::S),
            cause: e.text(f::CAUSE),
            o: rating(e, f::O),
            prevention: e.text(f::PREVENTION),
            detection: e.text(f::DETECTION),
            d: rating(e, f::D),
            action: e.text(f::ACTION),
            card: e.id(f::CARD),
            action_taken: e.text(f::ACTION_TAKEN),
            s2: rating(e, f::S2),
            o2: rating(e, f::O2),
            d2: rating(e, f::D2),
        }
    }

    /// S, O and D when all three are rated.
    pub fn sod(&self) -> Option<(u8, u8, u8)> {
        Some((self.s?, self.o?, self.d?))
    }

    /// S, O and D after the action: each revised value, or the original where not revised.
    /// `None` while nothing was revised.
    pub fn sod_after(&self) -> Option<(u8, u8, u8)> {
        if self.s2.is_none() && self.o2.is_none() && self.d2.is_none() {
            return None;
        }
        let (s, o, d) = self.sod()?;
        Some((
            self.s2.unwrap_or(s),
            self.o2.unwrap_or(o),
            self.d2.unwrap_or(d),
        ))
    }

    pub fn rpn(&self) -> Option<u32> {
        self.sod().map(rpn)
    }

    pub fn ap(&self, table: &ApTable) -> Option<Ap> {
        self.sod().and_then(|(s, o, d)| table.get(s, o, d))
    }

    pub fn revision(&self, table: &ApTable) -> Option<Revision> {
        let (s, o, d) = self.sod()?;
        let (s2, o2, d2) = self.sod_after()?;
        Some(Revision {
            rpn_before: rpn((s, o, d)),
            rpn_after: rpn((s2, o2, d2)),
            ap_before: table.get(s, o, d)?,
            ap_after: table.get(s2, o2, d2)?,
        })
    }

    /// Severity that decides whether the characteristic is critical (the revised one when
    /// there is one: a design change can lower it).
    pub fn current_s(&self) -> Option<u8> {
        self.s2.or(self.s)
    }

    pub fn is_critical(&self, from: u8) -> bool {
        self.current_s().is_some_and(|s| s >= from)
    }
}

/// `S × O × D` (1 to 1000).
pub fn rpn((s, o, d): (u8, u8, u8)) -> u32 {
    u32::from(s) * u32::from(o) * u32::from(d)
}

/// Rows of an FMEA deliverable in order.
pub fn rows(state: &State, deliverable: Id) -> Vec<Row> {
    items(state, deliverable, ITEM)
        .iter()
        .map(Row::read)
        .collect()
}

/// Rows sorted for action: AP (high first), then RPN, then S.
pub fn ranked(rows: &[Row], table: &ApTable) -> Vec<Row> {
    let mut v = rows.to_vec();
    v.sort_by(|a, b| {
        b.ap(table)
            .cmp(&a.ap(table))
            .then(b.rpn().cmp(&a.rpn()))
            .then(b.s.cmp(&a.s))
    });
    v
}

/// Where the project's AP table comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableSource {
    BuiltIn,
    /// Imported by the company, with the name given at the import.
    Company(String),
}

/// The AP table that applies to the project: the company's when one was imported (and is
/// complete), else the built-in one.
pub fn project_table(state: &State, project: Id) -> (ApTable, TableSource) {
    let e = state.entity(project);
    let company = e
        .and_then(|e| e.opt_text(df::project::AP_TABLE))
        .and_then(|t| ApTable::from_compact(&t));
    match company {
        Some(t) => {
            let name = e
                .map(|e| e.text(df::project::AP_TABLE_NAME))
                .unwrap_or_default();
            (t, TableSource::Company(name))
        }
        None => (ApTable::builtin(), TableSource::BuiltIn),
    }
}

/// Edits that import a company table from CSV for the whole project.
pub fn import_table(
    project: Id,
    csv: &str,
    name: &str,
) -> Result<Vec<Edit>, Vec<super::ap::CsvError>> {
    let t = ApTable::from_csv(csv)?;
    let r = Project::entity_ref(project);
    Ok(vec![
        Edit::new(r, df::project::AP_TABLE, t.to_compact()),
        Edit::new(r, df::project::AP_TABLE_NAME, name.trim()),
    ])
}

/// Edits that go back to the built-in table.
pub fn use_builtin_table(project: Id) -> Vec<Edit> {
    let r = Project::entity_ref(project);
    vec![
        Edit::new(r, df::project::AP_TABLE, ""),
        Edit::new(r, df::project::AP_TABLE_NAME, ""),
    ]
}

/// Edits that turn the recommended action of a row into a card at the end of "A fazer",
/// with origin FMEA and a link to the row; the row keeps the card's id. Nothing when the
/// row already has a live card.
pub fn action_card(state: &State, project: Id, row: Id, card: Id) -> Vec<Edit> {
    let Some(e) = state.entity(row).filter(|e| !e.is_deleted()) else {
        return Vec::new();
    };
    let r = Row::read(&ItemRef { id: row, entity: e });
    if r.card
        .and_then(|c| state.entity(c))
        .is_some_and(|c| !c.is_deleted())
    {
        return Vec::new();
    }
    let title = if r.action.trim().is_empty() {
        format!("FMEA: {}", r.mode.trim())
    } else {
        r.action.trim().to_string()
    };
    let mut edits = kanban::new_card(state, project, card, &title, Default::default());
    let c = crate::domain::Action::entity_ref(card);
    edits.retain(|e| e.field != df::action::ORIGIN);
    edits.push(Edit::new(c, df::action::ORIGIN, enum_value(Origin::Fmea)));
    edits.push(Edit::new(c, df::action::LINK, row.to_string()));
    let why = [r.mode.trim(), r.cause.trim()]
        .iter()
        .filter(|x| !x.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" / ");
    if !why.is_empty() {
        edits.push(Edit::new(c, df::action::WHY, why));
    }
    edits.push(Edit::new(item_ref(row), fields::CARD, card.to_string()));
    edits
}

/// A critical characteristic of an FMEA without a line in any control plan of the project.
#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    pub fmea: Id,
    pub row: Row,
}

/// Severity from which rows of this FMEA are critical.
pub fn critical_from(state: &State, deliverable: Id) -> u8 {
    state
        .entity(deliverable)
        .and_then(|e| rating(e, fields::CRITICAL_FROM))
        .unwrap_or(CRITICAL_FROM)
}

/// The critical rows of every FMEA of the project that no control plan line covers (a line
/// covers a row when it links to it, `linha_fmea`). The check of the Controlar tollgate.
pub fn critical_without_control(state: &State, project: Id) -> Vec<Gap> {
    let covered: Vec<Id> = of_tool(state, project, Tool::ControlPlan)
        .iter()
        .flat_map(|cp| plans::control_lines(state, cp.id))
        .filter_map(|l| l.fmea_row)
        .collect();
    let mut gaps = Vec::new();
    for f in of_tool(state, project, Tool::Fmea) {
        let from = critical_from(state, f.id);
        for row in rows(state, f.id) {
            if row.is_critical(from) && !covered.contains(&row.id) {
                gaps.push(Gap { fmea: f.id, row });
            }
        }
    }
    gaps
}

/// Every critical row of the project's FMEAs, with its FMEA (for the control plan picker).
pub fn critical_rows(state: &State, project: Id) -> Vec<(Deliverable, Row)> {
    let mut out = Vec::new();
    for f in of_tool(state, project, Tool::Fmea) {
        let from = critical_from(state, f.id);
        for row in rows(state, f.id) {
            if row.is_critical(from) {
                out.push((f.clone(), row));
            }
        }
    }
    out
}

/// Edit that sets a rating (1 to 10) or clears it (`None`).
pub fn set_rating(row: Id, field: &str, v: Option<u8>) -> Edit {
    let value = v.map_or(Value::from(""), Value::from);
    Edit::new(item_ref(row), field, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Action, Column};
    use crate::tools::testing::Fixture;

    fn row(f: &mut Fixture, d: Id, s: u8, o: u8, dd: u8) -> Id {
        f.item(
            d,
            ITEM,
            None,
            vec![
                (fields::MODE, "Furo fora de posição".into()),
                (fields::CAUSE, "Gabarito gasto".into()),
                (fields::S, s.into()),
                (fields::O, o.into()),
                (fields::D, dd.into()),
            ],
        )
    }

    #[test]
    fn rpn_ap_and_ranking() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Fmea);
        let a = row(&mut f, d, 5, 5, 5);
        let b = row(&mut f, d, 10, 4, 2);
        let c = f.item(d, ITEM, None, vec![(fields::S, 11.into())]);
        let t = ApTable::builtin();
        let rs = rows(&f.state, d);
        assert_eq!(rs[0].rpn(), Some(125));
        assert_eq!(rs[1].rpn(), Some(80));
        assert_eq!(rs[0].ap(&t), Some(t.get(5, 5, 5).unwrap()));
        assert_eq!(rs[1].ap(&t), Some(Ap::High));
        assert_eq!((rs[2].s, rs[2].rpn(), rs[2].ap(&t)), (None, None, None));
        let order: Vec<Id> = ranked(&rs, &t).iter().map(|r| r.id).collect();
        assert_eq!(
            order,
            [b, a, c],
            "AP first, then RPN: the lower RPN of b comes first"
        );
        assert_eq!(rpn((10, 10, 10)), 1000);
        assert_eq!(rpn((1, 1, 1)), 1);
    }

    #[test]
    fn revision_before_and_after() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Fmea);
        let r = row(&mut f, d, 9, 6, 6);
        let t = ApTable::builtin();
        assert_eq!(rows(&f.state, d)[0].revision(&t), None, "not revised yet");
        f.commit(vec![
            set_rating(r, fields::O2, Some(2)),
            set_rating(r, fields::D2, Some(3)),
        ]);
        let rev = rows(&f.state, d)[0].revision(&t).unwrap();
        assert_eq!((rev.rpn_before, rev.rpn_after), (324, 54));
        assert_eq!((rev.ap_before, rev.ap_after), (Ap::High, Ap::Medium));
        assert!(rev.improved());
        assert_eq!(rows(&f.state, d)[0].sod_after(), Some((9, 2, 3)));
        f.commit(vec![set_rating(r, fields::O2, None)]);
        assert_eq!(
            rows(&f.state, d)[0].sod_after(),
            Some((9, 6, 3)),
            "cleared: original O"
        );
    }

    #[test]
    fn action_becomes_a_card_linked_both_ways() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Fmea);
        let r = row(&mut f, d, 8, 6, 4);
        f.set(r, fields::ACTION, "Trocar o gabarito a cada 5 mil peças");
        let card = f.id();
        let e = action_card(&f.state, f.project, r, card);
        f.commit(e);
        let a = Action::read(card, f.state.entity(card).unwrap());
        assert_eq!(a.title, "Trocar o gabarito a cada 5 mil peças");
        assert_eq!(a.origin, Origin::Fmea);
        assert_eq!(a.link, Some(r));
        assert_eq!(a.column, Column::Todo);
        assert_eq!(a.why, "Furo fora de posição / Gabarito gasto");
        assert_eq!(rows(&f.state, d)[0].card, Some(card));
        assert_eq!(kanban::board(&f.state, f.project)[&Column::Todo].len(), 1);
        let other = f.id();
        assert!(
            action_card(&f.state, f.project, r, other).is_empty(),
            "one card per row"
        );
        // Card deleted: a new one can be made; the title falls back to the mode.
        f.set(card, crate::op::FIELD_DELETED, true);
        f.set(r, fields::ACTION, "");
        let e = action_card(&f.state, f.project, r, other);
        f.commit(e);
        assert_eq!(
            Action::read(other, f.state.entity(other).unwrap()).title,
            "FMEA: Furo fora de posição"
        );
        let missing = f.id();
        assert!(action_card(&f.state, f.project, missing, missing).is_empty());
    }

    #[test]
    fn critical_characteristics_against_the_control_plan() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Fmea);
        let crit = row(&mut f, d, 9, 2, 2);
        let _minor = row(&mut f, d, 6, 9, 9);
        let gaps = critical_without_control(&f.state, f.project);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].row.id, crit);
        let cp = f.deliverable(Tool::ControlPlan);
        let line = f.item(
            cp,
            plans::CONTROL_ITEM,
            None,
            vec![(plans::fields::CHARACTERISTIC, "Posição do furo".into())],
        );
        assert_eq!(
            critical_without_control(&f.state, f.project).len(),
            1,
            "not linked yet"
        );
        f.set(line, plans::fields::FMEA_ROW, crit.to_string());
        assert!(critical_without_control(&f.state, f.project).is_empty());
        // Threshold of the deliverable.
        f.set(d, fields::CRITICAL_FROM, 6);
        assert_eq!(critical_without_control(&f.state, f.project).len(), 1);
        assert_eq!(critical_rows(&f.state, f.project).len(), 2);
    }

    #[test]
    fn company_table_applies_to_the_project() {
        let mut f = Fixture::new();
        let (t, src) = project_table(&f.state, f.project);
        assert_eq!((t, src), (ApTable::builtin(), TableSource::BuiltIn));
        let all_high = "S,O,D,AP\n".to_string()
            + &(1..=10)
                .flat_map(|s| {
                    (1..=10).flat_map(move |o| (1..=10).map(move |d| format!("{s},{o},{d},H\n")))
                })
                .collect::<String>();
        let e = import_table(f.project, &all_high, " Tabela da empresa ").unwrap();
        f.commit(e);
        let (t, src) = project_table(&f.state, f.project);
        assert_eq!(t.get(1, 1, 1), Some(Ap::High));
        assert_eq!(src, TableSource::Company("Tabela da empresa".into()));
        assert!(import_table(f.project, "1,1,1,H", "x").is_err());
        f.commit(use_builtin_table(f.project));
        assert_eq!(project_table(&f.state, f.project).1, TableSource::BuiltIn);
    }

    #[test]
    fn company_scales() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Fmea);
        assert_eq!(Scale::S.field(10), "escala_s_10");
        f.set(d, &Scale::D.field(10), "Não há controle de detecção");
        let sc = scale(&f.state, d, Scale::D);
        assert_eq!(sc[9], "Não há controle de detecção");
        assert!(sc[0].is_empty());
    }
}
