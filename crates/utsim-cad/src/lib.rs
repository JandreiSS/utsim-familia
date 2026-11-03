//! Núcleo da planta CAD do UTSim: leitura de DXF 2D, calibração, camadas, papéis e obstáculos.
//!
//! A crate é pura (sem interface, sem relógio, sem sorteio): a mesma entrada dá sempre a
//! mesma saída. O arquivo CAD não é lido pela simulação; ele alimenta o desenho no piso e a
//! geração dos obstáculos, cujo resultado fica no modelo.
//!
//! Decisão: o leitor de DXF é mínimo e próprio (texto ASCII, sem dependências), em vez da
//! crate `dxf`, para não prender o projeto a uma dependência pesada e manter a saída
//! determinística. DXF binário não é lido (a mensagem de erro orienta a salvar em texto).
//! Arquivos DWG não são lidos: converta para DXF no programa de CAD.

mod calib;
mod floor_plan;
mod geom;
mod obstacles;
mod plan;
mod reader;
mod roles;
mod simplify;
mod svg;

pub use calib::{Calibration, Transform};
pub use floor_plan::{FloorPlanDef, FloorPlanLayer, fnv1a_hash};
pub use geom::{Bbox, Pt};
pub use obstacles::{Obstacles, build_obstacles};
pub use plan::{Label, Layer, LayerSummary, Plan, Polyline};
pub use reader::{ReadOptions, Units, read_dxf};
pub use roles::{Role, RoleMap, suggest_role};
pub use simplify::simplify_polyline;
pub use svg::{FlatLines, SvgOptions, to_svg};

/// Erros da leitura e do tratamento da planta.
#[derive(Debug, thiserror::Error)]
pub enum CadError {
    /// O arquivo não é um DXF de texto válido.
    #[error("DXF inválido: {0}")]
    Invalid(String),
    /// Parâmetro fora do domínio (calibração com pontos iguais, distância não positiva etc.).
    #[error("parâmetro inválido: {0}")]
    Param(String),
}
