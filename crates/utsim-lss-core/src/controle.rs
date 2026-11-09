//! Control with the twin and the panel (P5, `docs/plano_lean_seis_sigma.md`, 2.5 and 3.5).
//!
//! Each line of the control plan can point to where the characteristic is read in
//! operation: `kpi:<id>`, a KPI the twin panel observes per window, or `tag:<tag>`, an
//! observation of the twin (live panel or recording, read by `utsim_quality::gemeo`). The
//! line keeps frozen phase II limits, taken from the baseline data, from the validated
//! model (the mean the twin simulates for the KPI ± 3 standard deviations) or typed.
//!
//! A reading charts the series against those limits with the Western Electric rules. When
//! a point after the last reading breaks a rule, the response plan signal of the line opens
//! its card on the board, with the stretch of the series in the "why". The mean of the
//! reading against the baseline gives the realised value of the linked saving (money per
//! unit of improvement, per month), entered as a month of the follow-up for the controller
//! to validate. The series can also be kept as a data set of origin Gêmeo for the analyses.

use utsim_quality::charts::{self, ChartData, ChartKind, Limits};
use utsim_quality::gemeo::Point;
use utsim_quality::rules::RuleSet;

use crate::dados::{DataOrigin, DatasetInput};
use crate::date::Date;
use crate::domain::Action;
use crate::domain::fields as df;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::savings::{self, Month};
use crate::state::State;
use crate::tools::plans::{self, CONTROL_ITEM};
use crate::tools::{Deliverable, ItemRef, item_ref, items};

pub use crate::tools::plans::fields;
pub use utsim_quality::gemeo::{self, TwinData};

/// Where the frozen limits of a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LimitsFrom {
    #[default]
    Manual,
    /// Phase I of the baseline data (I-MR, points with a cause left out).
    Data,
    /// What the validated model expects (the twin's simulated windows).
    Model,
}

impl LimitsFrom {
    pub fn as_str(self) -> &'static str {
        match self {
            LimitsFrom::Manual => "manual",
            LimitsFrom::Data => "dados",
            LimitsFrom::Model => "modelo",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "dados" => LimitsFrom::Data,
            "modelo" => LimitsFrom::Model,
            _ => LimitsFrom::Manual,
        }
    }
}

/// Which direction of the characteristic is an improvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Better {
    #[default]
    Higher,
    Lower,
}

impl Better {
    pub fn as_str(self) -> &'static str {
        match self {
            Better::Higher => "maior",
            Better::Lower => "menor",
        }
    }

    pub fn parse(s: &str) -> Self {
        if s == "menor" {
            Better::Lower
        } else {
            Better::Higher
        }
    }

    fn sign(self) -> f64 {
        match self {
            Better::Higher => 1.0,
            Better::Lower => -1.0,
        }
    }
}

/// What a control plan line watches in operation.
#[derive(Debug, Clone, PartialEq)]
pub struct Watch {
    pub line: Id,
    pub characteristic: String,
    /// `kpi:<id>`, `tag:<tag>` or empty (not watched).
    pub source: String,
    /// Centre, lower and upper limits.
    pub limits: Option<(f64, f64, f64)>,
    pub limits_from: LimitsFrom,
    pub response: Option<Id>,
    pub saving: Option<Id>,
    pub baseline: Option<f64>,
    pub per_unit: Option<f64>,
    pub better: Better,
    pub checked: Option<f64>,
}

impl Watch {
    pub fn read(i: &ItemRef) -> Self {
        let e = i.entity;
        let limits = match (
            e.number(fields::CENTER),
            e.number(fields::LCL),
            e.number(fields::UCL),
        ) {
            (Some(c), Some(l), Some(u)) => Some((c, l, u)),
            _ => None,
        };
        Watch {
            line: i.id,
            characteristic: i.text(fields::CHARACTERISTIC),
            source: i.text(fields::SOURCE).trim().to_string(),
            limits,
            limits_from: LimitsFrom::parse(&i.text(fields::LIMITS_FROM)),
            response: e.id(fields::RESPONSE),
            saving: e.id(fields::SAVING),
            baseline: e.number(fields::BASELINE),
            per_unit: e.number(fields::PER_UNIT),
            better: Better::parse(&i.text(fields::BETTER)),
            checked: e.number(fields::CHECKED),
        }
    }

    /// The name shown: the characteristic, or the source.
    pub fn name(&self) -> String {
        if self.characteristic.trim().is_empty() {
            self.source.clone()
        } else {
            self.characteristic.trim().to_string()
        }
    }
}

/// The lines of a control plan.
pub fn watches(state: &State, deliverable: Id) -> Vec<Watch> {
    items(state, deliverable, CONTROL_ITEM)
        .iter()
        .map(Watch::read)
        .collect()
}

/// The twin address of a control plan (panel or recordings).
pub fn twin_address(state: &State, deliverable: Id) -> String {
    state
        .entity(deliverable)
        .map(|e| e.text(fields::TWIN))
        .unwrap_or_default()
}

/// Edits that keep the twin address of a control plan.
pub fn twin_address_edits(deliverable: Id, address: &str) -> Vec<Edit> {
    vec![Edit::new(
        Deliverable::entity_ref(deliverable),
        fields::TWIN,
        address.trim(),
    )]
}

/// Edits that freeze the limits of a line.
pub fn limits_edits(line: Id, limits: (f64, f64, f64), from: LimitsFrom) -> Vec<Edit> {
    let r = item_ref(line);
    vec![
        Edit::new(r, fields::CENTER, limits.0),
        Edit::new(r, fields::LCL, limits.1),
        Edit::new(r, fields::UCL, limits.2),
        Edit::new(r, fields::LIMITS_FROM, from.as_str()),
    ]
}

fn fixed(l: &Limits) -> Option<(f64, f64, f64)> {
    match l {
        Limits::Fixed { center, lcl, ucl } => Some((*center, *lcl, *ucl)),
        _ => None,
    }
}

/// Limits from baseline values in time order: phase I of an individuals chart, leaving out
/// the points outside the limits (at most three rounds).
pub fn limits_from_values(values: &[f64]) -> Result<(f64, f64, f64), String> {
    let v: Vec<Vec<f64>> = values
        .iter()
        .filter(|x| x.is_finite())
        .map(|x| vec![*x])
        .collect();
    if v.len() < 2 {
        return Err("a linha de base precisa de ao menos dois valores".into());
    }
    let (ch, _) = charts::phase1_revised(
        ChartKind::IMr,
        &ChartData::Variables(v),
        &RuleSet::basic(),
        3,
    )?;
    let p = ch.panels.first().ok_or("carta sem painel")?;
    match (p.center.first(), p.lcl.first(), p.ucl.first()) {
        (Some(c), Some(l), Some(u)) => Ok((*c, *l, *u)),
        _ => Err("carta sem limites".into()),
    }
}

/// Limits from what the validated model expects of a KPI, as the twin panel gives it.
pub fn limits_from_model(data: &TwinData, source: &str) -> Option<(f64, f64, f64)> {
    data.model_limits(source).as_ref().and_then(fixed)
}

/// A reading of one line.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub points: Vec<Point>,
    pub limits: (f64, f64, f64),
    pub limits_from: LimitsFrom,
    /// Points that broke a rule: index and the rules, described.
    pub signals: Vec<(usize, String)>,
    /// Indices of `signals` after the last reading.
    pub new: Vec<usize>,
    pub mean: f64,
    /// Alerts of the twin for this KPI (model and plant drifted apart).
    pub alerts: Vec<String>,
}

impl Reading {
    pub fn last_t(&self) -> Option<f64> {
        self.points.last().map(|p| p.t)
    }
}

/// Charts the series of a line against its limits (or the model's, when the line has
/// none). Errors: no source, the series is not in the twin data, no limits.
pub fn check(w: &Watch, data: &TwinData) -> Result<Reading, String> {
    if w.source.is_empty() {
        return Err(format!("{}: sem fonte no gêmeo", w.name()));
    }
    let points = data
        .get(&w.source)
        .ok_or_else(|| format!("{}: série {} não encontrada no gêmeo", w.name(), w.source))?
        .to_vec();
    if points.is_empty() {
        return Err(format!("{}: série {} vazia", w.name(), w.source));
    }
    let (limits, limits_from) = match w.limits {
        Some(l) => (l, w.limits_from),
        None => (
            limits_from_model(data, &w.source).ok_or_else(|| {
                format!(
                    "{}: sem limites (congele os da linha de base ou os do modelo)",
                    w.name()
                )
            })?,
            LimitsFrom::Model,
        ),
    };
    let ch = charts::apply(
        &Limits::Fixed {
            center: limits.0,
            lcl: limits.1,
            ucl: limits.2,
        },
        &ChartData::Variables(points.iter().map(|p| vec![p.value]).collect()),
        &RuleSet::western_electric(),
        &[],
    )?;
    let mut signals: Vec<(usize, String)> = Vec::new();
    for v in ch.panels.iter().flat_map(|p| &p.violations) {
        let d = v.rule.describe();
        match signals.iter_mut().find(|(i, _)| *i == v.index) {
            Some((_, s)) if !s.contains(&d) => {
                s.push_str("; ");
                s.push_str(&d);
            }
            Some(_) => {}
            None => signals.push((v.index, d)),
        }
    }
    signals.sort_by_key(|(i, _)| *i);
    let last = points.last().map_or(f64::NEG_INFINITY, |p| p.t);
    // A twin started again counts its time from zero: everything is new then.
    let from = w
        .checked
        .filter(|c| *c <= last)
        .unwrap_or(f64::NEG_INFINITY);
    let new = signals
        .iter()
        .map(|(i, _)| *i)
        .filter(|i| points[*i].t > from)
        .collect();
    let mean = points.iter().map(|p| p.value).sum::<f64>() / points.len() as f64;
    let alerts = data
        .alerts_of(&w.source)
        .iter()
        .map(|a| format!("{} {} em t = {}: {}", a.level, a.test, num(a.t), a.message))
        .collect();
    Ok(Reading {
        points,
        limits,
        limits_from,
        signals,
        new,
        mean,
        alerts,
    })
}

fn num(x: f64) -> String {
    let s = format!("{x:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

/// The "why" of the card: the source, the rules broken and the last points of the series.
pub fn excerpt(w: &Watch, r: &Reading) -> String {
    let mut s = format!(
        "Sinal no gêmeo: {} ({}), limites {} | {} | {}.",
        w.name(),
        w.source,
        num(r.limits.1),
        num(r.limits.0),
        num(r.limits.2)
    );
    for i in &r.new {
        if let Some((_, rules)) = r.signals.iter().find(|(j, _)| j == i) {
            let p = r.points[*i];
            s.push_str(&format!(" t = {}: {} ({rules}).", num(p.t), num(p.value)));
        }
    }
    let first = r.points.len().saturating_sub(10);
    let tail: Vec<String> = r.points[first..].iter().map(|p| num(p.value)).collect();
    s.push_str(&format!(" Últimos pontos: {}.", tail.join("; ")));
    s
}

/// Edits of a reading: the card of the response signal when a new point broke a rule (one
/// card per reading), and the time checked so far.
pub fn reading_edits(state: &State, project: Id, w: &Watch, r: &Reading, card: Id) -> Vec<Edit> {
    let mut e = Vec::new();
    if let (false, Some(signal)) = (r.new.is_empty(), w.response) {
        let c = plans::signal_card(state, project, signal, card);
        if !c.is_empty() {
            e.extend(c);
            e.push(Edit::new(
                Action::entity_ref(card),
                df::action::WHY,
                excerpt(w, r),
            ));
        }
    }
    if let Some(t) = r.last_t() {
        e.push(Edit::new(item_ref(w.line), fields::CHECKED, t));
    }
    if w.limits.is_none() {
        e.extend(limits_edits(w.line, r.limits, r.limits_from));
    }
    e
}

/// The realised value per month the reading shows: the mean against the baseline, in the
/// direction of the improvement, times the money per unit. `None` without a baseline or a
/// value per unit.
pub fn realized_value(w: &Watch, r: &Reading) -> Option<f64> {
    let (b, k) = (w.baseline?, w.per_unit?);
    Some((r.mean - b) * w.better.sign() * k)
}

/// Edits that enter the realised value of the reading as a month of the linked saving,
/// with the calculation in the note. `None` when the line has no saving, baseline or value
/// per unit.
pub fn realized_edits(
    state: &State,
    w: &Watch,
    r: &Reading,
    month: Month,
    new_id: Id,
) -> Option<Vec<Edit>> {
    let saving = w.saving?;
    let value = realized_value(w, r)?;
    let entry = savings::entries(state, saving)
        .into_iter()
        .find(|e| e.month == Some(month))
        .map_or(new_id, |e| e.id);
    let mut e = savings::record_month(state, saving, month, value, new_id);
    e.push(Edit::new(
        EntityRef::new(Kind::SavingEntry, entry),
        savings::fields::NOTE,
        format!(
            "Realizado pelo gêmeo: {} ({}), média {} em {} pontos contra a linha de base {}, \
             {} por unidade de melhoria ao mês.",
            w.name(),
            w.source,
            num(r.mean),
            r.points.len(),
            num(w.baseline.unwrap_or_default()),
            num(w.per_unit.unwrap_or_default())
        ),
    ));
    Some(e)
}

/// The data set of origin Gêmeo for the series of a reading, once its CSV
/// ([`gemeo::csv`]) is saved under `file` with `hash`.
pub fn dataset(
    w: &Watch,
    r: &Reading,
    data: &TwinData,
    file: &str,
    hash: &str,
    today: Date,
) -> DatasetInput {
    let col = w
        .source
        .split_once(':')
        .map_or(w.source.as_str(), |(_, s)| s);
    DatasetInput {
        name: format!("{} (gêmeo)", w.name()),
        origin: DataOrigin::Twin,
        file: file.to_string(),
        hash: hash.to_string(),
        columns: vec!["t".into(), col.replace(';', ",")],
        rows: r.points.len(),
        model_name: data.model.clone(),
        date: Some(today),
        note: format!("{} lido de {}", w.source, data.origin),
        ..DatasetInput::default()
    }
}

/// The CSV of a reading's series.
pub fn csv(w: &Watch, r: &Reading) -> String {
    let col = w
        .source
        .split_once(':')
        .map_or(w.source.as_str(), |(_, s)| s);
    gemeo::csv(col, &r.points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, SavingCategory};
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;
    use serde_json::Value;
    use std::collections::BTreeMap;

    fn twin(values: &[f64]) -> TwinData {
        let mut series = BTreeMap::new();
        series.insert(
            "kpi:vazao".to_string(),
            values
                .iter()
                .enumerate()
                .map(|(i, v)| Point {
                    t: (i + 1) as f64 * 10.0,
                    value: *v,
                })
                .collect(),
        );
        let mut expected = BTreeMap::new();
        expected.insert(
            "kpi:vazao".to_string(),
            gemeo::ModelKpi {
                mean: 12.0,
                sd: 0.5,
                n: 30,
            },
        );
        TwinData {
            origin: "http://127.0.0.1:8080/api/gemeo".into(),
            model: "linha".into(),
            series,
            expected,
            alerts: Vec::new(),
        }
    }

    struct Setup {
        f: Fixture,
        line: Id,
        signal: Id,
        saving: Id,
    }

    fn setup() -> Setup {
        let mut f = Fixture::new();
        let resp = f.deliverable(Tool::ResponsePlan);
        let signal = f.item(
            resp,
            plans::RESPONSE_ITEM,
            None,
            vec![
                (fields::SIGNAL, "Vazão fora de controle".into()),
                (fields::IMMEDIATE, "Conferir o gargalo".into()),
            ],
        );
        let saving = f.id();
        let p = f.project;
        f.commit(savings::new_saving(
            p,
            saving,
            "Mais peças por hora",
            SavingCategory::Hard,
            1000.0,
            savings::Source::Simulation,
        ));
        let plan = f.deliverable(Tool::ControlPlan);
        let line = f.item(
            plan,
            CONTROL_ITEM,
            None,
            vec![
                (fields::CHARACTERISTIC, "Vazão da linha".into()),
                (fields::SOURCE, "kpi:vazao".into()),
                (fields::RESPONSE, signal.to_string().into()),
                (fields::SAVING, saving.to_string().into()),
                (fields::BASELINE, Value::from(11.0)),
                (fields::PER_UNIT, Value::from(500.0)),
            ],
        );
        Setup {
            f,
            line,
            signal,
            saving,
        }
    }

    fn watch(s: &Setup) -> Watch {
        Watch::read(&ItemRef {
            id: s.line,
            entity: s.f.state.entity(s.line).unwrap(),
        })
    }

    #[test]
    fn limits_from_the_model_and_a_point_out_opens_the_card_once() {
        let mut s = setup();
        let w = watch(&s);
        assert_eq!(w.limits, None);
        let r = check(&w, &twin(&[12.0, 12.2, 11.9, 14.0])).unwrap();
        assert_eq!(r.limits, (12.0, 10.5, 13.5));
        assert_eq!(r.limits_from, LimitsFrom::Model);
        assert_eq!(r.new, [3]);
        let card = s.f.id();
        let p = s.f.project;
        let e = reading_edits(&s.f.state, p, &w, &r, card);
        s.f.commit(e);
        let a = Action::read(card, s.f.state.entity(card).unwrap());
        assert_eq!(a.title, "Vazão fora de controle: Conferir o gargalo");
        assert_eq!(a.link, Some(s.signal));
        assert_eq!(a.column, Column::Todo);
        assert!(a.why.contains("t = 40: 14"), "{}", a.why);
        // The limits are frozen and the time checked is kept.
        let w = watch(&s);
        assert_eq!(w.limits, Some((12.0, 10.5, 13.5)));
        assert_eq!(w.limits_from, LimitsFrom::Model);
        assert_eq!(w.checked, Some(40.0));
        // The same reading again opens nothing new.
        let r = check(&w, &twin(&[12.0, 12.2, 11.9, 14.0])).unwrap();
        assert!(r.new.is_empty());
        let card2 = s.f.id();
        let e = reading_edits(&s.f.state, p, &w, &r, card2);
        assert!(e.iter().all(|e| e.entity.id != card2));
        // A twin started again: its points are new.
        let r = check(&w, &twin(&[14.0])).unwrap();
        assert_eq!(r.new, [0]);
    }

    #[test]
    fn limits_from_baseline_values() {
        let v = [10.0, 10.4, 9.8, 10.1, 9.9, 10.2, 10.0, 9.7, 10.3, 10.0];
        let (c, l, u) = limits_from_values(&v).unwrap();
        assert!((c - 10.04).abs() < 1e-9);
        assert!(l < 9.7 && u > 10.4);
        assert!(limits_from_values(&[1.0]).is_err());
    }

    #[test]
    fn realised_month_of_the_linked_saving() {
        let mut s = setup();
        let w = watch(&s);
        let r = check(&w, &twin(&[12.0, 12.4, 11.6, 12.0])).unwrap();
        assert_eq!(realized_value(&w, &r), Some(500.0));
        let m = Month {
            year: 2026,
            month: 11,
        };
        let id = s.f.id();
        let e = realized_edits(&s.f.state, &w, &r, m, id).unwrap();
        s.f.commit(e);
        let en = savings::entries(&s.f.state, s.saving);
        assert_eq!(en.len(), 1);
        assert_eq!(en[0].value, 500.0);
        assert!(
            en[0].note.contains("média 12 em 4 pontos"),
            "{}",
            en[0].note
        );
        // Lower is better turns the sign.
        let mut w2 = w.clone();
        w2.better = Better::Lower;
        assert_eq!(realized_value(&w2, &r), Some(-500.0));
    }

    #[test]
    fn errors_without_source_series_or_limits() {
        let s = setup();
        let mut w = watch(&s);
        let d = twin(&[12.0]);
        w.source = String::new();
        assert!(check(&w, &d).unwrap_err().contains("sem fonte"));
        w.source = "tag:nada".into();
        assert!(check(&w, &d).unwrap_err().contains("não encontrada"));
        w.source = "kpi:vazao".into();
        let mut d2 = d.clone();
        d2.expected.clear();
        assert!(check(&w, &d2).unwrap_err().contains("sem limites"));
    }

    #[test]
    fn data_set_of_origin_twin() {
        let s = setup();
        let w = watch(&s);
        let d = twin(&[12.0, 13.0]);
        let r = check(&w, &d).unwrap();
        assert_eq!(csv(&w, &r), "t;vazao\n10;12\n20;13\n");
        let input = dataset(&w, &r, &d, "a.csv", "h", Date::new(2026, 11, 9).unwrap());
        assert_eq!(input.origin, DataOrigin::Twin);
        assert_eq!(input.columns, ["t", "vazao"]);
        assert_eq!(input.rows, 2);
        assert_eq!(input.name, "Vazão da linha (gêmeo)");
    }
}
