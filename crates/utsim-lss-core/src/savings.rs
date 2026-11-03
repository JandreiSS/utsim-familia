//! Savings (`docs/plano_lean_seis_sigma.md`, 1.10; decision G1: the default hard and soft
//! definitions and 12 months of follow-up).
//!
//! States `estimado → previsto → realizado → validado`: the belt estimates (in the
//! charter); the controller validates the forecast; monthly entries after the
//! implementation make it realised; the controller confirms the realised entries. Each
//! value carries its rationale (formula, assumptions, data source) and who validated it and
//! when. A saving estimated by simulation never becomes validated without real entries.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::domain::{
    RoleKind, Saving, SavingCategory, SavingStatus, enum_value, fields as dfields, of_project,
};
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::roles;
use crate::state::{Entity, State};

/// Months of follow-up after the Control phase (decision G1).
pub const DEFAULT_FOLLOW_UP_MONTHS: u32 = 12;

pub mod fields {
    // Saving (besides `projeto`, `descricao`, `categoria`, `estado`, `valor`, `memoria`).
    pub const SOURCE: &str = "origem";
    /// Expected value per month during the follow-up.
    pub const MONTHLY: &str = "valor_mensal";
    /// First month of the follow-up, `AAAA-MM`.
    pub const START: &str = "inicio_acompanhamento";
    pub const MONTHS: &str = "meses_acompanhamento";
    pub const FORECAST_BY: &str = "previsto_por";
    pub const FORECAST_ON: &str = "previsto_em";
    pub const VALIDATED_BY: &str = "validado_por";
    pub const VALIDATED_ON: &str = "validado_em";
    // Monthly entry.
    pub const SAVING: &str = "ganho";
    pub const MONTH: &str = "mes";
    pub const VALUE: &str = "valor";
    pub const ENTRY_VALIDATED: &str = "validado";
    pub const NOTE: &str = "observacao";
}

/// Where an estimate came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Source {
    #[default]
    #[serde(rename = "belt")]
    Belt,
    /// Scenarios in the simulator (section 3.4).
    #[serde(rename = "simulacao")]
    Simulation,
    #[serde(rename = "controladoria")]
    Controller,
}

/// A month, `AAAA-MM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Month {
    pub year: i32,
    pub month: u8,
}

impl Month {
    pub fn of(d: Date) -> Month {
        Month {
            year: d.year,
            month: d.month,
        }
    }

    pub fn plus(self, n: i32) -> Month {
        let total = self.year * 12 + i32::from(self.month) - 1 + n;
        Month {
            year: total.div_euclid(12),
            month: (total.rem_euclid(12) + 1) as u8,
        }
    }

    /// `MM/AAAA`.
    pub fn format_br(self) -> String {
        format!("{:02}/{:04}", self.month, self.year)
    }
}

impl fmt::Display for Month {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

impl FromStr for Month {
    type Err = ();
    /// `AAAA-MM` or `MM/AAAA`.
    fn from_str(s: &str) -> Result<Self, ()> {
        let t = s.trim();
        let (y, m) = if let Some((y, m)) = t.split_once('-') {
            (y, m)
        } else if let Some((m, y)) = t.split_once('/') {
            (y, m)
        } else {
            return Err(());
        };
        if y.len() != 4 || m.is_empty() || m.len() > 2 {
            return Err(());
        }
        let year: i32 = y.parse().map_err(|_| ())?;
        let month: u8 = m.parse().map_err(|_| ())?;
        if (1..=12).contains(&month) {
            Ok(Month { year, month })
        } else {
            Err(())
        }
    }
}

/// The saving fields added in stage L1.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub saving: Saving,
    pub source: Source,
    pub monthly: f64,
    pub start: Option<Month>,
    pub months: u32,
    pub forecast_by: Option<Id>,
    pub forecast_on: Option<Date>,
    pub validated_by: Option<Id>,
    pub validated_on: Option<Date>,
}

impl Plan {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields as f;
        let date = |field| e.opt_text(field).and_then(|s| s.parse::<Date>().ok());
        Self {
            saving: Saving::read(id, e),
            source: e.typed(f::SOURCE).unwrap_or_default(),
            monthly: e.number(f::MONTHLY).unwrap_or(0.0),
            start: e.opt_text(f::START).and_then(|s| s.parse().ok()),
            months: e
                .integer(f::MONTHS)
                .filter(|m| *m > 0)
                .map_or(DEFAULT_FOLLOW_UP_MONTHS, |m| m.min(120) as u32),
            forecast_by: e.id(f::FORECAST_BY),
            forecast_on: date(f::FORECAST_ON),
            validated_by: e.id(f::VALIDATED_BY),
            validated_on: date(f::VALIDATED_ON),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: Id,
    pub saving: Option<Id>,
    pub month: Option<Month>,
    pub value: f64,
    pub validated: bool,
    pub note: String,
}

impl Entry {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields as f;
        Self {
            id,
            saving: e.id(f::SAVING),
            month: e.opt_text(f::MONTH).and_then(|s| s.parse().ok()),
            value: e.number(f::VALUE).unwrap_or(0.0),
            validated: e.boolean(f::ENTRY_VALIDATED),
            note: e.text(f::NOTE),
        }
    }
}

pub fn plans(state: &State, project: Id) -> Vec<Plan> {
    of_project(state, Kind::Saving, project, Plan::read)
}

/// Monthly entries of a saving, by month.
pub fn entries(state: &State, saving: Id) -> Vec<Entry> {
    let mut v: Vec<Entry> = state
        .of_kind(Kind::SavingEntry)
        .filter(|(_, e)| e.id(fields::SAVING) == Some(saving))
        .map(|(id, e)| Entry::read(id, e))
        .collect();
    v.sort_by_key(|e| (e.month, e.id));
    v
}

/// Edits that create a saving (estimated by the belt, or by simulation).
pub fn new_saving(
    project: Id,
    id: Id,
    description: &str,
    category: SavingCategory,
    value: f64,
    source: Source,
) -> Vec<Edit> {
    use dfields::saving as f;
    let r = EntityRef::new(Kind::Saving, id);
    vec![
        Edit::new(r, f::PROJECT, project.to_string()),
        Edit::new(r, f::DESCRIPTION, description.trim()),
        Edit::new(r, f::CATEGORY, enum_value(category)),
        Edit::new(r, f::STATUS, enum_value(SavingStatus::Estimated)),
        Edit::new(r, f::VALUE, value),
        Edit::new(r, fields::SOURCE, enum_value(source)),
        Edit::new(r, fields::MONTHS, DEFAULT_FOLLOW_UP_MONTHS),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SavingError {
    #[error("ganho não encontrado")]
    NotFound,
    #[error("só a controladoria do projeto valida ganhos; defina a controladoria em Papéis")]
    NotController,
    #[error("passagem de estado não permitida")]
    BadTransition,
    #[error("o ganho precisa de ao menos um lançamento mensal")]
    NoEntries,
    #[error("ganho estimado por simulação só é validado com lançamentos reais")]
    SimulationOnly,
}

/// Edits that move a saving to `to`. Forward one step at a time, or back one step by the
/// controller (to correct). Forecast and validation are by the controller; realised needs
/// a monthly entry; validating confirms every entry too.
pub fn transition(
    state: &State,
    project: Id,
    saving: Id,
    to: SavingStatus,
    by: Id,
    today: Date,
) -> Result<Vec<Edit>, SavingError> {
    use SavingStatus::*;
    let e = state
        .entity(saving)
        .filter(|e| e.kind == Kind::Saving && !e.is_deleted())
        .ok_or(SavingError::NotFound)?;
    let plan = Plan::read(saving, e);
    let from = plan.saving.status;
    let controller = roles::has_role(state, project, by, RoleKind::Controller);
    let ents = entries(state, saving);
    let step = |s: SavingStatus| match s {
        Estimated => 0,
        Forecast => 1,
        Realized => 2,
        Validated => 3,
    };
    let (a, b) = (step(from), step(to));
    if b == a + 1 {
        match to {
            Forecast | Validated if !controller => return Err(SavingError::NotController),
            Realized if ents.is_empty() => return Err(SavingError::NoEntries),
            Validated if ents.is_empty() => {
                return Err(if plan.source == Source::Simulation {
                    SavingError::SimulationOnly
                } else {
                    SavingError::NoEntries
                });
            }
            _ => {}
        }
    } else if b + 1 == a {
        if !controller {
            return Err(SavingError::NotController);
        }
    } else {
        return Err(SavingError::BadTransition);
    }
    let r = EntityRef::new(Kind::Saving, saving);
    let mut edits = vec![Edit::new(r, dfields::saving::STATUS, enum_value(to))];
    match to {
        Forecast if b > a => {
            edits.push(Edit::new(r, fields::FORECAST_BY, by.to_string()));
            edits.push(Edit::new(r, fields::FORECAST_ON, today.to_string()));
        }
        Validated => {
            edits.push(Edit::new(r, fields::VALIDATED_BY, by.to_string()));
            edits.push(Edit::new(r, fields::VALIDATED_ON, today.to_string()));
            for en in ents.iter().filter(|x| !x.validated) {
                edits.push(Edit::new(
                    EntityRef::new(Kind::SavingEntry, en.id),
                    fields::ENTRY_VALIDATED,
                    true,
                ));
            }
        }
        _ => {}
    }
    Ok(edits)
}

/// Edits that record (or correct) the realised value of a month. A forecast saving becomes
/// realised with its first entry.
pub fn record_month(state: &State, saving: Id, month: Month, value: f64, new_id: Id) -> Vec<Edit> {
    let existing = entries(state, saving)
        .into_iter()
        .find(|e| e.month == Some(month));
    let mut edits = match existing {
        Some(en) => {
            let r = EntityRef::new(Kind::SavingEntry, en.id);
            let mut e = vec![Edit::new(r, fields::VALUE, value)];
            if en.validated {
                // A corrected value needs the controller again.
                e.push(Edit::new(r, fields::ENTRY_VALIDATED, false));
            }
            e
        }
        None => {
            let r = EntityRef::new(Kind::SavingEntry, new_id);
            vec![
                Edit::new(r, fields::SAVING, saving.to_string()),
                Edit::new(r, fields::MONTH, month.to_string()),
                Edit::new(r, fields::VALUE, value),
            ]
        }
    };
    let status = state
        .entity(saving)
        .and_then(|e| e.typed::<SavingStatus>(dfields::saving::STATUS))
        .unwrap_or_default();
    let r = EntityRef::new(Kind::Saving, saving);
    // Forecast: the first entry makes it realised. Validated: a new or corrected month
    // needs the controller again.
    if matches!(status, SavingStatus::Forecast | SavingStatus::Validated) {
        edits.push(Edit::new(
            r,
            dfields::saving::STATUS,
            enum_value(SavingStatus::Realized),
        ));
    }
    edits
}

/// One month of the follow-up.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthRow {
    pub month: Month,
    pub expected: f64,
    pub realized: Option<f64>,
    pub validated: bool,
    pub cumulative_expected: f64,
    pub cumulative_realized: f64,
}

/// The follow-up table: every month from the start, expected × realised and cumulative.
/// Without a start, it starts at the first entry; without entries either, it is empty.
pub fn follow_up(plan: &Plan, entries: &[Entry]) -> Vec<MonthRow> {
    let Some(start) = plan
        .start
        .or_else(|| entries.iter().filter_map(|e| e.month).min())
    else {
        return Vec::new();
    };
    let by_month: BTreeMap<Month, &Entry> = entries
        .iter()
        .filter_map(|e| e.month.map(|m| (m, e)))
        .collect();
    let mut rows = Vec::new();
    let (mut ce, mut cr) = (0.0, 0.0);
    for i in 0..plan.months as i32 {
        let m = start.plus(i);
        let en = by_month.get(&m);
        ce += plan.monthly;
        cr += en.map_or(0.0, |e| e.value);
        rows.push(MonthRow {
            month: m,
            expected: plan.monthly,
            realized: en.map(|e| e.value),
            validated: en.is_some_and(|e| e.validated),
            cumulative_expected: ce,
            cumulative_realized: cr,
        });
    }
    rows
}

/// Sums of a project by category and state. Estimated and forecast use the saving's value;
/// realised is the sum of the monthly entries; validated the sum of the validated entries.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Totals {
    pub by: BTreeMap<(Category, SavingStatus), f64>,
}

/// [`SavingCategory`] with an order (for the map key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Hard,
    Soft,
}

impl From<SavingCategory> for Category {
    fn from(c: SavingCategory) -> Self {
        match c {
            SavingCategory::Hard => Category::Hard,
            SavingCategory::Soft => Category::Soft,
        }
    }
}

impl Totals {
    pub fn get(&self, c: Category, s: SavingStatus) -> f64 {
        self.by.get(&(c, s)).copied().unwrap_or(0.0)
    }
}

pub fn totals(state: &State, project: Id) -> Totals {
    let mut t = Totals::default();
    for p in plans(state, project) {
        let c = Category::from(p.saving.category);
        let ents = entries(state, p.saving.id);
        let realized: f64 = ents.iter().map(|e| e.value).sum();
        let validated: f64 = ents.iter().filter(|e| e.validated).map(|e| e.value).sum();
        *t.by.entry((c, SavingStatus::Estimated)).or_default() += p.saving.value;
        if p.saving.status >= SavingStatus::Forecast {
            *t.by.entry((c, SavingStatus::Forecast)).or_default() += p.saving.value;
        }
        *t.by.entry((c, SavingStatus::Realized)).or_default() += realized;
        *t.by.entry((c, SavingStatus::Validated)).or_default() += validated;
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Approach, Project};
    use proptest::prelude::*;

    struct T {
        s: State,
        n: u64,
        p: Id,
        belt: Id,
        ctrl: Id,
    }

    impl T {
        fn new() -> T {
            let mut t = T {
                s: State::new(),
                n: 0,
                p: Id::NIL,
                belt: Id::NIL,
                ctrl: Id::NIL,
            };
            t.p = t.id();
            t.belt = t.id();
            t.ctrl = t.id();
            let mut n = t.n;
            let e = Project::create(t.p, "P", Approach::Dmaic, || {
                n += 1;
                Id::v7(n, [n as u8; 10])
            });
            t.n = n;
            t.commit(e);
            let rid = t.id();
            let e = roles::add_role(&t.s, t.p, rid, t.ctrl, RoleKind::Controller);
            t.commit(e);
            t
        }
        fn id(&mut self) -> Id {
            self.n += 1;
            Id::v7(self.n, [self.n as u8; 10])
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
        fn status(&self, id: Id) -> SavingStatus {
            Plan::read(id, self.s.entity(id).unwrap()).saving.status
        }
    }

    fn today() -> Date {
        Date::new(2026, 11, 3).unwrap()
    }

    #[test]
    fn states_and_validation() {
        let mut t = T::new();
        let g = t.id();
        t.commit(new_saving(
            t.p,
            g,
            "Menos refugo",
            SavingCategory::Hard,
            120_000.0,
            Source::Belt,
        ));
        assert_eq!(t.status(g), SavingStatus::Estimated);
        // The belt cannot validate the forecast; skipping states is refused.
        assert_eq!(
            transition(&t.s, t.p, g, SavingStatus::Forecast, t.belt, today()),
            Err(SavingError::NotController)
        );
        assert_eq!(
            transition(&t.s, t.p, g, SavingStatus::Validated, t.ctrl, today()),
            Err(SavingError::BadTransition)
        );
        let e = transition(&t.s, t.p, g, SavingStatus::Forecast, t.ctrl, today()).unwrap();
        t.commit(e);
        let pl = Plan::read(g, t.s.entity(g).unwrap());
        assert_eq!(
            (pl.forecast_by, pl.forecast_on),
            (Some(t.ctrl), Some(today()))
        );
        assert_eq!(pl.months, 12);
        assert_eq!(
            transition(&t.s, t.p, g, SavingStatus::Realized, t.belt, today()),
            Err(SavingError::NoEntries)
        );
        // The first monthly entry makes it realised.
        let m = Month {
            year: 2027,
            month: 1,
        };
        let eid = t.id();
        let e = record_month(&t.s, g, m, 9_500.0, eid);
        t.commit(e);
        assert_eq!(t.status(g), SavingStatus::Realized);
        let eid = t.id();
        let e = record_month(&t.s, g, m.plus(1), 10_200.0, eid);
        t.commit(e);
        assert_eq!(entries(&t.s, g).len(), 2);
        assert_eq!(
            transition(&t.s, t.p, g, SavingStatus::Validated, t.belt, today()),
            Err(SavingError::NotController)
        );
        let e = transition(&t.s, t.p, g, SavingStatus::Validated, t.ctrl, today()).unwrap();
        t.commit(e);
        assert_eq!(t.status(g), SavingStatus::Validated);
        assert!(entries(&t.s, g).iter().all(|e| e.validated));
        let tot = totals(&t.s, t.p);
        assert_eq!(tot.get(Category::Hard, SavingStatus::Validated), 19_700.0);
        assert_eq!(tot.get(Category::Hard, SavingStatus::Forecast), 120_000.0);
        assert_eq!(tot.get(Category::Soft, SavingStatus::Estimated), 0.0);
        // Correcting a validated month goes back to realised and needs the controller.
        let e = record_month(&t.s, g, m, 9_000.0, Id::NIL);
        t.commit(e);
        assert_eq!(t.status(g), SavingStatus::Realized);
        assert!(!entries(&t.s, g)[0].validated);
        assert_eq!(
            totals(&t.s, t.p).get(Category::Hard, SavingStatus::Validated),
            10_200.0
        );
        // Back one step only by the controller.
        assert_eq!(
            transition(&t.s, t.p, g, SavingStatus::Forecast, t.belt, today()),
            Err(SavingError::NotController)
        );
        assert!(transition(&t.s, t.p, g, SavingStatus::Forecast, t.ctrl, today()).is_ok());
    }

    #[test]
    fn simulation_estimate_needs_real_entries() {
        let mut t = T::new();
        let g = t.id();
        t.commit(new_saving(
            t.p,
            g,
            "Capacidade liberada",
            SavingCategory::Soft,
            50_000.0,
            Source::Simulation,
        ));
        let e = transition(&t.s, t.p, g, SavingStatus::Forecast, t.ctrl, today()).unwrap();
        t.commit(e);
        // Force the state to realised without entries (e.g. an older file): still refused.
        t.commit(vec![Edit::new(
            EntityRef::new(Kind::Saving, g),
            dfields::saving::STATUS,
            enum_value(SavingStatus::Realized),
        )]);
        assert_eq!(
            transition(&t.s, t.p, g, SavingStatus::Validated, t.ctrl, today()),
            Err(SavingError::SimulationOnly)
        );
    }

    #[test]
    fn follow_up_table() {
        let mut t = T::new();
        let g = t.id();
        t.commit(new_saving(
            t.p,
            g,
            "x",
            SavingCategory::Hard,
            120.0,
            Source::Belt,
        ));
        let r = EntityRef::new(Kind::Saving, g);
        t.commit(vec![
            Edit::new(r, fields::MONTHLY, 10.0),
            Edit::new(r, fields::START, "2026-11"),
            Edit::new(r, fields::MONTHS, 3),
        ]);
        let eid = t.id();
        let e = record_month(
            &t.s,
            g,
            Month {
                year: 2026,
                month: 12,
            },
            12.0,
            eid,
        );
        t.commit(e);
        let pl = Plan::read(g, t.s.entity(g).unwrap());
        let rows = follow_up(&pl, &entries(&t.s, g));
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].month.to_string(), "2026-11");
        assert_eq!(rows[0].realized, None);
        assert_eq!(rows[1].realized, Some(12.0));
        assert_eq!(
            rows[2].month,
            Month {
                year: 2027,
                month: 1
            }
        );
        assert_eq!(rows[2].cumulative_expected, 30.0);
        assert_eq!(rows[2].cumulative_realized, 12.0);
    }

    #[test]
    fn months() {
        assert_eq!(
            "2026-11".parse::<Month>(),
            Ok(Month {
                year: 2026,
                month: 11
            })
        );
        assert_eq!(
            "03/2027".parse::<Month>(),
            Ok(Month {
                year: 2027,
                month: 3
            })
        );
        assert!("2026-13".parse::<Month>().is_err());
        assert!("26-01".parse::<Month>().is_err());
        assert_eq!(
            Month {
                year: 2026,
                month: 12
            }
            .plus(1)
            .to_string(),
            "2027-01"
        );
        assert_eq!(
            Month {
                year: 2026,
                month: 1
            }
            .plus(-1)
            .format_br(),
            "12/2025"
        );
        assert_eq!(Month::of(today()).to_string(), "2026-11");
    }

    proptest! {
        #[test]
        fn month_arithmetic(y in 1900i32..2200, m in 1u8..=12, k in -500i32..500) {
            let a = Month { year: y, month: m };
            prop_assert_eq!(a.plus(k).plus(-k), a);
            prop_assert_eq!(a.plus(k).to_string().parse::<Month>(), Ok(a.plus(k)));
            prop_assert!(k <= 0 || a.plus(k) > a);
        }
    }
}
