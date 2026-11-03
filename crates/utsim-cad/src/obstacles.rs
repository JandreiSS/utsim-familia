//! Obstáculos a partir das camadas de parede e coluna (entrada da futura `utsim-nav`).

use serde::{Deserialize, Serialize};

use crate::geom::{Pt, dist, signed_area};
use crate::plan::Plan;
use crate::roles::{Role, RoleMap};

/// Obstáculos e aberturas derivados da planta, em metros.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Obstacles {
    /// Polígonos simples em sentido anti-horário (anel sem repetir o 1º ponto).
    pub polygons: Vec<Vec<Pt>>,
    /// Eixos das paredes abertas que geraram polígonos (para encaixe e desenho).
    pub wall_centerlines: Vec<[Pt; 2]>,
    /// Segmentos das camadas de porta (aberturas), à parte dos obstáculos.
    pub openings: Vec<[Pt; 2]>,
}

fn ccw(mut ring: Vec<Pt>) -> Vec<Pt> {
    if signed_area(&ring) < 0.0 {
        ring.reverse();
    }
    ring
}

/// Retângulo de espessura `t` em torno do segmento (None se o segmento é degenerado).
fn thick_segment(a: Pt, b: Pt, t: f64) -> Option<Vec<Pt>> {
    let l = dist(a, b);
    if l <= 0.0 {
        return None;
    }
    let h = t / 2.0;
    let n = [-(b[1] - a[1]) / l * h, (b[0] - a[0]) / l * h];
    Some(ccw(vec![
        [a[0] + n[0], a[1] + n[1]],
        [b[0] + n[0], b[1] + n[1]],
        [b[0] - n[0], b[1] - n[1]],
        [a[0] - n[0], a[1] - n[1]],
    ]))
}

/// Constrói os obstáculos: polilinhas fechadas de `Parede`/`Coluna` viram polígonos; as
/// abertas de parede viram um retângulo por segmento, com a espessura dada (padrão do
/// plano: 0,15 m). Polilinhas abertas cujas pontas coincidem (tolerância 1 mm) valem como
/// fechadas. Colunas abertas são ignoradas. A camada `Porta` só alimenta `openings`.
pub fn build_obstacles(plan: &Plan, roles: &RoleMap, wall_thickness: f64) -> Obstacles {
    let mut out = Obstacles::default();
    let eps = 1e-3;
    for layer in &plan.layers {
        let role = roles.role_of(&layer.name);
        if !matches!(role, Role::Parede | Role::Coluna | Role::Porta) {
            continue;
        }
        for pl in &layer.polylines {
            if role == Role::Porta {
                out.openings.extend(pl.segments());
                continue;
            }
            let mut pts = pl.points.clone();
            let mut closed = pl.closed;
            if !closed && pts.len() > 3 && dist(pts[0], pts[pts.len() - 1]) <= eps {
                pts.pop();
                closed = true;
            }
            if closed && pts.len() >= 3 {
                let ring = ccw(pts);
                if signed_area(&ring).abs() > 0.0 {
                    out.polygons.push(ring);
                }
            } else if role == Role::Parede && wall_thickness > 0.0 {
                for s in pl.segments() {
                    if let Some(r) = thick_segment(s[0], s[1], wall_thickness) {
                        out.polygons.push(r);
                        out.wall_centerlines.push(s);
                    }
                }
            }
        }
    }
    out
}
