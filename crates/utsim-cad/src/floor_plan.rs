//! Proposta do campo `floor_plan` do modelo (ainda não ligado ao `utsim-model`).
//!
//! O campo seria aditivo e opcional (`#[serde(default)]`), então modelos antigos continuam
//! válidos sem migração. Fica aqui, serializável em RON, até o dono decidir ligá-lo ao
//! formato do modelo (ver `docs/arquitetura.md`).

use serde::{Deserialize, Serialize};

use crate::calib::{Calibration, Transform};
use crate::geom::Pt;
use crate::roles::Role;

/// Camada com papel e visibilidade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloorPlanLayer {
    pub name: String,
    pub role: Role,
    #[serde(default = "yes")]
    pub visible: bool,
}

fn yes() -> bool {
    true
}

fn default_wall() -> f64 {
    0.15
}

/// Definição da planta no modelo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloorPlanDef {
    /// Caminho do DXF, relativo ao `.ron`.
    pub file: String,
    /// Hash do conteúdo do arquivo (`fnv1a64:...`), para avisar quando a planta mudou.
    pub hash: String,
    /// Transformação do desenho (já em metros) para o piso.
    #[serde(default = "ident")]
    pub transform: Transform,
    /// Calibração usada (registro, para reabrir o diálogo).
    #[serde(default)]
    pub calibration: Option<Calibration>,
    pub layers: Vec<FloorPlanLayer>,
    /// Espessura das paredes abertas, em metros.
    #[serde(default = "default_wall")]
    pub wall_thickness: f64,
    /// Origem do desenho no piso (reservado).
    #[serde(default)]
    pub origin: Pt,
}

fn ident() -> Transform {
    Transform::IDENTITY
}

/// Hash FNV-1a de 64 bits do conteúdo, em texto `fnv1a64:` mais 16 dígitos hexadecimais.
/// Não é criptográfico: serve só para detectar mudança do arquivo.
pub fn fnv1a_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{h:016x}")
}
