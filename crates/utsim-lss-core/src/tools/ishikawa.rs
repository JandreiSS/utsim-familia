//! Cause and effect diagram (`docs/plano_lean_seis_sigma.md`, 2.3): the effect, causes by
//! category (the 6M or the team's own), subcauses at any depth, each cause marked as open,
//! probable, verified or ruled out, with the evidence that decided it. The rules are in
//! `utsim_ferramentas::ishikawa` (decision G3); this module reads the journal.

use super::{ItemRef, items};
use crate::id::Id;
use crate::state::State;

pub use utsim_ferramentas::ishikawa::{CauseStatus, ITEM, SIX_M, category_name, fields};

pub type Cause = utsim_ferramentas::ishikawa::Cause<Id>;
pub type Bone = utsim_ferramentas::ishikawa::Bone<Id>;
pub type Diagram = utsim_ferramentas::ishikawa::Diagram<Id>;

/// The diagram of a deliverable.
pub fn diagram(state: &State, deliverable: Id) -> Diagram {
    let inputs: Vec<utsim_ferramentas::ishikawa::Input<Id>> = items(state, deliverable, ITEM)
        .iter()
        .map(|i: &ItemRef| utsim_ferramentas::ishikawa::Input {
            id: i.id,
            parent: i.parent(),
            text: i.text(fields::TEXT),
            category: i.text(fields::CATEGORY),
            status: i.entity.typed(fields::STATUS).unwrap_or_default(),
            evidence: i.text(fields::EVIDENCE),
        })
        .collect();
    let effect = state
        .entity(deliverable)
        .map(|e| e.text(fields::EFFECT))
        .unwrap_or_default();
    utsim_ferramentas::ishikawa::diagram(&effect, &inputs)
}

/// Every cause of the project's Ishikawa diagrams, for the five whys link: `(deliverable,
/// cause)`.
pub fn project_causes(state: &State, project: Id) -> Vec<(Id, Cause)> {
    super::of_tool(state, project, super::Tool::Ishikawa)
        .iter()
        .flat_map(|d| {
            diagram(state, d.id)
                .causes()
                .cloned()
                .map(|c| (d.id, c))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Text of every Ishikawa cause in the state, by id (deleted ones are absent).
pub fn project_causes_by_id(state: &State) -> Vec<(Id, String)> {
    state
        .of_kind(crate::op::Kind::ToolItem)
        .filter(|(_, e)| e.text(super::fields::item::ITEM_KIND) == ITEM)
        .map(|(id, e)| (id, e.text(fields::TEXT)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::enum_value;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    #[test]
    fn bones_tree_and_statuses() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Ishikawa);
        f.set(d, fields::EFFECT, "Refugo por rebarba");
        let a = f.item(
            d,
            ITEM,
            None,
            vec![
                (fields::TEXT, "Ferramenta gasta".into()),
                (fields::CATEGORY, "maquina".into()),
            ],
        );
        let a1 = f.item(
            d,
            ITEM,
            Some(a),
            vec![(fields::TEXT, "Troca só quando quebra".into())],
        );
        f.item(
            d,
            ITEM,
            Some(a1),
            vec![(fields::TEXT, "Sem plano de troca".into())],
        );
        let b = f.item(
            d,
            ITEM,
            None,
            vec![
                (fields::TEXT, "Clima quente".into()),
                (fields::CATEGORY, "Fornecedor".into()),
            ],
        );
        f.set(a1, fields::STATUS, enum_value(CauseStatus::Verified));
        f.set(b, fields::STATUS, enum_value(CauseStatus::RuledOut));
        let g = diagram(&f.state, d);
        assert_eq!(g.effect, "Refugo por rebarba");
        assert_eq!(g.bones.len(), 7, "6M plus the team's category");
        let maq = &g.bones[1];
        assert_eq!(maq.category, "maquina");
        let depth: Vec<usize> = maq.causes.iter().map(|c| c.depth).collect();
        assert_eq!(depth, [0, 1, 2]);
        assert!(
            maq.causes.iter().all(|c| c.category == "maquina"),
            "subcauses inherit"
        );
        assert_eq!(g.bones[6].category, "Fornecedor");
        assert_eq!(category_name("Fornecedor"), "Fornecedor");
        assert_eq!(category_name("medicao"), "Medição");
        let verified: Vec<&str> = g
            .with_status(CauseStatus::Verified)
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(verified, ["Troca só quando quebra"]);
        assert_eq!(project_causes(&f.state, f.project).len(), 4);
        // A cycle made by two people: nothing disappears.
        f.set(a, crate::tools::fields::item::PARENT, a1.to_string());
        let g = diagram(&f.state, d);
        assert_eq!(g.causes().count(), 4);
        assert_eq!(g.cycles.len(), 1);
    }
}
