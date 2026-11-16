//! Cause and effect diagram, the rules without any project state: the effect, causes by
//! category (the 6M or the team's own), subcauses at any depth, each cause open, probable,
//! verified or ruled out with the evidence; the diagram is built from the causes as saved
//! (id, parent, text, category, status, evidence) in sibling order.

use serde::{Deserialize, Serialize};

use crate::arvore::{Node, forest};

/// Item kind of a cause in the apps' files.
pub const ITEM: &str = "causa";

pub mod fields {
    pub const TEXT: &str = "texto";
    /// One of [`super::SIX_M`] keys, or the name of a category of the team.
    pub const CATEGORY: &str = "categoria";
    pub const STATUS: &str = "estado";
    pub const EVIDENCE: &str = "evidencia";
    /// Of the diagram: the effect (the head of the fish).
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

/// Name on screen of a category key, in the language of the moment (the team's own
/// categories are their own name). The names in [`SIX_M`] are the Portuguese ones.
pub fn category_name(key: &str) -> String {
    let chave = match key {
        "mao_de_obra" => "ishikawa-mao_de_obra",
        "maquina" => "ishikawa-maquina",
        "metodo" => "ishikawa-metodo",
        "material" => "ishikawa-material",
        "medicao" => "ishikawa-medicao",
        "meio_ambiente" => "ishikawa-meio_ambiente",
        _ => return key.to_string(),
    };
    crate::texto::tx(chave).to_string()
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

/// A cause as saved, before the tree is built.
#[derive(Debug, Clone, PartialEq)]
pub struct Input<I> {
    pub id: I,
    pub parent: Option<I>,
    pub text: String,
    /// Read only for the roots: subcauses take their root's category.
    pub category: String,
    pub status: CauseStatus,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cause<I> {
    pub id: I,
    pub text: String,
    /// Category key; subcauses take their root's category.
    pub category: String,
    pub status: CauseStatus,
    pub evidence: String,
    pub depth: usize,
    pub parent: Option<I>,
}

impl<I> Cause<I> {
    /// Text shown for the cause in pickers.
    pub fn label(&self) -> String {
        format!("{}: {}", category_name(&self.category), self.text)
    }
}

/// One bone of the fish: a category and its causes in depth-first order.
#[derive(Debug, Clone, PartialEq)]
pub struct Bone<I> {
    pub category: String,
    pub causes: Vec<Cause<I>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram<I> {
    pub effect: String,
    /// The 6M always (even empty), then the team's categories in order of first use.
    pub bones: Vec<Bone<I>>,
    /// Causes whose parent links made a cycle (two people moved them at once).
    pub cycles: Vec<I>,
}

impl<I> Diagram<I> {
    pub fn causes(&self) -> impl Iterator<Item = &Cause<I>> {
        self.bones.iter().flat_map(|b| b.causes.iter())
    }

    pub fn with_status(&self, status: CauseStatus) -> impl Iterator<Item = &Cause<I>> {
        self.causes().filter(move |c| c.status == status)
    }
}

/// The diagram of `effect` and the causes in sibling order. A root without a category
/// goes to "Método".
pub fn diagram<I: Ord + Copy>(effect: &str, inputs: &[Input<I>]) -> Diagram<I> {
    let pairs: Vec<(I, Option<I>)> = inputs.iter().map(|i| (i.id, i.parent)).collect();
    let (nodes, cycles) = forest(&pairs);
    let find = |id: I| {
        inputs
            .iter()
            .find(|i| i.id == id)
            .expect("node of the inputs")
    };
    let mut bones: Vec<Bone<I>> = SIX_M
        .iter()
        .map(|(k, _)| Bone {
            category: k.to_string(),
            causes: Vec::new(),
        })
        .collect();
    let mut root_cat = String::new();
    for Node { id, depth, parent } in nodes {
        let i = find(id);
        if depth == 0 {
            root_cat = i.category.clone();
            if root_cat.trim().is_empty() {
                root_cat = SIX_M[2].0.to_string();
            }
        }
        let cause = Cause {
            id,
            text: i.text.clone(),
            category: root_cat.clone(),
            status: i.status,
            evidence: i.evidence.clone(),
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
        effect: effect.to_string(),
        bones,
        cycles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: u32, parent: Option<u32>, cat: &str) -> Input<u32> {
        Input {
            id,
            parent,
            text: format!("causa {id}"),
            category: cat.into(),
            status: CauseStatus::Open,
            evidence: String::new(),
        }
    }

    #[test]
    fn bones_with_subcauses_and_team_categories() {
        let d = diagram(
            "Furo fora de posição",
            &[
                input(1, None, "maquina"),
                input(2, Some(1), "material"),
                input(3, None, ""),
                input(4, None, "Fornecedor"),
            ],
        );
        assert_eq!(d.bones.len(), 7);
        let maq = d.bones.iter().find(|b| b.category == "maquina").unwrap();
        assert_eq!(maq.causes.len(), 2);
        assert_eq!(
            maq.causes[1].category, "maquina",
            "subcause takes the root's"
        );
        assert_eq!(maq.causes[1].depth, 1);
        assert_eq!(d.bones[2].causes[0].id, 3, "no category: Método");
        assert_eq!(d.bones[6].category, "Fornecedor");
        assert_eq!(d.causes().count(), 4);
        assert_eq!(d.causes().next().unwrap().label(), "Máquina: causa 1");
        assert_eq!(category_name("x"), "x");
    }
}
