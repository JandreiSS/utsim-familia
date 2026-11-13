//! Trajetos desenhados por nós (esteiras e caminhos no piso), como se desenham ruas: cada
//! nó tem posição 3D (a altura conta) e, nos nós do meio, um raio de curva opcional. O
//! trajeto real é a linha que passa pelos nós com a quina arredondada por um arco
//! tangente aos dois trechos (o mesmo arco do desenho 3D), e o comprimento que entra no
//! resultado é o dessa linha em 3D: subir 1 m numa esteira de 4 m de planta dá
//! √(4² + 1²) ≈ 4,12 m.
//!
//! Regras:
//! - O arco fica no plano do piso; a altura varia em linha reta com a distância em planta,
//!   de nó a nó (dentro do arco, da altura de um ponto de tangência à do outro).
//! - O raio pedido é reduzido quando não cabe: o ponto de tangência não passa da metade de
//!   nenhum dos dois trechos (o raio usado é o [`Fillet::radius`]).
//! - O arco é amostrado em passos de no máximo [`ARC_STEP`] (2°): o comprimento amostrado
//!   fica a menos de 0,02 % do arco exato.

use serde::{Deserialize, Serialize};

use crate::geom::Pt;

/// Ponto 3D `[x, y, z]` (m), `z` para cima.
pub type P3 = [f64; 3];

/// Maior ângulo de um passo do arco amostrado (rad).
pub const ARC_STEP: f64 = std::f64::consts::PI / 90.0;

/// Um nó do trajeto.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathNode {
    /// Posição (m). Numa esteira é relativa à posição do objeto; num corredor, absoluta.
    pub at: P3,
    /// Raio da curva neste nó (m); ausente ou zero: quina viva. Só vale nos nós do meio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
}

impl PathNode {
    pub fn new(at: P3) -> Self {
        PathNode { at, radius: None }
    }

    pub fn curved(at: P3, radius: f64) -> Self {
        PathNode {
            at,
            radius: Some(radius),
        }
    }
}

/// Arco de concordância num nó do meio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fillet {
    /// Raio usado (m), depois de reduzido para caber.
    pub radius: f64,
    /// Quanto o arco gira (rad, sempre ≥ 0).
    pub angle: f64,
    /// Pontos de tangência no trecho que chega e no que sai (em planta).
    pub start: Pt,
    pub end: Pt,
    /// Centro do arco (em planta).
    pub center: Pt,
}

fn plan(p: P3) -> Pt {
    [p[0], p[1]]
}

fn plan_dist(a: P3, b: P3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Distância 3D.
pub fn dist3(a: P3, b: P3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Comprimento de uma polilinha 3D.
pub fn length3(points: &[P3]) -> f64 {
    points.windows(2).map(|w| dist3(w[0], w[1])).sum()
}

fn unit(a: P3, b: P3) -> Option<Pt> {
    let d = plan_dist(a, b);
    (d > 1e-9).then(|| [(b[0] - a[0]) / d, (b[1] - a[1]) / d])
}

/// Arco do nó `i` (do meio), se ele tem raio e o trajeto vira ali.
pub fn fillet(nodes: &[PathNode], i: usize) -> Option<Fillet> {
    if i == 0 || i + 1 >= nodes.len() {
        return None;
    }
    let r = nodes[i].radius.filter(|r| r.is_finite() && *r > 0.0)?;
    let (a, b, c) = (nodes[i - 1].at, nodes[i].at, nodes[i + 1].at);
    let u = unit(a, b)?;
    let v = unit(b, c)?;
    let cos = (u[0] * v[0] + u[1] * v[1]).clamp(-1.0, 1.0);
    let angle = cos.acos();
    // Straight on (nothing to round) or a U-turn (no arc fits).
    if angle < 1e-6 || angle > std::f64::consts::PI - 1e-6 {
        return None;
    }
    let half = (angle / 2.0).tan();
    let room = (plan_dist(a, b) / 2.0).min(plan_dist(b, c) / 2.0);
    let t = (r * half).min(room);
    let radius = t / half;
    let start = [b[0] - u[0] * t, b[1] - u[1] * t];
    let end = [b[0] + v[0] * t, b[1] + v[1] * t];
    let left = u[0] * v[1] - u[1] * v[0] > 0.0;
    let n = if left { [-u[1], u[0]] } else { [u[1], -u[0]] };
    let center = [start[0] + n[0] * radius, start[1] + n[1] * radius];
    Some(Fillet {
        radius,
        angle,
        start,
        end,
        center,
    })
}

/// Altura no ponto `p` (em planta) do trecho reto `a`–`b`, pela distância em planta.
fn z_on(a: P3, b: P3, p: Pt) -> f64 {
    let total = plan_dist(a, b);
    if total <= 1e-12 {
        return b[2];
    }
    let s = ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt() / total;
    a[2] + (b[2] - a[2]) * s.clamp(0.0, 1.0)
}

/// A linha real do trajeto: os nós com as curvas amostradas, em 3D.
pub fn sample(nodes: &[PathNode]) -> Vec<P3> {
    let mut out: Vec<P3> = Vec::with_capacity(nodes.len());
    for (i, n) in nodes.iter().enumerate() {
        match fillet(nodes, i) {
            None => out.push(n.at),
            Some(f) => {
                let (a, b, c) = (nodes[i - 1].at, n.at, nodes[i + 1].at);
                let z0 = z_on(a, b, f.start);
                let z1 = z_on(c, b, f.end);
                let a0 = (f.start[1] - f.center[1]).atan2(f.start[0] - f.center[0]);
                let a1 = (f.end[1] - f.center[1]).atan2(f.end[0] - f.center[0]);
                let mut sweep = a1 - a0;
                // The short way round (the arc never turns more than the corner).
                while sweep > std::f64::consts::PI {
                    sweep -= std::f64::consts::TAU;
                }
                while sweep < -std::f64::consts::PI {
                    sweep += std::f64::consts::TAU;
                }
                let steps = ((sweep.abs() / ARC_STEP).ceil() as usize).max(1);
                for k in 0..=steps {
                    let s = k as f64 / steps as f64;
                    let ang = a0 + sweep * s;
                    out.push([
                        f.center[0] + f.radius * ang.cos(),
                        f.center[1] + f.radius * ang.sin(),
                        z0 + (z1 - z0) * s,
                    ]);
                }
            }
        }
    }
    out.dedup_by(|a, b| dist3(*a, *b) < 1e-12);
    out
}

/// Comprimento real (3D, com as curvas) do trajeto.
pub fn path_length(nodes: &[PathNode]) -> f64 {
    length3(&sample(nodes))
}

/// A linha em planta (para obstáculos, faixas no piso e o desenho).
pub fn plan_line(points: &[P3]) -> Vec<Pt> {
    points.iter().map(|p| plan(*p)).collect()
}

/// Inclinação de um passo: subida ÷ distância em planta (0,05 = 5 %). Vertical: infinita.
pub fn grade(a: P3, b: P3) -> f64 {
    let run = plan_dist(a, b);
    let rise = b[2] - a[2];
    if run <= 1e-12 {
        return if rise.abs() <= 1e-12 {
            0.0
        } else {
            f64::INFINITY * rise.signum()
        };
    }
    rise / run
}

/// Graus de uma inclinação (`grade` → ângulo).
pub fn degrees(grade: f64) -> f64 {
    grade.atan().to_degrees()
}

/// Um trecho entre dois nós vizinhos, como o painel mostra enquanto se desenha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    /// Comprimento 3D do trecho reto entre os nós (sem descontar as curvas).
    pub length: f64,
    /// Inclinação (subida ÷ planta).
    pub grade: f64,
}

pub fn legs(nodes: &[PathNode]) -> Vec<Leg> {
    nodes
        .windows(2)
        .map(|w| Leg {
            length: dist3(w[0].at, w[1].at),
            grade: grade(w[0].at, w[1].at),
        })
        .collect()
}

/// Maior inclinação (em módulo) ao longo da linha real.
pub fn max_grade(points: &[P3]) -> f64 {
    points
        .windows(2)
        .map(|w| grade(w[0], w[1]).abs())
        .fold(0.0, f64::max)
}

/// Ponto e direção (unitária, 3D) a `s` metros do início, medidos ao longo da linha;
/// `s` fora do trajeto fica na ponta.
pub fn point_at(points: &[P3], s: f64) -> Option<(P3, P3)> {
    let first = *points.first()?;
    if points.len() == 1 {
        return Some((first, [1.0, 0.0, 0.0]));
    }
    let mut left = s.max(0.0);
    let last = points.len() - 2;
    for (k, w) in points.windows(2).enumerate() {
        let d = dist3(w[0], w[1]);
        if d <= 1e-12 {
            continue;
        }
        if left <= d || k == last {
            let t = (left / d).min(1.0);
            let dir = [
                (w[1][0] - w[0][0]) / d,
                (w[1][1] - w[0][1]) / d,
                (w[1][2] - w[0][2]) / d,
            ];
            let p = [
                w[0][0] + dir[0] * d * t,
                w[0][1] + dir[1] * d * t,
                w[0][2] + dir[2] * d * t,
            ];
            return Some((p, dir));
        }
        left -= d;
    }
    Some((*points.last()?, [1.0, 0.0, 0.0]))
}

/// Contorno em planta de uma faixa de largura `width` em volta da linha (polígono simples
/// enquanto a linha não se cruza): o lado esquerdo e o direito, de volta.
pub fn band(line: &[Pt], width: f64) -> Vec<Pt> {
    let left = crate::aisles::offset_polyline(line, width / 2.0);
    let mut right = crate::aisles::offset_polyline(line, -width / 2.0);
    right.reverse();
    let mut out = left;
    out.extend(right);
    out
}

/// Desloca todos os nós (mover vários juntos).
pub fn translate(nodes: &mut [PathNode], d: P3) {
    for n in nodes {
        for (v, dv) in n.at.iter_mut().zip(d) {
            *v += dv;
        }
    }
}

/// Alturas dos nós `from..=to` em linha reta entre as duas pontas, pela distância em
/// planta: o trecho fica em inclinação constante (ou nivelado, se as pontas têm a mesma
/// altura).
pub fn constant_slope(nodes: &mut [PathNode], from: usize, to: usize) {
    if from >= to || to >= nodes.len() {
        return;
    }
    let mut run = vec![0.0];
    for k in from..to {
        let d = plan_dist(nodes[k].at, nodes[k + 1].at);
        run.push(run.last().copied().unwrap_or(0.0) + d);
    }
    let total = *run.last().unwrap_or(&0.0);
    let (z0, z1) = (nodes[from].at[2], nodes[to].at[2]);
    for (j, k) in (from..=to).enumerate() {
        let s = if total > 0.0 { run[j] / total } else { 0.0 };
        nodes[k].at[2] = z0 + (z1 - z0) * s;
    }
}

/// Problema de um trajeto com ao menos `min` nós (mensagem em português), se houver.
pub fn problem(nodes: &[PathNode], what: &str) -> Option<String> {
    if nodes.len() < 2 {
        return Some(format!("{what}: o trajeto precisa de ao menos 2 nós"));
    }
    if !nodes.iter().all(|n| n.at.iter().all(|v| v.is_finite())) {
        return Some(format!(
            "{what}: as coordenadas dos nós precisam ser finitas"
        ));
    }
    if nodes
        .iter()
        .any(|n| n.radius.is_some_and(|r| !(r.is_finite() && r >= 0.0)))
    {
        return Some(format!(
            "{what}: o raio da curva deve ser um número ≥ 0 (metros)"
        ));
    }
    if path_length(nodes) <= 1e-9 {
        return Some(format!("{what}: o trajeto tem comprimento zero"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn a_straight_flat_path_is_its_plan_length() {
        let n = [
            PathNode::new([0.0, 0.0, 1.0]),
            PathNode::new([4.0, 0.0, 1.0]),
        ];
        assert!(close(path_length(&n), 4.0, 1e-12));
        assert_eq!(max_grade(&sample(&n)), 0.0);
    }

    #[test]
    fn height_lengthens_the_path() {
        let n = [
            PathNode::new([0.0, 0.0, 0.0]),
            PathNode::new([4.0, 0.0, 1.0]),
        ];
        assert!(close(path_length(&n), 17f64.sqrt(), 1e-12));
        assert!(close(max_grade(&sample(&n)), 0.25, 1e-12));
        assert!(close(degrees(0.25), 14.036, 1e-3));
    }

    #[test]
    fn a_rounded_corner_is_shorter_than_the_sharp_one() {
        let sharp = [
            PathNode::new([0.0, 0.0, 0.0]),
            PathNode::new([5.0, 0.0, 0.0]),
            PathNode::new([5.0, 5.0, 0.0]),
        ];
        let mut round = sharp.clone();
        round[1].radius = Some(2.0);
        // 10 − 2·2 + π·2/2 = 6 + π.
        let exact = 6.0 + std::f64::consts::PI;
        assert!(close(path_length(&sharp), 10.0, 1e-12));
        assert!(close(path_length(&round), exact, exact * 2e-4));
        let f = fillet(&round, 1).unwrap();
        assert!(close(f.radius, 2.0, 1e-12));
        assert!(close(f.start[0], 3.0, 1e-12) && close(f.end[1], 2.0, 1e-12));
    }

    #[test]
    fn a_radius_that_does_not_fit_shrinks() {
        let n = [
            PathNode::new([0.0, 0.0, 0.0]),
            PathNode::curved([2.0, 0.0, 0.0], 10.0),
            PathNode::new([2.0, 2.0, 0.0]),
        ];
        let f = fillet(&n, 1).unwrap();
        assert!(close(f.radius, 1.0, 1e-12));
    }

    #[test]
    fn the_arc_climbs_between_its_tangent_points() {
        let n = [
            PathNode::new([0.0, 0.0, 0.0]),
            PathNode::curved([4.0, 0.0, 1.0], 1.0),
            PathNode::new([4.0, 4.0, 2.0]),
        ];
        let pts = sample(&n);
        assert!(pts.windows(2).all(|w| w[1][2] >= w[0][2] - 1e-12));
        assert_eq!(pts.last().unwrap()[2], 2.0);
    }

    #[test]
    fn point_at_walks_along_the_line() {
        let pts = [[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 4.0, 0.0]];
        let (p, d) = point_at(&pts, 5.0).unwrap();
        assert_eq!(p, [3.0, 2.0, 0.0]);
        assert_eq!(d, [0.0, 1.0, 0.0]);
        assert_eq!(point_at(&pts, 99.0).unwrap().0, [3.0, 4.0, 0.0]);
    }

    #[test]
    fn constant_slope_levels_the_middle_nodes() {
        let mut n = vec![
            PathNode::new([0.0, 0.0, 0.0]),
            PathNode::new([1.0, 0.0, 5.0]),
            PathNode::new([3.0, 0.0, 3.0]),
        ];
        constant_slope(&mut n, 0, 2);
        assert!(close(n[1].at[2], 1.0, 1e-12));
    }

    #[test]
    fn band_wraps_the_line() {
        let b = band(&[[0.0, 0.0], [4.0, 0.0]], 1.0);
        assert_eq!(b.len(), 4);
        assert!(close(crate::aisles::polygon_area(&b), 4.0, 1e-9));
    }
}
