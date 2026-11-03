//! Capacity and assignment (`docs/plano_lean_seis_sigma.md`, 8.2).
//!
//! - **Capacity** of a person: hours per week for the program, absences, a limit of
//!   simultaneous projects by level (defaults: yellow 1, green 2, black 4 with up to 4
//!   green belts mentored; champions up to 6 projects followed), all editable in the
//!   program. **Load**: the hours of the active assignments plus the hours of the open
//!   schedule tasks of the person, summed over **every** portfolio of the program, so a
//!   person working for two units is seen once.
//! - **Assignment**: for a selected opportunity the app ranks the belts by explained
//!   rules, in this order: eligible (level required by the approach, a free project slot,
//!   enough free hours, not away most of the period), then needs a project for the
//!   certification, same unit, same area, experience in the planned tools (competence
//!   map) and free hours. Every position carries its reasons. The MBB decides; the
//!   decision is recorded with the position the person had in the suggestion.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::date::Date;
use crate::domain::{Approach, ProjectStatus};
use crate::id::Id;
use crate::mentoria::{CompetenceMap, Level};
use crate::op::{Edit, EntityRef, Kind};
use crate::programa::{BeltLevel, IdeaRecord, Member, ProjectRecord};
use crate::state::{Entity, State};

pub mod fields {
    pub const PROGRAM: &str = "programa";
    // Limits on the program entity.
    pub const LIMIT_YELLOW: &str = "limite_yellow";
    pub const LIMIT_GREEN: &str = "limite_green";
    pub const LIMIT_BLACK: &str = "limite_black";
    pub const LIMIT_MENTEES: &str = "limite_mentorados";
    pub const LIMIT_CHAMPION: &str = "limite_champion";
    // Absence.
    pub const PERSON: &str = "pessoa";
    pub const START: &str = "inicio";
    pub const END: &str = "fim";
    pub const REASON: &str = "motivo";
    // Assignment.
    pub const OPPORTUNITY: &str = "oportunidade";
    pub const IDEA: &str = "ideia";
    pub const HOURS_WEEK: &str = "horas_semana";
    pub const MENTOR: &str = "mentor";
    pub const POSITION: &str = "posicao_sugerida";
    pub const DATE: &str = "data";
    pub const BY: &str = "decidido_por";
    pub const ENDED: &str = "encerrada";
}

/// Limits of simultaneous work by level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub yellow: u32,
    pub green: u32,
    pub black: u32,
    /// Green belts a black belt mentors at once.
    pub mentees: u32,
    /// Projects a champion follows at once.
    pub champion: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            yellow: 1,
            green: 2,
            black: 4,
            mentees: 4,
            champion: 6,
        }
    }
}

impl Limits {
    pub fn read(state: &State, program: Id) -> Limits {
        let d = Limits::default();
        let Some(e) = state.entity(program) else {
            return d;
        };
        let n = |f: &str, dv: u32| {
            e.integer(f)
                .filter(|n| *n > 0)
                .map_or(dv, |n| n.min(50) as u32)
        };
        Limits {
            yellow: n(fields::LIMIT_YELLOW, d.yellow),
            green: n(fields::LIMIT_GREEN, d.green),
            black: n(fields::LIMIT_BLACK, d.black),
            mentees: n(fields::LIMIT_MENTEES, d.mentees),
            champion: n(fields::LIMIT_CHAMPION, d.champion),
        }
    }

    pub fn of_level(&self, l: BeltLevel) -> u32 {
        match l {
            BeltLevel::Yellow => self.yellow,
            BeltLevel::Green => self.green,
            BeltLevel::Black | BeltLevel::MasterBlack => self.black,
        }
    }
}

/// Hours per week a level gives a project by default.
pub fn default_hours(level: BeltLevel) -> f64 {
    match level {
        BeltLevel::Yellow => 4.0,
        BeltLevel::Green => 8.0,
        BeltLevel::Black | BeltLevel::MasterBlack => 16.0,
    }
}

/// The level an approach asks for (the complexity of the planned tools): a redesign
/// (DMADV) asks for a black belt, a DMAIC for a green belt, the short approaches for a
/// yellow belt.
pub fn required_level(approach: Approach) -> BeltLevel {
    match approach {
        Approach::Dmadv => BeltLevel::Black,
        Approach::Dmaic => BeltLevel::Green,
        Approach::DirectAction | Approach::Kaizen | Approach::A3 | Approach::Pdca => {
            BeltLevel::Yellow
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Absence {
    pub id: Id,
    pub person: Option<Id>,
    pub start: Option<Date>,
    pub end: Option<Date>,
    pub reason: String,
}

impl Absence {
    pub fn read(id: Id, e: &Entity) -> Self {
        let d = |f| e.opt_text(f).and_then(|s| s.parse::<Date>().ok());
        Self {
            id,
            person: e.id(fields::PERSON),
            start: d(fields::START),
            end: d(fields::END),
            reason: e.text(fields::REASON),
        }
    }
}

pub fn absences(state: &State) -> Vec<Absence> {
    let mut v: Vec<Absence> = state
        .of_kind(Kind::Absence)
        .map(|(id, e)| Absence::read(id, e))
        .collect();
    v.sort_by_key(|a| (a.start, a.id));
    v
}

/// Edits of an absence: the person first (a person may record their own).
pub fn add_absence(id: Id, person: Id, start: Date, end: Date, reason: &str) -> Vec<Edit> {
    let r = EntityRef::new(Kind::Absence, id);
    let (start, end) = if end < start {
        (end, start)
    } else {
        (start, end)
    };
    vec![
        Edit::new(r, fields::PERSON, person.to_string()),
        Edit::new(r, fields::START, start.to_string()),
        Edit::new(r, fields::END, end.to_string()),
        Edit::new(r, fields::REASON, reason.trim()),
    ]
}

/// Days of `[from, to]` (inclusive) a person is away.
pub fn absent_days(absences: &[Absence], person: Id, from: Date, to: Date) -> i64 {
    let mut days: BTreeSet<i64> = BTreeSet::new();
    for a in absences.iter().filter(|a| a.person == Some(person)) {
        let (Some(s), Some(e)) = (a.start, a.end) else {
            continue;
        };
        let lo = s.max(from).days_since_epoch();
        let hi = e.min(to).days_since_epoch();
        days.extend(lo..=hi);
    }
    days.len() as i64
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub id: Id,
    pub opportunity: Option<Id>,
    pub idea: Option<Id>,
    pub person: Option<Id>,
    pub hours_week: f64,
    pub mentor: Option<Id>,
    pub position: Option<i64>,
    pub reason: String,
    pub date: Option<Date>,
    pub by: Option<Id>,
    pub ended: bool,
}

impl Assignment {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            opportunity: e.id(fields::OPPORTUNITY),
            idea: e.id(fields::IDEA),
            person: e.id(fields::PERSON),
            hours_week: e.number(fields::HOURS_WEEK).unwrap_or(0.0).max(0.0),
            mentor: e.id(fields::MENTOR),
            position: e.integer(fields::POSITION),
            reason: e.text(fields::REASON),
            date: e.opt_text(fields::DATE).and_then(|s| s.parse().ok()),
            by: e.id(fields::BY),
            ended: e.boolean(fields::ENDED),
        }
    }
}

pub fn assignments(state: &State) -> Vec<Assignment> {
    let mut v: Vec<Assignment> = state
        .of_kind(Kind::Assignment)
        .map(|(id, e)| Assignment::read(id, e))
        .collect();
    v.sort_by_key(|a| a.id);
    v
}

/// Whether an assignment still takes hours: not ended, and its idea neither closed nor
/// out of the pipeline; once the idea's project is open, the project must be running.
pub fn is_active(a: &Assignment, ideas: &[IdeaRecord], projects: &[ProjectRecord]) -> bool {
    if a.ended {
        return false;
    }
    let Some(i) = a.idea.and_then(|i| ideas.iter().find(|x| x.id == i)) else {
        // The idea is not visible (its portfolio is not reachable): keep counting.
        return true;
    };
    if let Some(p) = i.project.and_then(|p| projects.iter().find(|r| r.id == p)) {
        return p.is_running();
    }
    matches!(
        i.status,
        ProjectStatus::Idea
            | ProjectStatus::Screening
            | ProjectStatus::Approved
            | ProjectStatus::Running
            | ProjectStatus::Suspended
    )
}

/// What the MBB decides.
#[derive(Debug, Clone, PartialEq)]
pub struct AssignInput {
    pub opportunity: Option<Id>,
    pub idea: Option<Id>,
    pub person: Id,
    pub hours_week: f64,
    pub mentor: Option<Id>,
    pub position: Option<usize>,
    pub reason: String,
    pub by: Id,
    pub date: Date,
}

pub fn assign(program: Id, id: Id, input: &AssignInput) -> Vec<Edit> {
    let r = EntityRef::new(Kind::Assignment, id);
    let mut e = vec![
        Edit::new(r, fields::PROGRAM, program.to_string()),
        Edit::new(r, fields::PERSON, input.person.to_string()),
        Edit::new(r, fields::HOURS_WEEK, input.hours_week.max(0.0)),
        Edit::new(r, fields::REASON, input.reason.trim()),
        Edit::new(r, fields::BY, input.by.to_string()),
        Edit::new(r, fields::DATE, input.date.to_string()),
    ];
    if let Some(o) = input.opportunity {
        e.push(Edit::new(r, fields::OPPORTUNITY, o.to_string()));
    }
    if let Some(i) = input.idea {
        e.push(Edit::new(r, fields::IDEA, i.to_string()));
    }
    if let Some(m) = input.mentor {
        e.push(Edit::new(r, fields::MENTOR, m.to_string()));
    }
    if let Some(p) = input.position {
        e.push(Edit::new(r, fields::POSITION, Value::from(p as i64 + 1)));
    }
    e
}

/// The load of one person over the whole program.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Load {
    pub person: Id,
    pub level: Option<BeltLevel>,
    /// Running projects as belt, in any unit.
    pub projects: Vec<Id>,
    pub units: BTreeSet<Id>,
    /// Hours per week committed: assignments plus open tasks.
    pub hours: f64,
    /// Hours per week available (0 when the capacity is not recorded).
    pub capacity: f64,
    pub project_limit: u32,
    pub mentees: usize,
    pub champion_projects: usize,
    /// Days away in the next four weeks.
    pub absent_days: i64,
}

impl Load {
    pub fn over_projects(&self) -> bool {
        self.level.is_some() && self.projects.len() as u32 > self.project_limit
    }

    pub fn over_hours(&self) -> bool {
        self.capacity > 0.0 && self.hours > self.capacity + 1e-9
    }

    pub fn free_hours(&self) -> f64 {
        (self.capacity - self.hours).max(0.0)
    }

    pub fn free_slot(&self) -> bool {
        (self.projects.len() as u32) < self.project_limit
    }
}

/// What one overloaded person exceeds.
#[derive(Debug, Clone, PartialEq)]
pub enum Overload {
    Projects {
        person: Id,
        has: usize,
        limit: u32,
    },
    Hours {
        person: Id,
        hours: f64,
        capacity: f64,
    },
    Mentees {
        person: Id,
        has: usize,
        limit: u32,
    },
    Champion {
        person: Id,
        has: usize,
        limit: u32,
    },
}

/// The load of every person of the program (belts and champions).
pub fn loads(
    members: &[Member],
    projects: &[ProjectRecord],
    ideas: &[IdeaRecord],
    assignments: &[Assignment],
    absences: &[Absence],
    limits: &Limits,
    today: Date,
) -> Vec<Load> {
    let real: Vec<&ProjectRecord> = projects
        .iter()
        .filter(|p| !p.training && p.is_running())
        .collect();
    let horizon = today.add_days(27);
    members
        .iter()
        .filter(|m| !m.anonymised)
        .map(|m| {
            let mine: Vec<&&ProjectRecord> =
                real.iter().filter(|p| p.belts.contains(&m.id)).collect();
            let mut hours: f64 = real
                .iter()
                .flat_map(|p| p.task_hours.iter())
                .filter(|(w, _)| *w == m.id)
                .map(|(_, h)| h)
                .sum();
            let active: Vec<&Assignment> = assignments
                .iter()
                .filter(|a| is_active(a, ideas, projects))
                .collect();
            hours += active
                .iter()
                .filter(|a| a.person == Some(m.id))
                .map(|a| a.hours_week)
                .sum::<f64>();
            let away = absent_days(absences, m.id, today, horizon);
            let capacity = m.hours_week.unwrap_or(0.0) * (1.0 - away as f64 / 28.0).max(0.0);
            Load {
                person: m.id,
                level: m.level,
                projects: mine.iter().map(|p| p.id).collect(),
                units: mine.iter().filter_map(|p| p.unit).collect(),
                hours,
                capacity,
                project_limit: m
                    .project_limit
                    .unwrap_or_else(|| m.level.map_or(0, |l| limits.of_level(l))),
                mentees: active
                    .iter()
                    .filter(|a| a.mentor == Some(m.id))
                    .filter_map(|a| a.person)
                    .collect::<BTreeSet<_>>()
                    .len(),
                champion_projects: real.iter().filter(|p| p.champions.contains(&m.id)).count(),
                absent_days: away,
            }
        })
        .collect()
}

/// The overload check of the whole program.
pub fn overloads(loads: &[Load], limits: &Limits) -> Vec<Overload> {
    let mut v = Vec::new();
    for l in loads {
        if l.over_projects() {
            v.push(Overload::Projects {
                person: l.person,
                has: l.projects.len(),
                limit: l.project_limit,
            });
        }
        if l.over_hours() {
            v.push(Overload::Hours {
                person: l.person,
                hours: l.hours,
                capacity: l.capacity,
            });
        }
        if l.mentees as u32 > limits.mentees {
            v.push(Overload::Mentees {
                person: l.person,
                has: l.mentees,
                limit: limits.mentees,
            });
        }
        if l.champion_projects as u32 > limits.champion {
            v.push(Overload::Champion {
                person: l.person,
                has: l.champion_projects,
                limit: limits.champion,
            });
        }
    }
    v
}

/// Belts with a free project slot, by level (for the committee's agenda).
pub fn free_by_level(loads: &[Load]) -> BTreeMap<BeltLevel, usize> {
    let mut m = BTreeMap::new();
    for l in loads {
        if let Some(level) = l.level {
            let e = m.entry(level).or_insert(0);
            if l.free_slot() && !l.over_hours() {
                *e += 1;
            }
        }
    }
    m
}

/// What a project needs of its belt.
#[derive(Debug, Clone, PartialEq)]
pub struct Need {
    pub level: BeltLevel,
    pub unit: Option<Id>,
    pub area: String,
    pub tools: Vec<String>,
    pub hours_week: f64,
}

impl Need {
    /// The need of an idea: level by its approach, default hours of that level.
    pub fn of_idea(idea: &IdeaRecord) -> Need {
        let level = required_level(idea.approach.unwrap_or_default());
        Need {
            level,
            unit: idea.unit,
            area: idea.process.clone(),
            tools: Vec::new(),
            hours_week: default_hours(level),
        }
    }
}

/// One reason of a candidate's position.
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    Level {
        has: BeltLevel,
        needs: BeltLevel,
        ok: bool,
    },
    Slots {
        used: usize,
        limit: u32,
        ok: bool,
    },
    Hours {
        free: f64,
        needs: f64,
        ok: bool,
    },
    Away {
        days: i64,
    },
    NeedsCertification,
    SameUnit(bool),
    SameArea,
    Experience {
        tools: usize,
        of: usize,
    },
}

impl Reason {
    /// Whether the reason speaks for the candidate.
    pub fn favourable(&self) -> bool {
        match self {
            Reason::Level { ok, .. } | Reason::Slots { ok, .. } | Reason::Hours { ok, .. } => *ok,
            Reason::Away { .. } => false,
            Reason::SameUnit(same) => *same,
            Reason::NeedsCertification | Reason::SameArea => true,
            Reason::Experience { tools, .. } => *tools > 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub person: Id,
    pub name: String,
    pub eligible: bool,
    pub reasons: Vec<Reason>,
    /// The sort key, explained by `reasons`.
    pub key: (bool, bool, bool, bool, usize, i64),
}

/// The belts in order for a need, each with the reasons of the position. Only people with
/// a belt level are candidates. Deterministic: ties go by name and id.
pub fn rank(
    need: &Need,
    members: &[Member],
    loads: &[Load],
    map: &CompetenceMap,
) -> Vec<Candidate> {
    let mut v: Vec<Candidate> = members
        .iter()
        .filter(|m| m.level.is_some() && !m.anonymised)
        .map(|m| {
            let level = m.level.unwrap_or_default();
            let load = loads.iter().find(|l| l.person == m.id);
            let used = load.map_or(0, |l| l.projects.len());
            let limit = load.map_or(0, |l| l.project_limit);
            let free = load.map_or(0.0, Load::free_hours);
            let away = load.map_or(0, |l| l.absent_days);
            let level_ok = level >= need.level;
            let slot_ok = (used as u32) < limit;
            let hours_ok = free + 1e-9 >= need.hours_week;
            let away_ok = away < 14;
            let eligible = level_ok && slot_ok && hours_ok && away_ok;
            let same_unit = need.unit.is_some() && m.unit == need.unit;
            let same_area = !need.area.trim().is_empty()
                && !m.area.trim().is_empty()
                && m.area.trim().eq_ignore_ascii_case(need.area.trim());
            let exp = need
                .tools
                .iter()
                .filter(|t| map.level(m.id, t) >= Some(Level::Applied))
                .count();
            let mut reasons = vec![
                Reason::Level {
                    has: level,
                    needs: need.level,
                    ok: level_ok,
                },
                Reason::Slots {
                    used,
                    limit,
                    ok: slot_ok,
                },
                Reason::Hours {
                    free,
                    needs: need.hours_week,
                    ok: hours_ok,
                },
            ];
            if away > 0 {
                reasons.push(Reason::Away { days: away });
            }
            if m.needs_certification {
                reasons.push(Reason::NeedsCertification);
            }
            if need.unit.is_some() {
                reasons.push(Reason::SameUnit(same_unit));
            }
            if same_area {
                reasons.push(Reason::SameArea);
            }
            if !need.tools.is_empty() {
                reasons.push(Reason::Experience {
                    tools: exp,
                    of: need.tools.len(),
                });
            }
            Candidate {
                person: m.id,
                name: m.name.clone(),
                eligible,
                reasons,
                key: (
                    eligible,
                    m.needs_certification,
                    same_unit,
                    same_area,
                    exp,
                    (free * 100.0).round() as i64,
                ),
            }
        })
        .collect();
    v.sort_by(|a, b| {
        b.key
            .cmp(&a.key)
            .then(a.name.cmp(&b.name))
            .then(a.person.cmp(&b.person))
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mentoria::{self, Source};
    use crate::programa::testkit::*;
    use proptest::prelude::*;

    fn member(n: u8, name: &str, level: Option<BeltLevel>, hours: f64) -> Member {
        Member {
            id: Id::v7(u64::from(n), [n; 10]),
            name: name.into(),
            level,
            hours_week: Some(hours),
            ..Member::default()
        }
    }

    #[test]
    fn overload_is_summed_over_every_unit() {
        let (u1, u2) = (Id::v7(100, [1; 10]), Id::v7(101, [1; 10]));
        let ana = member(1, "Ana", Some(BeltLevel::Green), 10.0);
        let champ = member(2, "Caio", None, 0.0);
        let mut ps = Vec::new();
        for (i, u) in [u1, u2, u2].iter().enumerate() {
            let mut p = project(
                Id::v7(200 + i as u64, [2; 10]),
                Some(*u),
                ProjectStatus::Running,
            );
            p.belts = vec![ana.id];
            p.champions = vec![champ.id];
            p.task_hours = vec![(ana.id, 3.0)];
            ps.push(p);
        }
        let mut closed = project(Id::v7(300, [3; 10]), Some(u1), ProjectStatus::Closed);
        closed.belts = vec![ana.id];
        ps.push(closed);
        let limits = Limits {
            champion: 2,
            ..Limits::default()
        };
        let today = d(2026, 11, 2);
        let abs = vec![Absence {
            id: Id::v7(400, [4; 10]),
            person: Some(ana.id),
            start: Some(today.add_days(7)),
            end: Some(today.add_days(13)),
            reason: "férias".into(),
        }];
        let ls = loads(
            &[ana.clone(), champ.clone()],
            &ps,
            &[],
            &[],
            &abs,
            &limits,
            today,
        );
        let l = &ls[0];
        assert_eq!(l.projects.len(), 3, "the closed project does not count");
        assert_eq!(l.units.len(), 2);
        assert!((l.hours - 9.0).abs() < 1e-12);
        assert_eq!(l.absent_days, 7);
        assert!(
            (l.capacity - 7.5).abs() < 1e-12,
            "a quarter of the month away"
        );
        let ov = overloads(&ls, &limits);
        assert!(ov.contains(&Overload::Projects {
            person: ana.id,
            has: 3,
            limit: 2
        }));
        assert!(
            ov.iter()
                .any(|o| matches!(o, Overload::Hours { person, .. } if *person == ana.id))
        );
        assert!(ov.contains(&Overload::Champion {
            person: champ.id,
            has: 3,
            limit: 2
        }));
        assert_eq!(free_by_level(&ls)[&BeltLevel::Green], 0);
    }

    #[test]
    fn ranking_is_explained_and_deterministic() {
        let u = Id::v7(100, [1; 10]);
        let mut ana = member(1, "Ana", Some(BeltLevel::Green), 10.0);
        ana.unit = Some(u);
        let mut bia = member(2, "Bia", Some(BeltLevel::Green), 10.0);
        bia.needs_certification = true;
        let caio = member(3, "Caio", Some(BeltLevel::Yellow), 40.0);
        let mut duda = member(4, "Duda", Some(BeltLevel::Black), 20.0);
        duda.unit = Some(u);
        let eva = member(5, "Eva", None, 40.0);
        let mut full = member(6, "Fabio", Some(BeltLevel::Green), 2.0);
        full.unit = Some(u);
        let ms = vec![
            full.clone(),
            eva,
            duda.clone(),
            caio.clone(),
            bia.clone(),
            ana.clone(),
        ];
        let today = d(2026, 11, 2);
        let ls = loads(&ms, &[], &[], &[], &[], &Limits::default(), today);
        let mut map = CompetenceMap::default();
        let es = vec![mentoria::Entry {
            id: Id::v7(9, [9; 10]),
            person: Some(duda.id),
            tool: "fmea".into(),
            level: Level::Demonstrated,
            source: Source::Mbb,
            evidence: String::new(),
            date: None,
            by: Some(Id::v7(50, [5; 10])),
        }];
        map.cells = mentoria::competence_map(&es, &[], |_| true).cells;
        let need = Need {
            level: BeltLevel::Green,
            unit: Some(u),
            area: String::new(),
            tools: vec!["fmea".into()],
            hours_week: 8.0,
        };
        let r = rank(&need, &ms, &ls, &map);
        let names: Vec<&str> = r.iter().map(|c| c.name.as_str()).collect();
        // Bia needs a certification project; Duda and Ana are in the unit, Duda has the
        // experience; Fabio and Caio are not eligible (hours, level); Eva is no belt.
        assert_eq!(names, ["Bia", "Duda", "Ana", "Fabio", "Caio"]);
        assert!(r[0].reasons.contains(&Reason::NeedsCertification));
        assert!(
            r[1].reasons
                .contains(&Reason::Experience { tools: 1, of: 1 })
        );
        let fabio = &r[3];
        assert!(!fabio.eligible);
        assert!(
            fabio
                .reasons
                .iter()
                .any(|x| matches!(x, Reason::Hours { ok: false, .. }) && !x.favourable())
        );
        assert!(
            r[4].reasons
                .iter()
                .any(|x| matches!(x, Reason::Level { ok: false, .. }))
        );
        assert_eq!(required_level(Approach::Dmadv), BeltLevel::Black);
        assert_eq!(required_level(Approach::Kaizen), BeltLevel::Yellow);
    }

    #[test]
    fn assignments_and_absences_through_the_journal() {
        let mut t = T::new();
        let p = t.program;
        let ana = t.person("Ana", Some(BeltLevel::Green), None);
        let a = t.id();
        let idea_id = t.id();
        t.commit(assign(
            p,
            a,
            &AssignInput {
                opportunity: None,
                idea: Some(idea_id),
                person: ana,
                hours_week: 8.0,
                mentor: Some(t.me),
                position: Some(0),
                reason: "precisa de projeto para a certificação".into(),
                by: t.me,
                date: d(2026, 11, 3),
            },
        ));
        let x = t.id();
        t.commit(add_absence(
            x,
            ana,
            d(2026, 12, 20),
            d(2026, 12, 10),
            "férias",
        ));
        let ab = absences(&t.s);
        assert_eq!(ab[0].start, Some(d(2026, 12, 10)), "dates put in order");
        assert_eq!(absent_days(&ab, ana, d(2026, 12, 1), d(2026, 12, 31)), 11);
        let asg = assignments(&t.s);
        assert_eq!(asg[0].position, Some(1));
        let mut i = idea(idea_id, None, ProjectStatus::Approved, 50.0);
        assert!(is_active(&asg[0], std::slice::from_ref(&i), &[]));
        i.status = ProjectStatus::Cancelled;
        assert!(!is_active(&asg[0], std::slice::from_ref(&i), &[]));
        let ms = crate::programa::members(&t.s);
        let ls = loads(
            &ms,
            &[],
            &[i.clone()],
            &asg,
            &ab,
            &Limits::default(),
            d(2026, 11, 3),
        );
        let mine = ls.iter().find(|l| l.person == ana).unwrap();
        assert_eq!(mine.hours, 0.0, "cancelled idea: no hours");
        i.status = ProjectStatus::Approved;
        let ls = loads(
            &ms,
            &[],
            &[i],
            &asg,
            &ab,
            &Limits::default(),
            d(2026, 11, 3),
        );
        assert_eq!(ls.iter().find(|l| l.person == ana).unwrap().hours, 8.0);
        assert_eq!(
            ls.iter().find(|l| l.person == t.me).map(|l| l.mentees),
            None,
            "the MBB is not a member here"
        );
        t.set(Kind::Program, p, fields::LIMIT_GREEN, 3);
        assert_eq!(Limits::read(&t.s, p).green, 3);
    }

    proptest! {
        /// Eligible candidates always come before the ineligible ones, and the order does
        /// not depend on the order of the members.
        #[test]
        fn eligible_first_and_order_independent(hours in proptest::collection::vec(0.0f64..20.0, 1..8), rev in any::<bool>()) {
            let mut ms: Vec<Member> = hours
                .iter()
                .enumerate()
                .map(|(i, h)| member(i as u8 + 1, &format!("P{i}"), Some(BeltLevel::Green), *h))
                .collect();
            let ls = loads(&ms, &[], &[], &[], &[], &Limits::default(), d(2026, 1, 5));
            let need = Need { level: BeltLevel::Green, unit: None, area: String::new(), tools: vec![], hours_week: 8.0 };
            let a = rank(&need, &ms, &ls, &CompetenceMap::default());
            if rev { ms.reverse(); }
            let b = rank(&need, &ms, &ls, &CompetenceMap::default());
            prop_assert_eq!(a.iter().map(|c| c.person).collect::<Vec<_>>(), b.iter().map(|c| c.person).collect::<Vec<_>>());
            let first_bad = a.iter().position(|c| !c.eligible).unwrap_or(a.len());
            prop_assert!(a[first_bad..].iter().all(|c| !c.eligible));
        }
    }
}
