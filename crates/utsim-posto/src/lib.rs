//! Work stations and the shared **unit folder** (factory or laboratory) of the UTSim
//! Qualidade and UTSim Segurança do Trabalho apps (stage QS8 of
//! `docs/plano_qualidade_e_seguranca.md`).
//!
//! The work station is the spine of both apps: quality characteristics, hazards, risks and
//! protective equipment point to it, and it points to an object of a `.ron` model by the
//! object's name.
//!
//! A unit folder is shared through the user's Drive. Every change is one small JSON file
//! in `ops/` that nobody edits again, so two people editing at once never lose an operation:
//! the sync client only adds files. The state is the replay of every operation in a fixed
//! order (instant, author, sequence, id), with the last write winning per record.

pub mod pasta;
pub mod registro;
pub mod restrita;

pub use pasta::{Desfeito, Unidade, UnidadeErro};
pub use registro::{Acao, Estado, Operacao};
pub use restrita::{Pessoa, Restrita, nomes_no_texto};

use serde::{Deserialize, Serialize};

/// Collection of the work stations in the unit state.
pub const POSTOS: &str = "postos";

/// A work station.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Posto {
    pub id: String,
    pub nome: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub descricao: String,
    /// Activities done at the station (from the time study elements when there is one).
    #[serde(default)]
    pub atividades: Vec<String>,
    /// People exposed per shift.
    #[serde(default)]
    pub pessoas_expostas: u32,
    /// Protective equipment required.
    #[serde(default)]
    pub epi: Vec<String>,
    /// Model file (relative to the unit folder or absolute) and object the station is.
    #[serde(default)]
    pub modelo: Option<String>,
    #[serde(default)]
    pub objeto: Option<String>,
}

/// Stations suggested from a model: one per process object (processor, combiner,
/// separator) not yet linked, with the protective equipment of the object.
pub fn sugerir_postos(
    model: &utsim_model::Model,
    existentes: &[Posto],
    modelo: &str,
) -> Vec<Posto> {
    use utsim_model::ObjectKind;
    model
        .objects
        .iter()
        .filter(|o| {
            matches!(
                o.kind,
                ObjectKind::Processor { .. }
                    | ObjectKind::Combiner { .. }
                    | ObjectKind::Separator { .. }
            )
        })
        .filter(|o| {
            !existentes
                .iter()
                .any(|p| p.objeto.as_deref() == Some(o.name.as_str()))
        })
        .map(|o| Posto {
            id: String::new(),
            nome: o.name.replace('_', " "),
            area: o.department.clone().unwrap_or_default(),
            descricao: String::new(),
            atividades: Vec::new(),
            pessoas_expostas: 1,
            epi: o.ppe.clone(),
            modelo: Some(modelo.to_string()),
            objeto: Some(o.name.clone()),
        })
        .collect()
}
