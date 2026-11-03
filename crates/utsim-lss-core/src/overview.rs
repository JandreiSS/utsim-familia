//! The project overview: the numbers of the first screen, computed here so the interface
//! only draws them.

use std::collections::BTreeMap;

use crate::date::Date;
use crate::domain::{
    Action, Column, Phase, Project, Risk, RiskStatus, Saving, SavingStatus, of_project, phases,
};
use crate::id::Id;
use crate::op::Kind;
use crate::state::State;

#[derive(Debug, Clone, PartialEq)]
pub struct Overview {
    pub project: Project,
    pub phases: Vec<Phase>,
    /// Index in `phases` of the current phase.
    pub current_phase: Option<usize>,
    pub actions_by_column: BTreeMap<Column, usize>,
    pub actions_total: usize,
    pub actions_overdue: Vec<Action>,
    /// Fraction of actions done (0 when there are none).
    pub actions_done_fraction: f64,
    pub risks_open: usize,
    pub risks_high: Vec<Risk>,
    pub savings_by_status: BTreeMap<SavingStatus, f64>,
    pub meetings: usize,
    pub conflicts: usize,
    /// Days to the deadline (negative: late).
    pub days_to_deadline: Option<i64>,
}

impl Overview {
    pub fn compute(state: &State, project: Id, today: Date) -> Option<Overview> {
        let entity = state.entity(project).filter(|e| e.kind == Kind::Project)?;
        let project_v = Project::read(project, entity);
        let phases = phases(state, project);
        let current_phase = project_v
            .current_phase
            .and_then(|id| phases.iter().position(|p| p.id == id));
        let actions = of_project(state, Kind::Action, project, Action::read);
        let mut by_col: BTreeMap<Column, usize> = Column::ALL.iter().map(|c| (*c, 0)).collect();
        for a in &actions {
            *by_col.entry(a.column).or_default() += 1;
        }
        let mut overdue: Vec<Action> = actions
            .iter()
            .filter(|a| a.is_overdue(today))
            .cloned()
            .collect();
        overdue.sort_by_key(|a| (a.due, a.id));
        let done = by_col.get(&Column::Done).copied().unwrap_or(0);
        let risks: Vec<Risk> = of_project(state, Kind::Risk, project, Risk::read)
            .into_iter()
            .filter(|r| r.status == RiskStatus::Open)
            .collect();
        let mut high: Vec<Risk> = risks.iter().filter(|r| r.is_high()).cloned().collect();
        high.sort_by(|a, b| b.level().cmp(&a.level()).then(a.id.cmp(&b.id)));
        let mut savings: BTreeMap<SavingStatus, f64> = BTreeMap::new();
        for s in of_project(state, Kind::Saving, project, Saving::read) {
            *savings.entry(s.status).or_default() += s.value;
        }
        let meetings = of_project(state, Kind::Meeting, project, |id, _| id).len();
        Some(Overview {
            days_to_deadline: project_v.deadline.map(|d| today.days_until(d)),
            project: project_v,
            current_phase,
            phases,
            actions_total: actions.len(),
            actions_done_fraction: if actions.is_empty() {
                0.0
            } else {
                done as f64 / actions.len() as f64
            },
            actions_by_column: by_col,
            actions_overdue: overdue,
            risks_open: risks.len(),
            risks_high: high,
            savings_by_status: savings,
            meetings,
            conflicts: state.conflicts.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Approach, enum_value, fields};
    use crate::kanban;
    use crate::op::{Edit, EntityRef};

    #[test]
    fn overview_counts() {
        let mut n = 0u64;
        let mut ids = move || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        };
        let me = ids();
        let p = ids();
        let mut edits = Project::create(p, "Linha 2", Approach::Dmaic, &mut ids);
        edits.push(Edit::new(
            Project::entity_ref(p),
            fields::project::DEADLINE,
            "2026-12-01",
        ));
        let s = State::new();
        let mut s = State::replay(&s.prepare(me, 0, edits, &mut ids));
        let today: Date = "2026-11-03".parse().unwrap();
        let mut more = Vec::new();
        for (t, due, col) in [
            ("a", "2026-11-01", Column::Todo),
            ("b", "2026-11-10", Column::Doing),
            ("c", "2026-10-01", Column::Done),
        ] {
            let id = ids();
            let mut e = kanban::new_card(&s, p, id, t, col);
            e.push(Edit::new(Action::entity_ref(id), fields::action::DUE, due));
            for op in s.prepare(me, 0, e, &mut ids) {
                s.apply(&op).unwrap();
            }
        }
        let r = EntityRef::new(Kind::Risk, ids());
        more.push(Edit::new(r, fields::risk::PROJECT, p.to_string()));
        more.push(Edit::new(r, fields::risk::PROBABILITY, 4));
        more.push(Edit::new(r, fields::risk::IMPACT, 5));
        let g = EntityRef::new(Kind::Saving, ids());
        more.push(Edit::new(g, fields::saving::PROJECT, p.to_string()));
        more.push(Edit::new(g, fields::saving::VALUE, 1500.0));
        more.push(Edit::new(
            g,
            fields::saving::STATUS,
            enum_value(SavingStatus::Forecast),
        ));
        for op in s.prepare(me, 0, more, &mut ids) {
            s.apply(&op).unwrap();
        }
        let o = Overview::compute(&s, p, today).unwrap();
        assert_eq!(o.phases.len(), 5);
        assert_eq!(o.current_phase, Some(0));
        assert_eq!(o.actions_total, 3);
        assert_eq!(o.actions_overdue.len(), 1, "done cards are never overdue");
        assert_eq!(o.actions_overdue[0].title, "a");
        assert!((o.actions_done_fraction - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(o.risks_open, 1);
        assert_eq!(o.risks_high.len(), 1);
        assert_eq!(o.savings_by_status[&SavingStatus::Forecast], 1500.0);
        assert_eq!(o.days_to_deadline, Some(28));
        assert!(Overview::compute(&s, ids(), today).is_none());
    }
}
