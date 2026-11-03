//! The training teams (`docs/plano_lean_seis_sigma.md`, 8.5; decision MB6): the exercise
//! groups of a class, suggested under constraints and decided by the MBB.
//!
//! **Hard constraints**: balanced sizes (the groups differ by at most one person), at most
//! `max_same_area` people of the same area per group and never two people who must not be
//! together (a manager and their report, say). **Mix** (soft): as few pairs of the same
//! unit, area and function in a group as possible. Each green or black belt trainee needs
//! a candidate project and a champion: not a placement matter, but flagged.
//!
//! The method is a greedy placement with local swaps, deterministic for the same input:
//! the trainees in a fixed order (with constraints first, then the most common areas,
//! then the id) each go to the group where they add the least cost (ties: the smaller
//! group, then the first), and then any swap of two people that lowers the total cost is
//! made, in a fixed order, until none does. Every placement carries its reasons and every
//! violation left is explained. The MBB moves people by hand and confirms.

use std::collections::BTreeMap;

use crate::id::Id;
use crate::programa::BeltLevel;

pub const DEFAULT_GROUP_SIZE: usize = 5;
pub const DEFAULT_MAX_SAME_AREA: usize = 2;

/// Weight of a hard violation against the mix.
const HARD: f64 = 1000.0;
/// Passes of the local search at most.
const MAX_PASSES: usize = 50;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trainee {
    pub id: Id,
    pub unit: Option<Id>,
    pub area: String,
    pub function: String,
    pub has_project: bool,
    pub has_champion: bool,
    /// People this trainee must not share a group with.
    pub avoid: Vec<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    pub group_size: usize,
    pub max_same_area: usize,
    /// Green and black belt: every trainee needs a candidate project and a champion.
    pub need_project: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            group_size: DEFAULT_GROUP_SIZE,
            max_same_area: DEFAULT_MAX_SAME_AREA,
            need_project: false,
        }
    }
}

impl Rules {
    pub fn for_level(level: BeltLevel) -> Rules {
        Rules {
            need_project: level >= BeltLevel::Green,
            ..Rules::default()
        }
    }
}

/// A constraint not met by a distribution.
#[derive(Debug, Clone, PartialEq)]
pub enum Violation {
    /// More people of one area in a group than allowed.
    AreaCap {
        group: usize,
        area: String,
        count: usize,
        max: usize,
    },
    /// Two people who must not be together.
    Conflict { group: usize, a: Id, b: Id },
    /// A group out of the balanced sizes.
    Size {
        group: usize,
        size: usize,
        min: usize,
        max: usize,
    },
    /// A green or black belt trainee without a candidate project.
    NoProject { person: Id },
    /// A green or black belt trainee without a champion.
    NoChampion { person: Id },
}

/// Why a trainee is in their group.
#[derive(Debug, Clone, PartialEq)]
pub enum Why {
    /// Placed by the greedy step: the group's counts of the same unit, area and function
    /// before the placement, and its size.
    Placed {
        same_unit: usize,
        same_area: usize,
        same_function: usize,
        size: usize,
    },
    /// Placed early because of a "not together" constraint.
    Constrained,
    /// Moved by a swap with `with`, which lowered the cost from `before` to `after`.
    Swapped { with: Id, before: f64, after: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub person: Id,
    pub group: usize,
    pub reasons: Vec<Why>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Suggestion {
    pub groups: Vec<Vec<Id>>,
    pub placements: Vec<Placement>,
    pub violations: Vec<Violation>,
    /// Total cost (hard violations × 1000 plus the pairs of the same unit, area and
    /// function).
    pub cost: f64,
}

/// Number of groups and the balanced sizes `(min, max)` for `n` trainees.
pub fn shape(n: usize, rules: &Rules) -> (usize, usize, usize) {
    if n == 0 {
        return (0, 0, 0);
    }
    let size = rules.group_size.max(1);
    let k = n.div_ceil(size);
    (k, n / k, n.div_ceil(k))
}

fn pairs(n: usize) -> f64 {
    (n * n.saturating_sub(1) / 2) as f64
}

/// Cost of one group.
fn group_cost(members: &[usize], ts: &[Trainee], rules: &Rules, max_size: usize) -> f64 {
    let mut units: BTreeMap<Option<Id>, usize> = BTreeMap::new();
    let mut areas: BTreeMap<&str, usize> = BTreeMap::new();
    let mut funcs: BTreeMap<&str, usize> = BTreeMap::new();
    let mut c = 0.0;
    for (i, a) in members.iter().enumerate() {
        let t = &ts[*a];
        *units.entry(t.unit).or_default() += 1;
        if !t.area.is_empty() {
            *areas.entry(t.area.as_str()).or_default() += 1;
        }
        if !t.function.is_empty() {
            *funcs.entry(t.function.as_str()).or_default() += 1;
        }
        for b in &members[i + 1..] {
            let u = &ts[*b];
            if t.avoid.contains(&u.id) || u.avoid.contains(&t.id) {
                c += HARD;
            }
        }
    }
    for n in areas.values() {
        c += HARD * n.saturating_sub(rules.max_same_area) as f64;
        c += 0.5 * pairs(*n);
    }
    for n in units.values() {
        c += pairs(*n);
    }
    for n in funcs.values() {
        c += 0.25 * pairs(*n);
    }
    c += HARD * members.len().saturating_sub(max_size) as f64;
    c
}

fn total(groups: &[Vec<usize>], ts: &[Trainee], rules: &Rules, max: usize) -> f64 {
    groups.iter().map(|g| group_cost(g, ts, rules, max)).sum()
}

/// The suggestion for the trainees of a class.
pub fn suggest(trainees: &[Trainee], rules: &Rules) -> Suggestion {
    let mut ts: Vec<Trainee> = trainees.to_vec();
    ts.sort_by_key(|t| t.id);
    ts.dedup_by_key(|t| t.id);
    let n = ts.len();
    let (k, min, max) = shape(n, rules);
    if n == 0 {
        return Suggestion::default();
    }
    // Balanced sizes: the first `n % k` groups take one more.
    let caps: Vec<usize> = (0..k)
        .map(|g| if g < n - min * k { min + 1 } else { min })
        .collect();
    let mut area_count: BTreeMap<String, usize> = BTreeMap::new();
    for t in &ts {
        *area_count.entry(t.area.clone()).or_default() += 1;
    }
    let constrained =
        |t: &Trainee| !t.avoid.is_empty() || ts.iter().any(|u| u.avoid.contains(&t.id));
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| {
        let (ta, tb) = (&ts[*a], &ts[*b]);
        constrained(tb)
            .cmp(&constrained(ta))
            .then(area_count[&tb.area].cmp(&area_count[&ta.area]))
            .then(ta.id.cmp(&tb.id))
    });
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); k];
    let mut reasons: BTreeMap<usize, Vec<Why>> = BTreeMap::new();
    for i in order {
        let t = &ts[i];
        let mut best: Option<(f64, usize, usize)> = None;
        for (g, members) in groups.iter().enumerate() {
            if members.len() >= caps[g] {
                continue;
            }
            let before = group_cost(members, &ts, rules, max);
            let mut with = members.clone();
            with.push(i);
            let delta = group_cost(&with, &ts, rules, max) - before;
            let key = (delta, members.len(), g);
            if best.is_none_or(|b| {
                key.0 < b.0 - 1e-12 || ((key.0 - b.0).abs() <= 1e-12 && (key.1, key.2) < (b.1, b.2))
            }) {
                best = Some(key);
            }
        }
        let g = best.map_or(0, |b| b.2);
        let m = &groups[g];
        let why = Why::Placed {
            same_unit: m.iter().filter(|x| ts[**x].unit == t.unit).count(),
            same_area: m
                .iter()
                .filter(|x| !t.area.is_empty() && ts[**x].area == t.area)
                .count(),
            same_function: m
                .iter()
                .filter(|x| !t.function.is_empty() && ts[**x].function == t.function)
                .count(),
            size: m.len(),
        };
        let mut r = Vec::new();
        if constrained(t) {
            r.push(Why::Constrained);
        }
        r.push(why);
        reasons.insert(i, r);
        groups[g].push(i);
    }
    // Local search: the first improving swap, in a fixed order, until none.
    for _ in 0..MAX_PASSES {
        let mut improved = false;
        'scan: for ga in 0..k {
            for gb in ga + 1..k {
                for ia in 0..groups[ga].len() {
                    for ib in 0..groups[gb].len() {
                        let (a, b) = (groups[ga][ia], groups[gb][ib]);
                        let before = group_cost(&groups[ga], &ts, rules, max)
                            + group_cost(&groups[gb], &ts, rules, max);
                        let mut na = groups[ga].clone();
                        let mut nb = groups[gb].clone();
                        na[ia] = b;
                        nb[ib] = a;
                        let after =
                            group_cost(&na, &ts, rules, max) + group_cost(&nb, &ts, rules, max);
                        if after < before - 1e-9 {
                            groups[ga] = na;
                            groups[gb] = nb;
                            reasons.entry(a).or_default().push(Why::Swapped {
                                with: ts[b].id,
                                before,
                                after,
                            });
                            reasons.entry(b).or_default().push(Why::Swapped {
                                with: ts[a].id,
                                before,
                                after,
                            });
                            improved = true;
                            break 'scan;
                        }
                    }
                }
            }
        }
        if !improved {
            break;
        }
    }
    let cost = total(&groups, &ts, rules, max);
    let id_groups: Vec<Vec<Id>> = groups
        .iter()
        .map(|g| {
            let mut v: Vec<Id> = g.iter().map(|i| ts[*i].id).collect();
            v.sort();
            v
        })
        .collect();
    let mut placements: Vec<Placement> = Vec::new();
    for (g, members) in groups.iter().enumerate() {
        for i in members {
            placements.push(Placement {
                person: ts[*i].id,
                group: g,
                reasons: reasons.remove(i).unwrap_or_default(),
            });
        }
    }
    placements.sort_by_key(|p| p.person);
    let violations = check(&id_groups, &ts, rules);
    Suggestion {
        groups: id_groups,
        placements,
        violations,
        cost,
    }
}

/// The violations of a distribution (also of one adjusted by hand).
pub fn check(groups: &[Vec<Id>], trainees: &[Trainee], rules: &Rules) -> Vec<Violation> {
    let n: usize = groups.iter().map(Vec::len).sum();
    let k = groups.len().max(1);
    let (min, max) = (n / k, n.div_ceil(k));
    let find = |id: Id| trainees.iter().find(|t| t.id == id);
    let mut v = Vec::new();
    for (g, members) in groups.iter().enumerate() {
        if members.len() < min || members.len() > max {
            v.push(Violation::Size {
                group: g,
                size: members.len(),
                min,
                max,
            });
        }
        let mut areas: BTreeMap<&str, usize> = BTreeMap::new();
        for m in members {
            if let Some(t) = find(*m).filter(|t| !t.area.is_empty()) {
                *areas.entry(t.area.as_str()).or_default() += 1;
            }
        }
        for (a, c) in areas {
            if c > rules.max_same_area {
                v.push(Violation::AreaCap {
                    group: g,
                    area: a.to_string(),
                    count: c,
                    max: rules.max_same_area,
                });
            }
        }
        for (i, a) in members.iter().enumerate() {
            for b in &members[i + 1..] {
                let ta = find(*a);
                let tb = find(*b);
                if ta.is_some_and(|t| t.avoid.contains(b))
                    || tb.is_some_and(|t| t.avoid.contains(a))
                {
                    v.push(Violation::Conflict {
                        group: g,
                        a: *a,
                        b: *b,
                    });
                }
            }
        }
    }
    if rules.need_project {
        let mut ids: Vec<Id> = groups.iter().flatten().copied().collect();
        ids.sort();
        for id in ids {
            if let Some(t) = find(id) {
                if !t.has_project {
                    v.push(Violation::NoProject { person: id });
                }
                if !t.has_champion {
                    v.push(Violation::NoChampion { person: id });
                }
            }
        }
    }
    v
}

/// Whether a violation is about the placement (and not about the trainee's data).
pub fn is_placement(v: &Violation) -> bool {
    matches!(
        v,
        Violation::AreaCap { .. } | Violation::Conflict { .. } | Violation::Size { .. }
    )
}
