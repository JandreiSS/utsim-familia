//! SIPOC, written by hand (`docs/plano_lean_seis_sigma.md`, 2.1): suppliers, inputs, the
//! process in 5 to 7 steps, outputs and customers. The one generated from the UTSim model
//! comes with etapa L4.

use super::items;
use crate::id::Id;
use crate::state::State;

/// The five columns: item kind in the file and title on screen.
pub const COLUMNS: [(&str, &str); 5] = [
    ("fornecedor", "Fornecedores"),
    ("entrada", "Entradas"),
    ("etapa", "Processo (etapas)"),
    ("saida", "Saídas"),
    ("cliente", "Clientes"),
];

pub const STEP: &str = "etapa";
pub const TEXT: &str = "texto";

/// Usual number of process steps in a SIPOC.
pub const STEPS: std::ops::RangeInclusive<usize> = 5..=7;

#[derive(Debug, Clone, PartialEq)]
pub struct Sipoc {
    /// The texts of each column, in [`COLUMNS`] order.
    pub columns: Vec<Vec<(Id, String)>>,
}

impl Sipoc {
    pub fn steps(&self) -> usize {
        self.columns[2].len()
    }

    /// Columns left empty (by index).
    pub fn empty_columns(&self) -> Vec<usize> {
        (0..COLUMNS.len())
            .filter(|i| self.columns[*i].is_empty())
            .collect()
    }

    /// Whether the process has the usual 5 to 7 steps.
    pub fn steps_ok(&self) -> bool {
        STEPS.contains(&self.steps())
    }
}

pub fn of_deliverable(state: &State, deliverable: Id) -> Sipoc {
    Sipoc {
        columns: COLUMNS
            .iter()
            .map(|(k, _)| {
                items(state, deliverable, k)
                    .iter()
                    .map(|i| (i.id, i.text(TEXT)))
                    .collect()
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    #[test]
    fn columns_and_steps() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Sipoc);
        f.item(d, "fornecedor", None, vec![(TEXT, "Usina".into())]);
        for s in ["Receber", "Cortar", "Dobrar", "Soldar"] {
            f.item(d, STEP, None, vec![(TEXT, s.into())]);
        }
        let s = of_deliverable(&f.state, d);
        assert_eq!(s.steps(), 4);
        assert!(!s.steps_ok());
        assert_eq!(s.empty_columns(), [1, 3, 4]);
        assert_eq!(s.columns[2][1].1, "Cortar");
        f.item(d, STEP, None, vec![(TEXT, "Pintar".into())]);
        assert!(of_deliverable(&f.state, d).steps_ok());
    }
}
