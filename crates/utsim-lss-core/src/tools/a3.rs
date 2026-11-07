//! The A3 (`docs/plano_lean_seis_sigma.md`, 5.2, L2): one page composed of what the project
//! already has (charter, Pareto, CTQs, SIPOC, verified causes, root causes, C&E ranking,
//! FMEA priorities, actions, control plan), with a note of the team per section. Nothing is
//! copied: the page reads the other deliverables each time it is drawn.

use super::ap::Ap;
use super::ishikawa::CauseStatus;
use super::{Tool, cause_effect, fmea, ishikawa, of_tool, pareto, plans, sipoc, voc, whys};
use crate::date::Date;
use crate::domain::{Column, Project};
use crate::id::Id;
use crate::kanban;
use crate::op::Kind;
use crate::state::State;

pub use utsim_ferramentas::a3::Section;

/// An action of the plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanLine {
    pub title: String,
    pub column: Column,
    pub due: Option<Date>,
    pub who: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct A3 {
    pub title: String,
    pub problem: String,
    pub business_case: String,
    pub goal: String,
    pub baseline: String,
    pub deadline: Option<Date>,
    /// Vital few of the Pareto analyses: name and percentage.
    pub pareto_vital: Vec<(String, f64)>,
    /// CTQs: name, unit, limits.
    pub ctqs: Vec<String>,
    pub process_steps: Vec<String>,
    pub verified_causes: Vec<String>,
    pub root_causes: Vec<String>,
    /// The three inputs with the highest C&E totals.
    pub top_inputs: Vec<(String, f64)>,
    /// FMEA rows of high priority: mode and recommended action.
    pub fmea_high: Vec<(String, String)>,
    pub plan: Vec<PlanLine>,
    pub control_lines: Vec<String>,
    pub critical_gaps: usize,
    pub signals: usize,
    /// The team's note of each section, in [`Section::ALL`] order.
    pub notes: Vec<String>,
}

fn limits(c: &voc::Ctq) -> String {
    let n = |x: f64| {
        let s = format!("{x}");
        s.replace('.', ",")
    };
    let lim = match (c.lsl, c.usl) {
        (Some(l), Some(u)) => format!("{} a {}", n(l), n(u)),
        (Some(l), None) => format!("≥ {}", n(l)),
        (None, Some(u)) => format!("≤ {}", n(u)),
        (None, None) => String::new(),
    };
    let parts: Vec<String> = [c.unit.trim().to_string(), lim]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        c.name.clone()
    } else {
        format!("{} ({})", c.name, parts.join(", "))
    }
}

/// Composes the A3 of a project; `a3` is the deliverable holding the notes.
pub fn compose(state: &State, project: Id, a3: Option<Id>) -> A3 {
    let mut out = A3::default();
    if let Some(e) = state.entity(project).filter(|e| e.kind == Kind::Project) {
        let p = Project::read(project, e);
        out.title = p.title;
        out.problem = p.problem;
        out.business_case = p.business_case;
        out.goal = p.goal;
        out.baseline = p.baseline;
        out.deadline = p.deadline;
    }
    for d in of_tool(state, project, Tool::Pareto) {
        out.pareto_vital.extend(
            pareto::of_deliverable(state, d.id)
                .vital()
                .map(|r| (r.name.clone(), r.pct)),
        );
    }
    for d in of_tool(state, project, Tool::Voc) {
        out.ctqs.extend(voc::tree(state, d.id).ctqs().map(limits));
    }
    for d in of_tool(state, project, Tool::Sipoc) {
        out.process_steps.extend(
            sipoc::of_deliverable(state, d.id).columns[2]
                .iter()
                .map(|(_, t)| t.clone()),
        );
    }
    for d in of_tool(state, project, Tool::Ishikawa) {
        let g = ishikawa::diagram(state, d.id);
        out.verified_causes.extend(
            g.with_status(CauseStatus::Verified)
                .map(|c| format!("{} ({})", c.text, ishikawa::category_name(&c.category))),
        );
    }
    for d in of_tool(state, project, Tool::FiveWhys) {
        for c in whys::chains(state, d.id) {
            out.root_causes
                .extend(c.root_causes().iter().map(|w| w.text.clone()));
        }
    }
    for d in of_tool(state, project, Tool::CauseEffect) {
        let m = cause_effect::of_deliverable(state, d.id);
        out.top_inputs.extend(
            m.ranking
                .iter()
                .take(3)
                .map(|i| (m.inputs[*i].name.clone(), m.inputs[*i].total)),
        );
    }
    let (table, _) = fmea::project_table(state, project);
    for d in of_tool(state, project, Tool::Fmea) {
        for r in fmea::ranked(&fmea::rows(state, d.id), &table) {
            if r.ap(&table) == Some(Ap::High) {
                out.fmea_high.push((r.mode.clone(), r.action.clone()));
            }
        }
    }
    for (col, cards) in kanban::board(state, project) {
        for a in cards {
            out.plan.push(PlanLine {
                title: a.title,
                column: col,
                due: a.due,
                who: a.who,
            });
        }
    }
    for d in of_tool(state, project, Tool::ControlPlan) {
        out.control_lines.extend(
            plans::control_lines(state, d.id)
                .into_iter()
                .map(|l| l.characteristic),
        );
    }
    out.critical_gaps = fmea::critical_without_control(state, project).len();
    out.signals = of_tool(state, project, Tool::ResponsePlan)
        .iter()
        .map(|d| super::items(state, d.id, plans::RESPONSE_ITEM).len())
        .sum();
    let notes = a3.and_then(|id| state.entity(id));
    out.notes = Section::ALL
        .iter()
        .map(|s| notes.map(|e| e.text(s.note_field())).unwrap_or_default())
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fields as df;
    use crate::op::Edit;
    use crate::tools::Deliverable;
    use crate::tools::testing::Fixture;

    #[test]
    fn composed_of_the_deliverables() {
        let mut f = Fixture::new();
        let p = f.project;
        f.commit(vec![
            Edit::new(
                Project::entity_ref(p),
                df::project::PROBLEM,
                "Refugo de 8 % na linha 2",
            ),
            Edit::new(
                Project::entity_ref(p),
                df::project::GOAL,
                "Refugo abaixo de 2 %",
            ),
        ]);
        let par = f.deliverable(Tool::Pareto);
        for (n, c) in [("Rebarba", 70), ("Risco", 20), ("Mancha", 10)] {
            f.item(
                par,
                pareto::ITEM,
                None,
                vec![
                    (pareto::fields::NAME, n.into()),
                    (pareto::fields::COUNT, c.into()),
                ],
            );
        }
        let ish = f.deliverable(Tool::Ishikawa);
        let c = f.item(
            ish,
            ishikawa::ITEM,
            None,
            vec![
                (ishikawa::fields::TEXT, "Ferramenta gasta".into()),
                (ishikawa::fields::CATEGORY, "maquina".into()),
            ],
        );
        f.set(c, ishikawa::fields::STATUS, "verificada");
        let fm = f.deliverable(Tool::Fmea);
        f.item(
            fm,
            fmea::ITEM,
            None,
            vec![
                (fmea::fields::MODE, "Rebarba".into()),
                (fmea::fields::ACTION, "Trocar a ferramenta".into()),
                (fmea::fields::S, 9.into()),
                (fmea::fields::O, 5.into()),
                (fmea::fields::D, 3.into()),
            ],
        );
        f.item(
            fm,
            fmea::ITEM,
            None,
            vec![
                (fmea::fields::S, 2.into()),
                (fmea::fields::O, 2.into()),
                (fmea::fields::D, 2.into()),
            ],
        );
        let card = f.id();
        let e = kanban::new_card(&f.state, p, card, "Trocar a ferramenta", Column::Doing);
        f.commit(e);
        let a3 = f.id();
        f.commit(Deliverable::create(p, None, Tool::A3, a3, "A3"));
        f.commit(vec![Edit::new(
            Deliverable::entity_ref(a3),
            Section::Goal.note_field(),
            "Meta acordada com o patrocinador",
        )]);
        let page = compose(&f.state, p, Some(a3));
        assert_eq!(page.problem, "Refugo de 8 % na linha 2");
        assert_eq!(page.goal, "Refugo abaixo de 2 %");
        assert_eq!(
            page.pareto_vital,
            [("Rebarba".to_string(), 70.0), ("Risco".to_string(), 20.0)]
        );
        assert_eq!(page.verified_causes, ["Ferramenta gasta (Máquina)"]);
        assert_eq!(
            page.fmea_high,
            [("Rebarba".to_string(), "Trocar a ferramenta".to_string())]
        );
        assert_eq!(page.plan.len(), 1);
        assert_eq!(page.plan[0].column, Column::Doing);
        assert_eq!(page.critical_gaps, 1, "S 9 row without control plan");
        assert_eq!(page.notes[2], "Meta acordada com o patrocinador");
        assert_eq!(page.notes.len(), Section::ALL.len());
        let ctq = voc::Ctq {
            id: c,
            name: "Diâmetro".into(),
            definition: String::new(),
            unit: "mm".into(),
            lsl: Some(9.95),
            usl: Some(10.05),
            target: None,
        };
        assert_eq!(limits(&ctq), "Diâmetro (mm, 9,95 a 10,05)");
    }
}
