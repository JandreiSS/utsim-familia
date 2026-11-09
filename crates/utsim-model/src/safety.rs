//! Safety data of a work station, kept on the node itself (owner's direction of
//! 2026-11-08, `docs/plano_integracao_familia.md`, section 3): the hazards of the station
//! and the ergonomics marks of its elements. Edited in the Segurança tab of the node in
//! UTSim and in UTSim Segurança do Trabalho, both ways. Never read by the simulation.
//!
//! The scales are the ones of `utsim-seguranca-core` (probability and severity steps of
//! the risk matrix, posture steps 0 to 3, grip and vibration 0 to 2), checked here only
//! for range so that a damaged file is refused with a clear message.

use serde::{Deserialize, Serialize};

/// Safety block of an object.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SafetyDef {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hazards: Vec<HazardDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ergonomics: Vec<ErgonomicsMark>,
}

impl SafetyDef {
    pub fn is_empty(&self) -> bool {
        self.hazards.is_empty() && self.ergonomics.is_empty()
    }
}

/// Group of a hazard (the usual occupational groups).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HazardGroup {
    Physical,
    Chemical,
    Biological,
    Ergonomic,
    #[default]
    Accident,
}

/// A hazard of the work station.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HazardDef {
    /// Stable id inside the node (the apps use it to link incidents and FMEA lines).
    pub id: String,
    #[serde(default)]
    pub group: HazardGroup,
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub circumstance: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub injuries: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub controls: String,
    /// Step of the probability scale of the risk matrix (0 the lowest).
    #[serde(default)]
    pub probability: u8,
    /// Step of the severity scale (0 the lowest).
    #[serde(default)]
    pub severity: u8,
    /// Where it came from (`checklist:<inspection>:<item>`, `simulacao`, empty: typed).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
}

/// Ergonomics marks of one element of the time study at this station.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ErgonomicsMark {
    /// Operator (person) and element as `utsim tempos` names them.
    pub actor: String,
    pub element: String,
    /// Posture steps 0 to 3.
    #[serde(default)]
    pub trunk: u8,
    #[serde(default)]
    pub arms: u8,
    #[serde(default)]
    pub neck: u8,
    #[serde(default)]
    pub legs: u8,
    /// Load or force handled, kg.
    #[serde(default)]
    pub load_kg: f64,
    /// Grip and vibration steps 0 to 2.
    #[serde(default)]
    pub grip: u8,
    #[serde(default)]
    pub vibration: u8,
}

/// Steps of the default risk matrix (5 × 5).
pub const MATRIX_STEPS: u8 = 5;

/// Problems of the block (empty: valid).
pub fn problems(object: &str, s: &SafetyDef) -> Vec<String> {
    let mut out = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for h in &s.hazards {
        if h.id.trim().is_empty() {
            out.push(format!("'{object}': perigo sem id"));
        } else if !ids.insert(h.id.as_str()) {
            out.push(format!("'{object}': id de perigo repetido '{}'", h.id));
        }
        if h.description.trim().is_empty() {
            out.push(format!("'{object}': perigo '{}' sem descrição", h.id));
        }
        if h.probability >= MATRIX_STEPS || h.severity >= MATRIX_STEPS {
            out.push(format!(
                "'{object}': perigo '{}' com probabilidade ou severidade fora de 0 a {}",
                h.id,
                MATRIX_STEPS - 1
            ));
        }
    }
    for m in &s.ergonomics {
        if m.trunk > 3 || m.arms > 3 || m.neck > 3 || m.legs > 3 || m.grip > 2 || m.vibration > 2 {
            out.push(format!(
                "'{object}': marcação de ergonomia de '{}' fora das escalas",
                m.element
            ));
        }
        if !(m.load_kg.is_finite() && m.load_kg >= 0.0) {
            out.push(format!(
                "'{object}': carga de '{}' deve ser um número não negativo",
                m.element
            ));
        }
    }
    out
}
