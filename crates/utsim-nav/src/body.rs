//! Áreas ocupadas dos objetos, obstáculos, pontos de acesso e folga dos recursos móveis.

use serde::{Deserialize, Serialize};

use crate::geom::{Pt, rectangle, to_world};

/// Polígono que nenhum recurso atravessa: área ocupada de um objeto, parede, coluna ou
/// obstáculo desenhado à mão. Coordenadas do piso em metros; o anel não repete o 1º ponto.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Obstacle {
    pub name: String,
    pub polygon: Vec<Pt>,
}

impl Obstacle {
    /// Mensagem de erro, se o polígono não serve (menos de 3 pontos, coordenada não finita).
    pub fn problem(&self) -> Option<String> {
        if self.polygon.len() < 3 {
            return Some(format!(
                "obstáculo '{}': o polígono precisa de ao menos 3 pontos",
                self.name
            ));
        }
        if !self.polygon.iter().flatten().all(|v| v.is_finite()) {
            return Some(format!(
                "obstáculo '{}': as coordenadas precisam ser finitas",
                self.name
            ));
        }
        None
    }
}

/// Forma de um objeto fixo, com o que muda o tamanho da sua área.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Source,
    Queue,
    Processor,
    Combiner,
    Separator,
    Sink,
    Conveyor { length: f64 },
    Rack { bays: usize, bay_width: f64 },
}

/// Maior esteira considerada (igual ao desenho 3D).
const MAX_BELT: f64 = 400.0;
/// Mais vãos considerados num porta-paletes (igual ao desenho 3D).
const MAX_BAYS: usize = 60;
/// Largura da esteira e profundidade do porta-paletes (m), as mesmas do desenho 3D.
pub const BELT_WIDTH: f64 = 1.1;
pub const RACK_DEPTH: f64 = 1.0;

fn positive_or(v: f64, default: f64) -> f64 {
    if v.is_finite() && v > 0.0 { v } else { default }
}

/// Meias medidas `[x, y]` da área ocupada em coordenadas do objeto, sem escala. São as
/// mesmas caixas do desenho 3D (`utsim-scene`): processador 1,8 × 1,8 m, fila 4 × 1,4 m,
/// esteira pelo comprimento × 1,1 m, porta-paletes pelos vãos × 1 m.
pub fn half_extents(shape: Shape) -> Pt {
    match shape {
        Shape::Source => [0.8, 0.8],
        Shape::Queue => [2.0, 0.7],
        Shape::Processor | Shape::Sink => [0.9, 0.9],
        Shape::Combiner | Shape::Separator => [1.0, 1.0],
        Shape::Conveyor { length } => [
            positive_or(length, 5.0).min(MAX_BELT) / 2.0,
            BELT_WIDTH / 2.0,
        ],
        Shape::Rack { bays, bay_width } => [
            bays.clamp(1, MAX_BAYS) as f64 * positive_or(bay_width, 1.0) / 2.0,
            RACK_DEPTH / 2.0,
        ],
    }
}

/// Distância extra entre a borda inflada do obstáculo e o ponto de acesso padrão (m).
pub const ACCESS_GAP: f64 = 0.05;

/// Objeto fixo no piso.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub name: String,
    pub shape: Shape,
    pub center: Pt,
    /// Giro em radianos, anti-horário visto de cima (a esteira segue a sua saída).
    pub yaw: f64,
    /// Escala uniforme da área (`visual.scale`).
    pub scale: f64,
    /// Ponto de acesso dado no modelo, em coordenadas do objeto.
    pub access: Option<Pt>,
}

impl Body {
    pub fn half(&self) -> Pt {
        let h = half_extents(self.shape);
        let s = positive_or(self.scale, 1.0);
        [h[0] * s, h[1] * s]
    }

    /// Retângulo da área ocupada.
    pub fn polygon(&self) -> Vec<Pt> {
        rectangle(self.center, self.half(), self.yaw)
    }

    pub fn obstacle(&self) -> Obstacle {
        Obstacle {
            name: self.name.clone(),
            polygon: self.polygon(),
        }
    }

    /// Ponto de acesso no piso para um recurso com esta `clearance`.
    ///
    /// Padrão: no lado `-y` do objeto (a frente do porta-paletes, o lado da máquina onde a
    /// pessoa trabalha, o lado da esteira), no meio dele, à distância da folga mais
    /// [`ACCESS_GAP`] da borda: a empilhadeira encosta de frente no porta-paletes e a
    /// pessoa fica ao lado da máquina. Com `access` no modelo, vale o ponto dado.
    pub fn access_point(&self, clearance: f64) -> Pt {
        let local = self
            .access
            .unwrap_or([0.0, -(self.half()[1] + clearance.max(0.0) + ACCESS_GAP)]);
        to_world(self.center, local, self.yaw)
    }
}

/// Papel do recurso móvel, que define a sua largura.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Person,
    Forklift,
    Agv,
    Vehicle,
}

/// Margem padrão somada à meia largura do recurso (m), do plano.
pub const MARGIN: f64 = 0.3;

/// Meia largura do recurso (m): pessoa 0,3, empilhadeira 0,6, AGV e outro veículo 0,5.
pub fn half_width(role: Role) -> f64 {
    match role {
        Role::Person => 0.3,
        Role::Forklift => 0.6,
        Role::Agv | Role::Vehicle => 0.5,
    }
}

/// Folga padrão: meia largura mais [`MARGIN`]. Uma passagem só abre para o recurso quando
/// é mais larga que o dobro da folga.
pub fn default_clearance(role: Role) -> f64 {
    half_width(role) + MARGIN
}

/// Papel pelo nome do ativo (`visual.asset`), como no desenho 3D.
pub fn role_of_asset(asset: &str) -> Option<Role> {
    let a = asset.to_lowercase();
    if a.contains("empilhadeira") || a.contains("forklift") {
        Some(Role::Forklift)
    } else if a.contains("agv") {
        Some(Role::Agv)
    } else if a.contains("operador") || a.contains("pessoa") || a.contains("person") {
        Some(Role::Person)
    } else {
        None
    }
}

/// Obstáculos da planta CAD (paredes e colunas já em metros, no piso), com nomes
/// `prefixo_1`, `prefixo_2`... na ordem da planta.
pub fn cad_obstacles(obs: &utsim_cad::Obstacles, prefix: &str) -> Vec<Obstacle> {
    obs.polygons
        .iter()
        .enumerate()
        .map(|(i, p)| Obstacle {
            name: format!("{prefix}_{}", i + 1),
            polygon: p.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::point_polygon;

    fn rack() -> Body {
        Body {
            name: "estante".into(),
            shape: Shape::Rack {
                bays: 4,
                bay_width: 1.5,
            },
            center: [10.0, 5.0],
            yaw: 0.0,
            scale: 1.0,
            access: None,
        }
    }

    #[test]
    fn rack_footprint_follows_its_bays() {
        let r = rack();
        assert_eq!(r.half(), [3.0, 0.5]);
        // The forklift (clearance 0.9) stops in front of the rack, at -y.
        let a = r.access_point(0.9);
        assert!((a[0] - 10.0).abs() < 1e-12);
        assert!((a[1] - (5.0 - 0.5 - 0.9 - ACCESS_GAP)).abs() < 1e-12);
        assert!((point_polygon(a, &r.polygon()) - 0.95).abs() < 1e-12);
    }

    #[test]
    fn rotated_and_scaled_bodies() {
        let mut b = rack();
        b.shape = Shape::Processor;
        b.scale = 2.0;
        b.yaw = std::f64::consts::FRAC_PI_2;
        assert_eq!(b.half(), [1.8, 1.8]);
        // Turned a quarter: the access side -y becomes +x.
        let a = b.access_point(0.5);
        assert!((a[0] - (10.0 + 1.8 + 0.55)).abs() < 1e-9 && (a[1] - 5.0).abs() < 1e-9);
        b.access = Some([0.0, 3.0]);
        let a = b.access_point(0.5);
        assert!((a[0] - 7.0).abs() < 1e-9 && (a[1] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn clearances_and_roles() {
        assert_eq!(role_of_asset("empilhadeira_eletrica"), Some(Role::Forklift));
        assert_eq!(role_of_asset("operador"), Some(Role::Person));
        assert_eq!(role_of_asset("caixa"), None);
        assert!((default_clearance(Role::Person) - 0.6).abs() < 1e-12);
        assert!((default_clearance(Role::Forklift) - 0.9).abs() < 1e-12);
        let o = Obstacle {
            name: "x".into(),
            polygon: vec![[0.0, 0.0], [1.0, 0.0]],
        };
        assert!(o.problem().is_some());
    }
}
