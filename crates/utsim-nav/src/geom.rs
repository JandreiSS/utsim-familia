//! Geometria 2D mínima no plano do piso (`x`, `y` do modelo, em metros).

/// Ponto `[x, y]`.
pub type Pt = [f64; 2];

/// Tolerância das comparações de distância (1 nm): absorve o arredondamento sem mudar
/// nenhuma decisão em metros.
pub const EPS: f64 = 1e-9;

pub fn dist(a: Pt, b: Pt) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Comprimento de uma polilinha.
pub fn polyline_length(points: &[Pt]) -> f64 {
    points.windows(2).map(|w| dist(w[0], w[1])).sum()
}

fn cross(o: Pt, a: Pt, b: Pt) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Distância do ponto `p` ao segmento `ab`.
pub fn point_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    if len2 <= 0.0 {
        return dist(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0);
    dist(p, [a[0] + t * dx, a[1] + t * dy])
}

fn on_segment(p: Pt, a: Pt, b: Pt) -> bool {
    p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

/// Os segmentos `ab` e `cd` se tocam ou se cruzam (pontas incluídas).
pub fn segments_intersect(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && on_segment(a, c, d))
        || (d2 == 0.0 && on_segment(b, c, d))
        || (d3 == 0.0 && on_segment(c, a, b))
        || (d4 == 0.0 && on_segment(d, a, b))
}

/// Distância entre os segmentos `ab` e `cd` (zero quando se tocam).
pub fn segment_segment(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    if segments_intersect(a, b, c, d) {
        return 0.0;
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

/// Ponto dentro do polígono simples (regra par-ímpar; a borda conta como fora, mas a
/// distância até ela é zero).
pub fn point_in_polygon(p: Pt, poly: &[Pt]) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

fn edges(poly: &[Pt]) -> impl Iterator<Item = (Pt, Pt)> + '_ {
    (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()]))
}

/// Distância do ponto ao polígono cheio (zero dentro).
pub fn point_polygon(p: Pt, poly: &[Pt]) -> f64 {
    if poly.len() >= 3 && point_in_polygon(p, poly) {
        return 0.0;
    }
    edges(poly)
        .map(|(a, b)| point_segment(p, a, b))
        .fold(f64::INFINITY, f64::min)
}

/// Distância do segmento ao polígono cheio (zero quando o toca ou entra nele).
pub fn segment_polygon(a: Pt, b: Pt, poly: &[Pt]) -> f64 {
    if poly.len() >= 3 && (point_in_polygon(a, poly) || point_in_polygon(b, poly)) {
        return 0.0;
    }
    edges(poly)
        .map(|(c, d)| segment_segment(a, b, c, d))
        .fold(f64::INFINITY, f64::min)
}

/// Caixa envolvente `[min, max]` de um conjunto de pontos.
pub fn bbox(points: &[Pt]) -> Option<[Pt; 2]> {
    let first = *points.first()?;
    let mut b = [first, first];
    for p in points {
        b[0][0] = b[0][0].min(p[0]);
        b[0][1] = b[0][1].min(p[1]);
        b[1][0] = b[1][0].max(p[0]);
        b[1][1] = b[1][1].max(p[1]);
    }
    Some(b)
}

/// Retângulo de meias medidas `half` centrado em `center`, girado de `yaw` radianos
/// (anti-horário visto de cima), em sentido anti-horário.
pub fn rectangle(center: Pt, half: Pt, yaw: f64) -> Vec<Pt> {
    let (s, c) = yaw.sin_cos();
    [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
        .iter()
        .map(|k| {
            let (x, y) = (k[0] * half[0], k[1] * half[1]);
            [center[0] + x * c - y * s, center[1] + x * s + y * c]
        })
        .collect()
}

/// Ponto `local` (coordenadas do objeto) levado ao piso: girado de `yaw` e somado a `center`.
pub fn to_world(center: Pt, local: Pt, yaw: f64) -> Pt {
    let (s, c) = yaw.sin_cos();
    [
        center[0] + local[0] * c - local[1] * s,
        center[1] + local[0] * s + local[1] * c,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQ: [Pt; 4] = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];

    #[test]
    fn distances_to_a_square() {
        assert_eq!(point_polygon([1.0, 1.0], &SQ), 0.0);
        assert!((point_polygon([3.0, 1.0], &SQ) - 1.0).abs() < 1e-12);
        assert!((point_polygon([3.0, 3.0], &SQ) - 2f64.sqrt()).abs() < 1e-12);
        // Segment passing over the top at height 2.5: half a metre away.
        assert!((segment_polygon([-1.0, 2.5], [3.0, 2.5], &SQ) - 0.5).abs() < 1e-12);
        // Crossing the square: zero.
        assert_eq!(segment_polygon([-1.0, 1.0], [3.0, 1.0], &SQ), 0.0);
        // Touching a corner: zero.
        assert_eq!(segment_polygon([-1.0, 3.0], [3.0, -1.0], &SQ), 0.0);
    }

    #[test]
    fn rectangles_rotate_counter_clockwise() {
        let r = rectangle([0.0, 0.0], [2.0, 1.0], std::f64::consts::FRAC_PI_2);
        // Long side now along y.
        let b = bbox(&r).unwrap();
        assert!((b[1][1] - 2.0).abs() < 1e-12 && (b[1][0] - 1.0).abs() < 1e-12);
        let p = to_world([1.0, 1.0], [1.0, 0.0], std::f64::consts::FRAC_PI_2);
        assert!((p[0] - 1.0).abs() < 1e-12 && (p[1] - 2.0).abs() < 1e-12);
    }
}
