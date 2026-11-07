//! The link with the competence map of UTSim Seis Sigma (`docs/plano_ensino_e_jogo.md`,
//! 7.1, and decision N2 of the Seis Sigma plan): each mission a student solved becomes
//! `praticou` in the curriculum tools its concepts are tied to, by the package table
//! `competencias`. Writing in the program journal is the teacher's (an instructor), so
//! this module only lists the entries.

use std::collections::{BTreeMap, BTreeSet};

use crate::pacote::PacoteCarregado;

/// One entry to record: the student practised `ferramenta` in a solved mission.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Competencia {
    /// Student record in the class.
    pub registro: String,
    /// Tool id of the Seis Sigma curriculum.
    pub ferramenta: String,
    /// Mission id.
    pub missao: String,
    /// Text of the evidence, the same for the same package, mission and tool, so writing
    /// twice does not repeat the entry.
    pub evidencia: String,
}

/// The entries of the missions each student solved (`resolvidas`: record to mission ids),
/// in a stable order. Missions without a concept in the table give nothing.
pub fn competencias(
    p: &PacoteCarregado,
    resolvidas: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<Competencia> {
    let mut v = BTreeSet::new();
    for (registro, missoes) in resolvidas {
        for m in p.missoes.iter().filter(|m| missoes.contains(&m.missao.id)) {
            let ferramentas: BTreeSet<&String> = m
                .missao
                .conceitos
                .iter()
                .filter_map(|c| p.pacote.competencias.get(c))
                .flatten()
                .collect();
            for f in ferramentas {
                v.insert(Competencia {
                    registro: registro.clone(),
                    ferramenta: f.clone(),
                    missao: m.missao.id.clone(),
                    evidencia: format!(
                        "UTSim Ensino, {}: missão {} ({})",
                        p.pacote.titulo, m.missao.titulo, m.missao.id
                    ),
                });
            }
        }
    }
    v.into_iter().collect()
}

/// The tools the package ties to some concept, for the teacher to check against the
/// curriculum of the program.
pub fn ferramentas(p: &PacoteCarregado) -> BTreeSet<String> {
    p.pacote.competencias.values().flatten().cloned().collect()
}
