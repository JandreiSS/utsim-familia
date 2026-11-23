//! The program dashboards (`docs/plano_lean_seis_sigma.md`, 8.7): the MBB view (every
//! indicator, down to the project and the person) and the leadership view (at most six
//! indicators with target and trend, without personal data of training or coaching).
//!
//! Everything is computed from the project records, the portfolio ideas and the program
//! state, never from the project journals themselves, so it opens fast with hundreds of
//! projects (LSS-RNF-14). Training projects never count (LSS-RNF-15). The screens show,
//! next to each indicator, its rule and the instant of the data (LSS-RNF-16).

use std::collections::{BTreeMap, BTreeSet};

use crate::date::Date;
use crate::designacao::Load;
use crate::domain::{Approach, SavingStatus};
use crate::id::Id;
use crate::mentoria::CompetenceMap;
use crate::programa::{
    self, BeltLevel, IdeaRecord, Member, Metric, Opportunity, ProjectRecord, Stage, Unit,
};
use crate::revisao::{self, Review, ReviewItem};
use crate::savings::Category;
use crate::state::State;

/// Filters of both views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Filter {
    pub unit: Option<Id>,
    pub year: Option<i32>,
}

impl Filter {
    fn unit_ok(&self, u: Option<Id>) -> bool {
        self.unit.is_none() || self.unit == u
    }
}

/// Percentile `p` (0 to 1) with linear interpolation between ranks; `None` when empty.
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let pos = p.clamp(0.0, 1.0) * (v.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    Some(v[lo] + (v[hi] - v[lo]) * (pos - lo as f64))
}

// --- Pipeline -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Funnel {
    /// Count per stage and unit.
    pub by_unit: BTreeMap<(Stage, Option<Id>), usize>,
    pub totals: BTreeMap<Stage, usize>,
    /// Of the items that reached a stage, the fraction that reached the next one.
    pub conversion: Vec<(Stage, Stage, f64)>,
    /// Mean days in the current stage (ideas not yet opened).
    pub mean_age: BTreeMap<Stage, f64>,
}

/// The pipeline across the portfolios.
pub fn funnel(
    ideas: &[IdeaRecord],
    opps: &[Opportunity],
    projects: &[ProjectRecord],
    today: Date,
    f: &Filter,
) -> Funnel {
    let mut out = Funnel::default();
    let mut ages: BTreeMap<Stage, Vec<f64>> = BTreeMap::new();
    let mut seen_projects: BTreeSet<Id> = BTreeSet::new();
    let mut stages: Vec<Stage> = Vec::new();
    for i in ideas.iter().filter(|i| f.unit_ok(i.unit)) {
        let pr = programa::project_of(projects, i);
        if pr.is_some_and(|p| p.training) {
            continue;
        }
        let Some(s) = programa::stage(i, programa::opportunity_of(opps, i.id), pr) else {
            continue;
        };
        if let Some(p) = pr {
            seen_projects.insert(p.id);
        }
        *out.by_unit.entry((s, i.unit)).or_default() += 1;
        stages.push(s);
        if s < Stage::Running {
            if let Some(since) = i.since {
                ages.entry(s)
                    .or_default()
                    .push(since.days_until(today).max(0) as f64);
            }
        }
    }
    // Projects without an idea (opened directly) enter at their stage.
    for p in projects
        .iter()
        .filter(|p| !p.training && f.unit_ok(p.unit) && !seen_projects.contains(&p.id))
    {
        let s = match p.status {
            crate::domain::ProjectStatus::Closed => Stage::Closed,
            crate::domain::ProjectStatus::Cancelled | crate::domain::ProjectStatus::Archived => {
                continue;
            }
            _ => Stage::Running,
        };
        *out.by_unit.entry((s, p.unit)).or_default() += 1;
        stages.push(s);
    }
    for s in &stages {
        *out.totals.entry(*s).or_default() += 1;
    }
    for w in Stage::ALL.windows(2) {
        let reached = stages.iter().filter(|s| **s >= w[0]).count();
        let next = stages.iter().filter(|s| **s >= w[1]).count();
        if reached > 0 {
            out.conversion
                .push((w[0], w[1], next as f64 / reached as f64));
        }
    }
    out.mean_age = ages
        .into_iter()
        .map(|(s, v)| (s, v.iter().sum::<f64>() / v.len() as f64))
        .collect();
    out
}

// --- Cycle time per phase ---------------------------------------------------------------

/// Default target days per phase of the flow model, when the program sets none
/// ([`program_target_days`]).
pub fn target_days(phase: &str) -> Option<f64> {
    match phase {
        "Definir" => Some(30.0),
        "Medir" => Some(45.0),
        "Analisar" => Some(45.0),
        "Melhorar" | "Desenhar" => Some(60.0),
        "Controlar" | "Verificar" => Some(60.0),
        _ => None,
    }
}

/// The field of the program entity with the target days of a phase (`meta_dias_medir`).
pub fn target_field(phase: &str) -> String {
    let key: String = phase
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("meta_dias_{key}")
}

/// The target days of a phase in a program: the MBB's own value (above zero), else the
/// default of [`target_days`].
pub fn program_target_days(state: &State, program: Id, phase: &str) -> Option<f64> {
    state
        .entity(program)
        .and_then(|e| e.number(&target_field(phase)))
        .filter(|v| v.is_finite() && *v > 0.0)
        .or_else(|| target_days(phase))
}

/// Every phase name of the approaches, once each, in the order of the approaches.
pub fn all_phases() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for a in Approach::ALL {
        for p in a.phases() {
            if !v.contains(p) {
                v.push(p);
            }
        }
    }
    v
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhaseCycle {
    pub approach: Approach,
    pub phase_index: usize,
    pub phase: String,
    pub samples: Vec<f64>,
    pub median: Option<f64>,
    pub p80: Option<f64>,
    pub target: Option<f64>,
}

/// Days of each phase: from the approval of the previous tollgate (the opening of the
/// project for the first phase) to the approval of the phase's tollgate. Per approach and
/// phase; `level_of` gives the level of the belt to filter (`level`), and `target` the
/// target days of a phase (usually [`program_target_days`]).
pub fn cycle_times(
    projects: &[ProjectRecord],
    f: &Filter,
    level: Option<BeltLevel>,
    level_of: impl Fn(Id) -> Option<BeltLevel>,
    target: impl Fn(&str) -> Option<f64>,
) -> Vec<PhaseCycle> {
    let mut m: BTreeMap<(u8, usize), (Approach, String, Vec<f64>)> = BTreeMap::new();
    let order = |a: Approach| Approach::ALL.iter().position(|x| *x == a).unwrap_or(0) as u8;
    for p in projects.iter().filter(|p| !p.training && f.unit_ok(p.unit)) {
        if level.is_some() && !p.belts.iter().any(|b| level_of(*b) == level) {
            continue;
        }
        let mut start = p.opened_on;
        for t in &p.tollgates {
            let Some(end) = t.approved_on else {
                break;
            };
            if let Some(s) = start {
                if f.year.is_none_or(|y| end.year == y) {
                    m.entry((order(p.approach), t.phase_index))
                        .or_insert_with(|| (p.approach, t.phase_name.clone(), Vec::new()))
                        .2
                        .push(s.days_until(end).max(0) as f64);
                }
            }
            start = Some(end);
        }
    }
    m.into_iter()
        .map(|((_, i), (a, name, v))| PhaseCycle {
            approach: a,
            phase_index: i,
            median: percentile(&v, 0.5),
            p80: percentile(&v, 0.8),
            target: target(&name),
            phase: name,
            samples: v,
        })
        .collect()
}

// --- Savings ----------------------------------------------------------------------------

/// Savings by category and state, by unit (all years: estimated, forecast and realised;
/// the validated column of the year when a year is filtered). The official total is only
/// the validated one.
pub fn savings_by_state(
    projects: &[ProjectRecord],
    f: &Filter,
) -> BTreeMap<(Option<Id>, Category, SavingStatus), f64> {
    let mut m = BTreeMap::new();
    for p in projects.iter().filter(|p| !p.training && f.unit_ok(p.unit)) {
        for c in [Category::Hard, Category::Soft] {
            for s in [
                SavingStatus::Estimated,
                SavingStatus::Forecast,
                SavingStatus::Realized,
            ] {
                *m.entry((p.unit, c, s)).or_insert(0.0) += p.totals.get(c, s);
            }
            let v = match f.year {
                Some(y) => p.validated(y, c),
                None => p
                    .validated_by_year
                    .iter()
                    .filter(|((_, cc), _)| *cc == c)
                    .map(|(_, v)| v)
                    .sum(),
            };
            *m.entry((p.unit, c, SavingStatus::Validated)).or_insert(0.0) += v;
        }
    }
    m
}

/// The official total: validated hard plus soft.
pub fn official_total(m: &BTreeMap<(Option<Id>, Category, SavingStatus), f64>) -> f64 {
    m.iter()
        .filter(|((_, _, s), _)| *s == SavingStatus::Validated)
        .map(|(_, v)| v)
        .sum()
}

// --- Belts by area ----------------------------------------------------------------------

/// Validity of a certification by default (decision MB5).
pub const CERTIFICATION_YEARS: i32 = 3;
/// Warning window before a certification expires, days.
pub const EXPIRY_WARNING_DAYS: i64 = 90;

/// The day a certification expires.
pub fn expiry(certified: Date) -> Date {
    Date::new(
        certified.year + CERTIFICATION_YEARS,
        certified.month,
        certified.day,
    )
    .unwrap_or_else(|| certified.add_days(365 * i64::from(CERTIFICATION_YEARS)))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeltRow {
    pub unit: Option<Id>,
    /// Belts in a running project, by level.
    pub active: BTreeMap<BeltLevel, usize>,
    pub trained: BTreeMap<BeltLevel, usize>,
    pub certified: BTreeMap<BeltLevel, usize>,
    /// Belts (green or above) per 100 people.
    pub coverage: Option<f64>,
    /// Certifications expiring in the warning window, or expired.
    pub expiring: usize,
}

pub fn belts_by_unit(
    units: &[Unit],
    members: &[Member],
    projects: &[ProjectRecord],
    today: Date,
) -> Vec<BeltRow> {
    let active: BTreeSet<Id> = projects
        .iter()
        .filter(|p| !p.training && p.is_running())
        .flat_map(|p| p.belts.iter().copied())
        .collect();
    let mut keys: Vec<Option<Id>> = units.iter().map(|u| Some(u.id)).collect();
    if members.iter().any(|m| m.is_belt() && m.unit.is_none()) {
        keys.push(None);
    }
    keys.into_iter()
        .map(|u| {
            let mut r = BeltRow {
                unit: u,
                ..BeltRow::default()
            };
            let ms: Vec<&Member> = members
                .iter()
                .filter(|m| m.unit == u && !m.anonymised)
                .collect();
            for m in &ms {
                let Some(l) = m.level else { continue };
                if active.contains(&m.id) {
                    *r.active.entry(l).or_default() += 1;
                }
                if m.trained_year.is_some() {
                    *r.trained.entry(l).or_default() += 1;
                }
                if let Some(c) = m.certified_on {
                    *r.certified.entry(l).or_default() += 1;
                    if today.days_until(expiry(c)) <= EXPIRY_WARNING_DAYS {
                        r.expiring += 1;
                    }
                }
            }
            let people = units
                .iter()
                .find(|x| Some(x.id) == u)
                .and_then(|x| x.headcount);
            let belts = ms
                .iter()
                .filter(|m| m.level.is_some_and(|l| l >= BeltLevel::Green))
                .count();
            r.coverage = people.map(|p| 100.0 * belts as f64 / p);
            r
        })
        .collect()
}

// --- Tool usage -------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolUsage {
    /// Tool key → phase index → projects using it.
    pub by_tool: BTreeMap<String, BTreeMap<usize, usize>>,
    /// Projects per tool (any phase).
    pub projects: BTreeMap<String, usize>,
    /// Tools of the list never in an approved deliverable of a real project.
    pub never_applied: Vec<String>,
    /// Belts at `aplicou` or `demonstrou` per tool (competence map).
    pub belts: BTreeMap<String, usize>,
}

pub fn tool_usage(
    projects: &[ProjectRecord],
    tools: &[String],
    map: &CompetenceMap,
    f: &Filter,
) -> ToolUsage {
    let mut u = ToolUsage::default();
    let mut applied: BTreeSet<String> = BTreeSet::new();
    for p in projects.iter().filter(|p| !p.training && f.unit_ok(p.unit)) {
        let mut seen: BTreeSet<(String, usize)> = BTreeSet::new();
        let mut seen_tool: BTreeSet<String> = BTreeSet::new();
        for t in &p.tools {
            let key = crate::mentoria::tool_key(t.tool);
            if t.approved {
                applied.insert(key.clone());
            }
            if seen.insert((key.clone(), t.phase_index)) {
                *u.by_tool
                    .entry(key.clone())
                    .or_default()
                    .entry(t.phase_index)
                    .or_default() += 1;
            }
            if seen_tool.insert(key.clone()) {
                *u.projects.entry(key).or_default() += 1;
            }
        }
    }
    // The link with the competence map: belts at `aplicou` or above per tool.
    for ((_, tool), cell) in &map.cells {
        if cell.level >= Some(crate::mentoria::Level::Applied) {
            *u.belts.entry(tool.clone()).or_default() += 1;
        }
    }
    u.never_applied = tools
        .iter()
        .filter(|t| !applied.contains(*t))
        .cloned()
        .collect();
    u
}

// --- Overdue tollgates ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overdue {
    pub items: Vec<ReviewItem>,
    pub by_unit: BTreeMap<Option<Id>, usize>,
    pub by_belt: BTreeMap<Id, usize>,
    /// Submitted tollgates still waiting for the MBB.
    pub pending_reviews: usize,
}

pub fn overdue(projects: &[ProjectRecord], reviews: &[Review], today: Date, f: &Filter) -> Overdue {
    let filtered: Vec<ProjectRecord> = projects
        .iter()
        .filter(|p| f.unit_ok(p.unit))
        .cloned()
        .collect();
    let items = revisao::overdue_tollgates(&filtered, today);
    let mut o = Overdue {
        pending_reviews: revisao::queue(&filtered, reviews, today).len(),
        ..Overdue::default()
    };
    for i in &items {
        *o.by_unit.entry(i.unit).or_default() += 1;
        for b in &i.belts {
            *o.by_belt.entry(*b).or_default() += 1;
        }
    }
    o.items = items;
    o
}

// --- Workload ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LevelLoad {
    pub people: usize,
    pub hours: f64,
    pub capacity: f64,
    pub overloaded: usize,
}

/// Hours committed against capacity, by level (people without a level: champions and
/// others, under `None`).
pub fn workload(loads: &[Load]) -> BTreeMap<Option<BeltLevel>, LevelLoad> {
    let mut m: BTreeMap<Option<BeltLevel>, LevelLoad> = BTreeMap::new();
    for l in loads {
        let e = m.entry(l.level).or_default();
        e.people += 1;
        e.hours += l.hours;
        e.capacity += l.capacity;
        if l.over_hours() || l.over_projects() {
            e.overloaded += 1;
        }
    }
    m
}

// --- Leadership view --------------------------------------------------------------------

/// The six indicators of the leadership view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kpi {
    ValidatedSavings,
    ProjectsClosed,
    BeltsCertified,
    Coverage,
    OpenPipeline,
    OverdueTollgates,
}

impl Kpi {
    pub const ALL: [Kpi; 6] = [
        Kpi::ValidatedSavings,
        Kpi::ProjectsClosed,
        Kpi::BeltsCertified,
        Kpi::Coverage,
        Kpi::OpenPipeline,
        Kpi::OverdueTollgates,
    ];

    /// Whether lower is better.
    pub fn lower_is_better(self) -> bool {
        self == Kpi::OverdueTollgates
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KpiValue {
    pub kpi: Kpi,
    pub value: f64,
    pub target: Option<f64>,
    /// The value of the previous year, for the trend (`None`: not comparable).
    pub previous: Option<f64>,
}

impl KpiValue {
    /// +1 better than last year, -1 worse, 0 same or unknown.
    pub fn trend(&self) -> i8 {
        let Some(p) = self.previous else { return 0 };
        let d = self.value - p;
        let d = if self.kpi.lower_is_better() { -d } else { d };
        if d > 1e-9 {
            1
        } else if d < -1e-9 {
            -1
        } else {
            0
        }
    }
}

/// Everything the views need, read once per frame or refresh.
pub struct Inputs<'a> {
    pub state: &'a State,
    pub program: Id,
    pub projects: &'a [ProjectRecord],
    pub ideas: &'a [IdeaRecord],
    pub members: &'a [Member],
    pub opportunities: &'a [Opportunity],
    pub reviews: &'a [Review],
    pub today: Date,
}

/// The leadership view of a year: aggregates only, no person.
pub fn leadership(i: &Inputs<'_>, year: i32, f: &Filter) -> Vec<KpiValue> {
    let units = programa::units(i.state, i.program);
    let ts = programa::targets(i.state, i.program);
    let target = |m: Metric| -> Option<f64> {
        let v: Vec<f64> = ts
            .iter()
            .filter(|t| t.year == year && t.metric == m && f.unit_ok(t.unit))
            .map(|t| t.value)
            .collect();
        (!v.is_empty()).then(|| v.iter().sum())
    };
    let actual = |m: Metric, y: i32| -> f64 {
        programa::actual(m, None, f.unit, y, i.projects, i.members, &units).0
    };
    let savings = |y: i32| actual(Metric::HardValidated, y) + actual(Metric::SoftValidated, y);
    let savings_target = match (target(Metric::HardValidated), target(Metric::SoftValidated)) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    };
    let fun = funnel(i.ideas, i.opportunities, i.projects, i.today, f);
    let open_pipeline: usize = [
        Stage::Idea,
        Stage::Screening,
        Stage::Selection,
        Stage::Opening,
    ]
    .iter()
    .map(|s| fun.totals.get(s).copied().unwrap_or(0))
    .sum();
    let late = overdue(i.projects, i.reviews, i.today, f).items.len();
    vec![
        KpiValue {
            kpi: Kpi::ValidatedSavings,
            value: savings(year),
            target: savings_target,
            previous: Some(savings(year - 1)),
        },
        KpiValue {
            kpi: Kpi::ProjectsClosed,
            value: actual(Metric::ProjectsClosed, year),
            target: target(Metric::ProjectsClosed),
            previous: Some(actual(Metric::ProjectsClosed, year - 1)),
        },
        KpiValue {
            kpi: Kpi::BeltsCertified,
            value: actual(Metric::BeltsCertified, year),
            target: target(Metric::BeltsCertified),
            previous: Some(actual(Metric::BeltsCertified, year - 1)),
        },
        KpiValue {
            kpi: Kpi::Coverage,
            value: actual(Metric::Coverage, year),
            target: if f.unit.is_some() {
                target(Metric::Coverage)
            } else {
                None
            },
            previous: None,
        },
        KpiValue {
            kpi: Kpi::OpenPipeline,
            value: open_pipeline as f64,
            target: None,
            previous: None,
        },
        KpiValue {
            kpi: Kpi::OverdueTollgates,
            value: late as f64,
            target: Some(0.0),
            previous: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::designacao::{self, Limits};
    use crate::domain::ProjectStatus;
    use crate::programa::testkit::*;
    use crate::programa::{TollgateRecord, ToolUse};
    use crate::tollgate::TollgateStatus;
    use crate::tools::Tool;

    #[test]
    fn program_sets_its_own_target_days() {
        let mut t = T::new();
        let p = t.program;
        assert_eq!(target_field("Pré-evento"), "meta_dias_pré_evento");
        assert_eq!(program_target_days(&t.s, p, "Medir"), Some(45.0));
        t.set(crate::op::Kind::Program, p, &target_field("Medir"), 30.0);
        assert_eq!(program_target_days(&t.s, p, "Medir"), Some(30.0));
        // Zero or less falls back to the default; a phase with no default has none.
        t.set(crate::op::Kind::Program, p, &target_field("Medir"), 0.0);
        assert_eq!(program_target_days(&t.s, p, "Medir"), Some(45.0));
        assert_eq!(program_target_days(&t.s, p, "Evento"), None);
        t.set(crate::op::Kind::Program, p, &target_field("Evento"), 5.0);
        assert_eq!(program_target_days(&t.s, p, "Evento"), Some(5.0));
        let all = all_phases();
        assert!(all.contains(&"Definir") && all.contains(&"Desenhar"));
        assert_eq!(all.iter().filter(|x| **x == "Definir").count(), 1);
    }

    fn tg(
        i: usize,
        name: &str,
        status: TollgateStatus,
        approved: Option<Date>,
        planned: Option<Date>,
    ) -> TollgateRecord {
        TollgateRecord {
            id: Id::v7(1000 + i as u64, [i as u8; 10]),
            phase: Id::v7(2000 + i as u64, [i as u8; 10]),
            phase_name: name.into(),
            phase_index: i,
            status,
            round: 1,
            planned,
            submitted_on: None,
            approved_on: approved,
            verified: vec![],
            frozen_hash: String::new(),
            changed_after_approval: false,
        }
    }

    #[test]
    fn percentiles() {
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[3.0], 0.8), Some(3.0));
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.5), Some(3.0));
        assert!((percentile(&[10.0, 20.0, 30.0, 40.0, 50.0], 0.8).unwrap() - 42.0).abs() < 1e-12);
    }

    /// A synthetic program with known counts: every indicator against the expected count,
    /// with a training project present and ignored.
    #[test]
    fn synthetic_program_indicators() {
        let mut t = T::new();
        let p = t.program;
        let (u1, u2) = (t.id(), t.id());
        t.commit(programa::add_unit(p, u1, "Planta 1"));
        t.commit(programa::add_unit(p, u2, "Planta 2"));
        t.set(
            crate::Kind::BusinessUnit,
            u1,
            programa::fields::HEADCOUNT,
            100,
        );
        let ana = t.person("Ana", Some(BeltLevel::Green), Some(u1));
        t.set(crate::Kind::Person, ana, programa::fields::HOURS_WEEK, 8);
        t.set(
            crate::Kind::Person,
            ana,
            programa::fields::CERTIFIED_ON,
            "2023-12-01",
        );
        let bia = t.person("Bia", Some(BeltLevel::Black), Some(u2));
        t.set(
            crate::Kind::Person,
            bia,
            programa::fields::TRAINED_YEAR,
            2026,
        );
        let ms = programa::members(&t.s);
        let today = d(2026, 11, 3);

        // Projects: A (u1, DMAIC, Definir and Medir approved), B (u2, closed), training.
        let mut a = project(t.id(), Some(u1), ProjectStatus::Running);
        a.approach = Approach::Dmaic;
        a.belts = vec![ana];
        a.opened_on = Some(d(2026, 1, 1));
        a.tollgates = vec![
            tg(
                0,
                "Definir",
                TollgateStatus::Approved,
                Some(d(2026, 1, 31)),
                None,
            ),
            tg(
                1,
                "Medir",
                TollgateStatus::Approved,
                Some(d(2026, 3, 2)),
                None,
            ),
            tg(
                2,
                "Analisar",
                TollgateStatus::Open,
                None,
                Some(d(2026, 10, 1)),
            ),
        ];
        a.tools = vec![
            ToolUse {
                tool: Tool::Sipoc,
                deliverable: t.id(),
                phase_index: 0,
                approved: true,
            },
            ToolUse {
                tool: Tool::Fmea,
                deliverable: t.id(),
                phase_index: 2,
                approved: false,
            },
        ];
        a.task_hours = vec![(ana, 6.0)];
        a.totals
            .by
            .insert((Category::Hard, SavingStatus::Forecast), 50_000.0);
        let mut b = project(t.id(), Some(u2), ProjectStatus::Closed);
        b.approach = Approach::Dmaic;
        b.belts = vec![bia];
        b.opened_on = Some(d(2026, 1, 1));
        b.closed_on = Some(d(2026, 9, 1));
        b.tollgates = vec![tg(
            0,
            "Definir",
            TollgateStatus::Approved,
            Some(d(2026, 1, 21)),
            None,
        )];
        b.validated_by_year.insert((2026, Category::Hard), 30_000.0);
        b.validated_by_year.insert((2025, Category::Hard), 10_000.0);
        b.tools = vec![ToolUse {
            tool: Tool::Sipoc,
            deliverable: t.id(),
            phase_index: 0,
            approved: true,
        }];
        let mut tr = project(t.id(), Some(u1), ProjectStatus::Running);
        tr.training = true;
        tr.belts = vec![ana];
        tr.opened_on = Some(d(2026, 1, 1));
        tr.tollgates = vec![tg(
            0,
            "Definir",
            TollgateStatus::Submitted,
            None,
            Some(d(2026, 2, 1)),
        )];
        tr.tools = vec![ToolUse {
            tool: Tool::Pareto,
            deliverable: t.id(),
            phase_index: 0,
            approved: true,
        }];
        tr.validated_by_year.insert((2026, Category::Hard), 1e9);
        tr.task_hours = vec![(ana, 40.0)];
        let ps = vec![a.clone(), b.clone(), tr];

        // Ideas: two in u1 (idea, screening sent to committee), one opened as A.
        let i1 = idea(t.id(), Some(u1), ProjectStatus::Idea, 30.0);
        let i2 = idea(t.id(), Some(u1), ProjectStatus::Screening, 60.0);
        let mut i3 = idea(t.id(), Some(u1), ProjectStatus::Running, 80.0);
        i3.project = Some(a.id);
        let oid = t.id();
        let e = programa::send_to_committee(p, oid, &i2, &[], t.me, today).unwrap();
        t.commit(e);
        let opps = programa::opportunities(&t.s, p);
        let ideas = vec![i1, i2, i3];

        let fun = funnel(&ideas, &opps, &ps, today, &Filter::default());
        assert_eq!(fun.totals.get(&Stage::Idea), Some(&1));
        assert_eq!(fun.totals.get(&Stage::Selection), Some(&1));
        assert_eq!(
            fun.totals.get(&Stage::Running),
            Some(&1),
            "A, from its idea"
        );
        assert_eq!(
            fun.totals.get(&Stage::Closed),
            Some(&1),
            "B, opened directly"
        );
        assert_eq!(fun.by_unit.get(&(Stage::Closed, Some(u2))), Some(&1));
        let conv = |s| fun.conversion.iter().find(|c| c.0 == s).unwrap().2;
        assert!((conv(Stage::Idea) - 3.0 / 4.0).abs() < 1e-12);
        assert!((conv(Stage::Running) - 0.5).abs() < 1e-12);
        assert_eq!(fun.mean_age[&Stage::Idea], 33.0);
        let only_u2 = funnel(
            &ideas,
            &opps,
            &ps,
            today,
            &Filter {
                unit: Some(u2),
                year: None,
            },
        );
        assert_eq!(only_u2.totals.values().sum::<usize>(), 1);

        let ct = cycle_times(&ps, &Filter::default(), None, |_| None, target_days);
        let define = ct.iter().find(|c| c.phase == "Definir").unwrap();
        assert_eq!(define.samples, vec![30.0, 20.0]);
        assert_eq!(define.median, Some(25.0));
        assert_eq!(define.target, Some(30.0));
        assert_eq!(
            ct.iter().find(|c| c.phase == "Medir").unwrap().samples,
            vec![30.0]
        );
        let level_of = |x: Id| ms.iter().find(|m| m.id == x).and_then(|m| m.level);
        let black = cycle_times(
            &ps,
            &Filter::default(),
            Some(BeltLevel::Black),
            level_of,
            target_days,
        );
        assert_eq!(black.len(), 1);

        let sv = savings_by_state(
            &ps,
            &Filter {
                unit: None,
                year: Some(2026),
            },
        );
        assert_eq!(official_total(&sv), 30_000.0);
        assert_eq!(
            sv[&(Some(u1), Category::Hard, SavingStatus::Forecast)],
            50_000.0
        );
        assert_eq!(
            official_total(&savings_by_state(&ps, &Filter::default())),
            40_000.0
        );

        let us = programa::units(&t.s, p);
        let br = belts_by_unit(&us, &ms, &ps, today);
        let r1 = br.iter().find(|r| r.unit == Some(u1)).unwrap();
        assert_eq!(r1.active.get(&BeltLevel::Green), Some(&1));
        assert_eq!(r1.certified.get(&BeltLevel::Green), Some(&1));
        assert_eq!(r1.expiring, 1, "certified 2023-12-01: expires in 28 days");
        assert_eq!(r1.coverage, Some(1.0));
        let r2 = br.iter().find(|r| r.unit == Some(u2)).unwrap();
        assert_eq!(r2.trained.get(&BeltLevel::Black), Some(&1));
        assert!(r2.active.is_empty(), "B is closed");

        let tools = crate::mentoria::all_tools();
        let tu = tool_usage(&ps, &tools, &CompetenceMap::default(), &Filter::default());
        assert_eq!(tu.projects["sipoc"], 2);
        assert_eq!(tu.by_tool["fmea"][&2], 1);
        assert!(!tu.projects.contains_key("pareto"), "training out");
        assert!(tu.never_applied.contains(&"fmea".to_string()));
        assert!(!tu.never_applied.contains(&"sipoc".to_string()));

        let od = overdue(&ps, &[], today, &Filter::default());
        assert_eq!(od.items.len(), 1);
        assert_eq!(od.by_unit[&Some(u1)], 1);
        assert_eq!(od.by_belt[&ana], 1);
        assert_eq!(od.pending_reviews, 0, "the only submitted one is training");

        let ls = designacao::loads(&ms, &ps, &ideas, &[], &[], &Limits::default(), today);
        let wl = workload(&ls);
        assert_eq!(wl[&Some(BeltLevel::Green)].hours, 6.0, "training hours out");
        assert_eq!(wl[&Some(BeltLevel::Green)].overloaded, 0);

        let id = t.id();
        let e = programa::set_target(&t.s, p, id, u2, 2026, Metric::HardValidated, None, 60_000.0)
            .unwrap();
        t.commit(e);
        let rs: Vec<Review> = Vec::new();
        let inputs = Inputs {
            state: &t.s,
            program: p,
            projects: &ps,
            ideas: &ideas,
            members: &ms,
            opportunities: &opps,
            reviews: &rs,
            today,
        };
        let lv = leadership(&inputs, 2026, &Filter::default());
        assert_eq!(lv.len(), 6);
        let k = |x| lv.iter().find(|v| v.kpi == x).unwrap().clone();
        assert_eq!(k(Kpi::ValidatedSavings).value, 30_000.0);
        assert_eq!(k(Kpi::ValidatedSavings).target, Some(60_000.0));
        assert_eq!(k(Kpi::ValidatedSavings).trend(), 1);
        assert_eq!(k(Kpi::ProjectsClosed).value, 1.0);
        assert_eq!(k(Kpi::OpenPipeline).value, 2.0);
        assert_eq!(k(Kpi::OverdueTollgates).value, 1.0);
        assert_eq!(k(Kpi::OverdueTollgates).trend(), 0);
        assert_eq!(expiry(d(2023, 5, 10)), d(2026, 5, 10));
        assert_eq!(expiry(d(2024, 2, 29)), d(2027, 2, 28));
    }
}
