//! Simplificação Douglas-Peucker.

use crate::geom::Pt;

fn point_seg_dist(p: Pt, a: Pt, b: Pt) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let l2 = dx * dx + dy * dy;
    if l2 == 0.0 {
        return ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt();
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
    let cx = a[0] + t * dx;
    let cy = a[1] + t * dy;
    ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt()
}

/// Simplifica uma polilinha (sem recursão, para não estourar a pilha em linhas longas).
/// Mantém sempre as extremidades; em polilinhas fechadas, mantém ao menos 3 pontos.
pub fn simplify_polyline(points: &[Pt], closed: bool, tol: f64) -> Vec<Pt> {
    if points.len() <= 2 || tol <= 0.0 {
        return points.to_vec();
    }
    let mut pts: Vec<Pt> = points.to_vec();
    if closed {
        // trata o anel como linha aberta que volta ao início
        pts.push(points[0]);
    }
    let n = pts.len();
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((i, j)) = stack.pop() {
        if j <= i + 1 {
            continue;
        }
        let mut worst = 0.0;
        let mut at = i;
        for (k, &p) in pts.iter().enumerate().take(j).skip(i + 1) {
            let d = point_seg_dist(p, pts[i], pts[j]);
            if d > worst {
                worst = d;
                at = k;
            }
        }
        if worst > tol {
            keep[at] = true;
            stack.push((i, at));
            stack.push((at, j));
        }
    }
    let mut out: Vec<Pt> = pts
        .iter()
        .zip(&keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect();
    if closed {
        out.pop(); // o ponto de fechamento repetido
        if out.len() < 3 {
            // anel degenerado: mantém o original
            return points.to_vec();
        }
    }
    out
}
