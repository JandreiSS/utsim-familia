//! Cause and effect diagram (`docs/plano_lean_seis_sigma.md`, 2.3): the effect, causes by
//! category (the 6M or the team's own), subcauses at any depth, each cause marked as open,
//! probable, verified or ruled out, with the evidence that decided it.

use serde::{Deserialize, Serialize};

use super::tree::{Node, forest};
use super::{ItemRef, items};
use crate::id::Id;
use crate::state::State;

pub const ITEM: &str = "causa";

pub mod fields {
    pub const TEXT: &str = "texto";
    /// One of [`super::SIX_M`] keys, or the name of a category of the team.
    pub const CATEGORY: &str = "categoria";
    pub const STATUS: &str = "estado";
    pub const EVIDENCE: &str = "evidencia";
    /// Deliverable: the effect (the head of the fish).
    pub const EFFECT: &str = "efeito";
}

/// The 6M: key in the file and name on screen.
pub const SIX_M: [(&str, &str); 6] = [
    ("mao_de_obra", "Mão de obra"),
    ("maquina", "Máquina"),
    ("metodo", "Método"),
    ("material", "Material"),
    ("medicao", "Medição"),
    ("meio_ambiente", "Meio ambiente"),
];

/// Name on screen of a category key (the team's own categories are their own name).
pub fn category_name(key: &str) -> String {
    SIX_M
        .iter()
        .find(|(k, _)| *k == key)
        .map_or_else(|| key.to_string(), |(_, n)| n.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CauseStatus {
    #[default]
    #[serde(rename = "aberta")]
    Open,
    #[serde(rename = "provavel")]
    Probable,
    #[serde(rename = "verificada")]
    Verified,
    #[serde(rename = "descartada")]
    RuledOut,
}

impl CauseStatus {
    pub const ALL: [CauseStatus; 4] = [
        CauseStatus::Open,
        CauseStatus::Probable,
        CauseStatus::Verified,
        CauseStatus::RuledOut,
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cause {
    pub id: Id,
    pub text: String,
    /// Category key; subcauses take their root's category.
    pub category: String,
    pub status: CauseStatus,
    pub evidence: String,
    pub depth: usize,
    pub parent: Option<Id>,
}

/// One bone of the fish: a category and its causes in depth-first order.
#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub category: String,
    pub causes: Vec<Cause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub effect: String,
    /// The 6M always (even empty), then the team's categories in order of first use.
    pub bones: Vec<Bone>,
    /// Causes whose parent links made a cycle (two people moved them at once).
    pub cycles: Vec<Id>,
}

impl Diagram {
    pub fn causes(&self) -> impl Iterator<Item = &Cause> {
        self.bones.iter().flat_map(|b| b.causes.iter())
    }

    pub fn with_status(&self, status: CauseStatus) -> impl Iterator<Item = &Cause> {
        self.causes().filter(move |c| c.status == status)
    }
}

/// The diagram of a deliverable.
pub fn diagram(state: &State, deliverable: Id) -> Diagram {
    let all = items(state, deliverable, ITEM);
    let pairs: Vec<(Id, Option<Id>)> = all.iter().map(|i| (i.id, i.parent())).collect();
    let (nodes, cycles) = forest(&pairs);
    let find = |id: Id| all.iter().find(|i| i.id == id).expect("node of the items");
    let mut bones: Vec<Bone> = SIX_M
        .iter()
        .map(|(k, _)| Bone {
            category: k.to_string(),
            causes: Vec::new(),
        })
        .collect();
    let mut root_cat = String::new();
    for Node { id, depth, parent } in nodes {
        let i: &ItemRef = find(id);
        if depth == 0 {
            root_cat = i.text(fields::CATEGORY);
            if root_cat.trim().is_empty() {
                root_cat = SIX_M[2].0.to_string();
            }
        }
        let cause = Cause {
            id,
            text: i.text(fields::TEXT),
            category: root_cat.clone(),
            status: i.entity.typed(fields::STATUS).unwrap_or_default(),
            evidence: i.text(fields::EVIDENCE),
            depth,
            parent,
        };
        match bones.iter_mut().find(|b| b.category == root_cat) {
            Some(b) => b.causes.push(cause),
            None => bones.push(Bone {
                category: root_cat.clone(),
                causes: vec![cause],
            }),
        }
    }
    Diagram {
        effect: state
            .entity(deliverable)
            .map(|e| e.text(fields::EFFECT))
            .unwrap_or_default(),
        bones,
        cycles,
    }
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
