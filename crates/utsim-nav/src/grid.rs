//! Grade de ocupação com folga, A* de 8 vizinhos e suavização por linha de visada.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::body::Obstacle;
use crate::geom::{
    EPS, Pt, bbox, dist, point_in_polygon, point_polygon, polyline_length, segment_polygon,
};

/// Parâmetros da grade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridOptions {
    /// Lado da célula (m). Padrão 0,25.
    pub cell: f64,
    /// Mais células permitidas; numa planta maior a célula cresce até caber.
    pub max_cells: usize,
    /// Faixa livre em volta de tudo (m), para contornar obstáculos da borda.
    pub margin: f64,
}

impl Default for GridOptions {
    fn default() -> Self {
        GridOptions {
            cell: 0.25,
            max_cells: 1_000_000,
            margin: 2.0,
        }
    }
}

struct Prepared {
    polygon: Vec<Pt>,
    lo: Pt,
    hi: Pt,
}

/// Caminho livre no piso.
#[derive(Clone, Debug, PartialEq)]
pub struct NavPath {
    /// Da origem ao destino, com as duas pontas.
    pub points: Vec<Pt>,
    pub length: f64,
}

/// Obstáculos inflados por uma folga, discretizados numa grade.
///
/// **Regras.** Um ponto é livre quando está a pelo menos `clearance` de todo obstáculo (fora
/// dele). A célula é livre quando o seu centro está a pelo menos `max(clearance, lado·√2/2)`:
/// assim um passo entre centros vizinhos nunca atravessa um obstáculo, mesmo fino (parede de
/// 0,15 m). O caminho é o A* de 8 vizinhos (diagonal só com os dois lados livres, custo em
/// metros, heurística octil), seguido de suavização: das pontas, cada vértice é ligado ao mais
/// distante que ele vê em linha reta (segmento a pelo menos `clearance` de todo obstáculo,
/// teste exato). O resultado é ao menos tão longo quanto o menor caminho exato e, em geral,
/// fica a menos de um lado de célula dele.
///
/// **Pontas fora do livre.** Uma origem ou destino dentro de um obstáculo (ou da folga) sai
/// pela célula livre mais próxima, em linha reta.
///
/// **Determinismo.** Tudo é função dos dados: empate na fila do A* pelo menor índice de
/// célula, vizinhos em ordem fixa, nada de iteração de `HashMap`. A ordem dos obstáculos
/// não muda o resultado.
pub struct NavGrid {
    obstacles: Vec<Prepared>,
    names: Vec<String>,
    clearance: f64,
    origin: Pt,
    cell: f64,
    nx: usize,
    ny: usize,
    blocked: Vec<bool>,
    /// Componente conexa de cada célula livre (`u32::MAX` = bloqueada).
    component: Vec<u32>,
}

const NEIGHBOURS: [(i64, i64); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, -1),
];

impl NavGrid {
    /// Grade que cobre os obstáculos e os `points` dados (posições e pontos de acesso), com
    /// a faixa `opts.margin` em volta.
    pub fn new(obstacles: &[Obstacle], clearance: f64, points: &[Pt], opts: &GridOptions) -> Self {
        let clearance = if clearance.is_finite() {
            clearance.max(0.0)
        } else {
            0.0
        };
        let prepared: Vec<Prepared> = obstacles
            .iter()
            .filter(|o| o.problem().is_none())
            .map(|o| {
                let b = bbox(&o.polygon).expect("polygon has points");
                Prepared {
                    polygon: o.polygon.clone(),
                    lo: b[0],
                    hi: b[1],
                }
            })
            .collect();
        let names = obstacles
            .iter()
            .filter(|o| o.problem().is_none())
            .map(|o| o.name.clone())
            .collect();
        let mut all: Vec<Pt> = points
            .iter()
            .copied()
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .collect();
        for o in &prepared {
            all.push(o.lo);
            all.push(o.hi);
        }
        let b = bbox(&all).unwrap_or([[0.0, 0.0], [0.0, 0.0]]);
        let base = if opts.cell.is_finite() && opts.cell > 0.0 {
            opts.cell
        } else {
            0.25
        };
        let pad = opts.margin.max(0.0) + clearance + base;
        let (w, h) = (b[1][0] - b[0][0] + 2.0 * pad, b[1][1] - b[0][1] + 2.0 * pad);
        let mut cell = base;
        let max_cells = opts.max_cells.max(16) as f64;
        let count = |c: f64| ((w / c).ceil() + 1.0) * ((h / c).ceil() + 1.0);
        if count(cell) > max_cells {
            // Larger cells, rounded up to a centimetre: same input, same cell.
            cell = ((w * h / max_cells).sqrt() * 100.0).ceil() / 100.0;
            while count(cell) > max_cells {
                cell += 0.01;
            }
        }
        let nx = (w / cell).ceil() as usize + 1;
        let ny = (h / cell).ceil() as usize + 1;
        let origin = [b[0][0] - pad, b[0][1] - pad];
        let mut grid = NavGrid {
            obstacles: prepared,
            names,
            clearance,
            origin,
            cell,
            nx,
            ny,
            blocked: vec![false; nx * ny],
            component: Vec::new(),
        };
        grid.stamp();
        grid.label_components();
        grid
    }

    /// Distância mínima do centro de uma célula livre aos obstáculos.
    fn threshold(&self) -> f64 {
        self.clearance
            .max(self.cell * std::f64::consts::FRAC_1_SQRT_2 + 1e-6)
    }

    fn stamp(&mut self) {
        let thr = self.threshold();
        for k in 0..self.obstacles.len() {
            let (lo, hi) = (self.obstacles[k].lo, self.obstacles[k].hi);
            let (i0, j0) = self.cell_of([lo[0] - thr, lo[1] - thr]);
            let (i1, j1) = self.cell_of([hi[0] + thr, hi[1] + thr]);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let idx = j * self.nx + i;
                    if self.blocked[idx] {
                        continue;
                    }
                    let c = self.center(idx);
                    let d = point_polygon(c, &self.obstacles[k].polygon);
                    if d < thr || d == 0.0 {
                        self.blocked[idx] = true;
                    }
                }
            }
        }
    }

    fn label_components(&mut self) {
        let n = self.nx * self.ny;
        let mut comp = vec![u32::MAX; n];
        let mut next = 0u32;
        let mut stack = Vec::new();
        for s in 0..n {
            if self.blocked[s] || comp[s] != u32::MAX {
                continue;
            }
            comp[s] = next;
            stack.push(s);
            while let Some(u) = stack.pop() {
                for v in self.neighbours(u) {
                    if comp[v.0] == u32::MAX {
                        comp[v.0] = next;
                        stack.push(v.0);
                    }
                }
            }
            next += 1;
        }
        self.component = comp;
    }

    pub fn clearance(&self) -> f64 {
        self.clearance
    }

    /// Lado da célula usado (pode ser maior que o pedido numa planta grande).
    pub fn cell(&self) -> f64 {
        self.cell
    }

    pub fn obstacle_count(&self) -> usize {
        self.obstacles.len()
    }

    fn cell_of(&self, p: Pt) -> (usize, usize) {
        let fx = ((p[0] - self.origin[0]) / self.cell).floor();
        let fy = ((p[1] - self.origin[1]) / self.cell).floor();
        let i = if fx.is_finite() {
            fx.clamp(0.0, (self.nx - 1) as f64) as usize
        } else {
            0
        };
        let j = if fy.is_finite() {
            fy.clamp(0.0, (self.ny - 1) as f64) as usize
        } else {
            0
        };
        (i, j)
    }

    fn center(&self, idx: usize) -> Pt {
        let (i, j) = (idx % self.nx, idx / self.nx);
        [
            self.origin[0] + (i as f64 + 0.5) * self.cell,
            self.origin[1] + (j as f64 + 0.5) * self.cell,
        ]
    }

    /// Vizinhos livres de `u` com o custo do passo (m), em ordem fixa.
    fn neighbours(&self, u: usize) -> impl Iterator<Item = (usize, f64)> + '_ {
        let (i, j) = ((u % self.nx) as i64, (u / self.nx) as i64);
        let free = move |x: i64, y: i64| {
            x >= 0
                && y >= 0
                && (x as usize) < self.nx
                && (y as usize) < self.ny
                && !self.blocked[y as usize * self.nx + x as usize]
        };
        NEIGHBOURS.iter().filter_map(move |&(dx, dy)| {
            let (x, y) = (i + dx, j + dy);
            if !free(x, y) {
                return None;
            }
            let diagonal = dx != 0 && dy != 0;
            if diagonal && !(free(i + dx, j) && free(i, j + dy)) {
                return None;
            }
            let cost = if diagonal {
                self.cell * std::f64::consts::SQRT_2
            } else {
                self.cell
            };
            Some((y as usize * self.nx + x as usize, cost))
        })
    }

    /// Distância de `p` ao obstáculo mais próximo (zero dentro de um).
    pub fn distance_to_obstacles(&self, p: Pt) -> f64 {
        self.obstacles
            .iter()
            .map(|o| point_polygon(p, &o.polygon))
            .fold(f64::INFINITY, f64::min)
    }

    /// O recurso cabe em `p`: fora de todo obstáculo e a pelo menos a folga dele.
    pub fn is_free(&self, p: Pt) -> bool {
        self.obstacles.iter().all(|o| {
            if p[0] < o.lo[0] - self.clearance - 1.0
                || p[0] > o.hi[0] + self.clearance + 1.0
                || p[1] < o.lo[1] - self.clearance - 1.0
                || p[1] > o.hi[1] + self.clearance + 1.0
            {
                return true;
            }
            let d = point_polygon(p, &o.polygon);
            d > 0.0 && d >= self.clearance - EPS
        })
    }

    /// O recurso passa em linha reta de `a` a `b` sem chegar a menos da folga de um obstáculo.
    pub fn segment_free(&self, a: Pt, b: Pt) -> bool {
        let m = self.clearance + 1.0;
        let (lo, hi) = (
            [a[0].min(b[0]) - m, a[1].min(b[1]) - m],
            [a[0].max(b[0]) + m, a[1].max(b[1]) + m],
        );
        self.obstacles.iter().all(|o| {
            if o.hi[0] < lo[0] || o.lo[0] > hi[0] || o.hi[1] < lo[1] || o.lo[1] > hi[1] {
                return true;
            }
            let d = segment_polygon(a, b, &o.polygon);
            d > 0.0 && d >= self.clearance - EPS
        })
    }

    /// Nomes dos obstáculos que o segmento `ab` cruza ou toca (sem folga), na ordem dada.
    pub fn crossed(&self, a: Pt, b: Pt) -> Vec<&str> {
        self.obstacles
            .iter()
            .zip(&self.names)
            .filter(|(o, _)| segment_polygon(a, b, &o.polygon) == 0.0)
            .map(|(_, n)| n.as_str())
            .collect()
    }

    /// Nomes dos obstáculos que contêm `p`.
    pub fn containing(&self, p: Pt) -> Vec<&str> {
        self.obstacles
            .iter()
            .zip(&self.names)
            .filter(|(o, _)| point_in_polygon(p, &o.polygon))
            .map(|(_, n)| n.as_str())
            .collect()
    }

    /// Célula livre mais próxima de `p` (a dele, se for livre); empate pelo menor índice.
    fn nearest_free(&self, p: Pt) -> Option<usize> {
        let (ci, cj) = self.cell_of(p);
        let start = cj * self.nx + ci;
        if !self.blocked[start] {
            return Some(start);
        }
        let mut best: Option<(f64, usize)> = None;
        let mut limit = self.nx.max(self.ny);
        let mut k = 1;
        while k <= limit {
            let (ci, cj, k_i) = (ci as i64, cj as i64, k as i64);
            for y in (cj - k_i)..=(cj + k_i) {
                for x in (ci - k_i)..=(ci + k_i) {
                    let on_ring = (x - ci).abs() == k_i || (y - cj).abs() == k_i;
                    if !on_ring || x < 0 || y < 0 || x as usize >= self.nx || y as usize >= self.ny
                    {
                        continue;
                    }
                    let idx = y as usize * self.nx + x as usize;
                    if self.blocked[idx] {
                        continue;
                    }
                    let d = dist(p, self.center(idx));
                    if best.is_none_or(|(bd, bi)| d < bd || (d == bd && idx < bi)) {
                        best = Some((d, idx));
                    }
                }
            }
            if best.is_some() && limit == self.nx.max(self.ny) {
                // A closer free cell can still sit on the next rings (corners are farther).
                limit = (k * 3).div_ceil(2) + 1;
            }
            k += 1;
        }
        best.map(|(_, i)| i)
    }

    /// As células de `a` e `b` estão ligadas (há caminho entre elas).
    pub fn connected(&self, a: Pt, b: Pt) -> bool {
        match (self.nearest_free(a), self.nearest_free(b)) {
            (Some(s), Some(g)) => self.component[s] == self.component[g],
            _ => false,
        }
    }

    fn astar(&self, s: usize, g: usize) -> Option<Vec<usize>> {
        if self.component[s] != self.component[g] {
            return None;
        }
        let goal = self.center(g);
        let h = |idx: usize| {
            let c = self.center(idx);
            let (dx, dy) = ((c[0] - goal[0]).abs(), (c[1] - goal[1]).abs());
            let (lo, hi) = (dx.min(dy), dx.max(dy));
            hi - lo + lo * std::f64::consts::SQRT_2
        };
        // g cost, parent, closed.
        let mut nodes: HashMap<usize, (f64, usize, bool)> = HashMap::new();
        let mut open = BinaryHeap::new();
        nodes.insert(s, (0.0, usize::MAX, false));
        open.push(Reverse((h(s).to_bits(), s)));
        while let Some(Reverse((_, u))) = open.pop() {
            let (gu, _, closed) = nodes[&u];
            if closed {
                continue;
            }
            nodes.get_mut(&u).expect("node").2 = true;
            if u == g {
                let mut cells = vec![g];
                let mut cur = g;
                while cur != s {
                    cur = nodes[&cur].1;
                    cells.push(cur);
                }
                cells.reverse();
                return Some(cells);
            }
            for (v, cost) in self.neighbours(u) {
                let gv = gu + cost;
                let better = nodes
                    .get(&v)
                    .is_none_or(|&(old, _, c)| !c && gv < old - 1e-12);
                if better {
                    nodes.insert(v, (gv, u, false));
                    open.push(Reverse(((gv + h(v)).to_bits(), v)));
                }
            }
        }
        None
    }

    /// Menor caminho livre de `a` a `b` (ver as regras do tipo). `None` quando não há.
    pub fn find_path(&self, a: Pt, b: Pt) -> Option<NavPath> {
        if a == b {
            return Some(NavPath {
                points: vec![a],
                length: 0.0,
            });
        }
        if self.is_free(a) && self.is_free(b) && self.segment_free(a, b) {
            return Some(NavPath {
                points: vec![a, b],
                length: dist(a, b),
            });
        }
        let (s, g) = (self.nearest_free(a)?, self.nearest_free(b)?);
        let cells = self.astar(s, g)?;
        let mut pts = vec![a];
        pts.extend(cells.iter().map(|&c| self.center(c)));
        pts.push(b);
        pts.dedup();
        let points = self.smooth(&pts);
        let length = polyline_length(&points);
        Some(NavPath { points, length })
    }

    /// Corta vértices enquanto o segmento continua livre (duas passadas).
    fn smooth(&self, pts: &[Pt]) -> Vec<Pt> {
        let n = pts.len();
        if n <= 2 {
            return pts.to_vec();
        }
        // Forward: from each anchor, walk while the next point is still visible.
        let mut first = vec![pts[0]];
        let mut i = 0;
        while i < n - 1 {
            let mut j = i + 1;
            while j + 1 < n && self.segment_free(pts[i], pts[j + 1]) {
                j += 1;
            }
            first.push(pts[j]);
            i = j;
        }
        // Then from each anchor straight to the farthest visible vertex.
        let m = first.len();
        let mut out = vec![first[0]];
        let mut i = 0;
        while i < m - 1 {
            let mut j = m - 1;
            while j > i + 1 && !self.segment_free(first[i], first[j]) {
                j -= 1;
            }
            out.push(first[j]);
            i = j;
        }
        out
    }

    /// Impressão digital dos obstáculos e da folga (`fnv1a64:...`): muda quando a planta,
    /// a área de um objeto ou a folga mudam.
    pub fn fingerprint(&self) -> String {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.clearance.to_bits().to_le_bytes());
        for (o, name) in self.obstacles.iter().zip(&self.names) {
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(0);
            for p in &o.polygon {
                bytes.extend_from_slice(&p[0].to_bits().to_le_bytes());
                bytes.extend_from_slice(&p[1].to_bits().to_le_bytes());
            }
            bytes.push(0xff);
        }
        utsim_cad::fnv1a_hash(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::rectangle;

    fn square(name: &str, center: Pt, half: f64) -> Obstacle {
        Obstacle {
            name: name.into(),
            polygon: rectangle(center, [half, half], 0.0),
        }
    }

    /// Exact shortest length from `(-d, 0)` to `(d, 0)` around a square of half side `s`
    /// centered at the origin, inflated by the radius `r` (rounded corners).
    fn around_square(d: f64, s: f64, r: f64) -> f64 {
        // Tangent from A to the circle at the corner (-s, s): the path goes A → tangent
        // point → arc → top side (length 2s) → arc → tangent → B (symmetric).
        let c = [-s, s];
        let a = [-d, 0.0];
        let l = dist(a, c);
        let tangent = (l * l - r * r).sqrt();
        // Angle at the circle centre between the direction to A and the vertical.
        let to_a = (a[1] - c[1]).atan2(a[0] - c[0]);
        let alpha = (r / l).acos();
        // Tangent point direction from the centre.
        let t_dir = (to_a - alpha).rem_euclid(2.0 * std::f64::consts::PI);
        let arc = (std::f64::consts::FRAC_PI_2 - t_dir).abs() * r;
        2.0 * (tangent + arc) + 2.0 * s
    }

    #[test]
    fn straight_line_when_nothing_is_in_the_way() {
        let g = NavGrid::new(
            &[],
            0.5,
            &[[0.0, 0.0], [10.0, 0.0]],
            &GridOptions::default(),
        );
        let p = g.find_path([0.0, 0.0], [3.0, 4.0]).unwrap();
        assert_eq!(p.points, vec![[0.0, 0.0], [3.0, 4.0]]);
        assert_eq!(p.length, 5.0);
    }

    #[test]
    fn path_around_a_square_has_the_exact_length_within_a_cell() {
        let obs = [square("maquina", [0.0, 0.0], 1.0)];
        let g = NavGrid::new(
            &obs,
            0.5,
            &[[-5.0, 0.0], [5.0, 0.0]],
            &GridOptions::default(),
        );
        let p = g.find_path([-5.0, 0.0], [5.0, 0.0]).unwrap();
        let exact = around_square(5.0, 1.0, 0.5);
        assert!(p.length >= exact - 1e-9, "{} < {exact}", p.length);
        assert!(p.length <= exact + g.cell(), "{} vs {exact}", p.length);
        for w in p.points.windows(2) {
            assert!(g.segment_free(w[0], w[1]), "{w:?}");
        }
        // Symmetric problem, fixed tie-break: goes around one fixed side every time.
        let again = g.find_path([-5.0, 0.0], [5.0, 0.0]).unwrap();
        assert_eq!(p, again);
    }

    #[test]
    fn narrow_gap_closes_for_a_wide_resource() {
        // Two walls leave a 1 m gap at x = 0 between y = -0.5 and 0.5; a wall all along
        // y = 0 elsewhere forces the crossing through the gap.
        let walls = [
            Obstacle {
                name: "esq".into(),
                polygon: vec![[-20.0, -0.1], [-0.5, -0.1], [-0.5, 0.1], [-20.0, 0.1]],
            },
            Obstacle {
                name: "dir".into(),
                polygon: vec![[0.5, -0.1], [20.0, -0.1], [20.0, 0.1], [0.5, 0.1]],
            },
        ];
        let pts = [[0.0, -3.0], [0.0, 3.0]];
        let opts = GridOptions {
            margin: 0.0,
            ..GridOptions::default()
        };
        // Person (clearance 0.3): fits through the 1 m gap.
        let person = NavGrid::new(&walls, 0.3, &pts, &opts);
        let p = person.find_path(pts[0], pts[1]).unwrap();
        assert!((p.length - 6.0).abs() < 1e-9, "{}", p.length);
        // Forklift (clearance 0.9): the gap is closed, it goes around a wall end.
        let forklift = NavGrid::new(&walls, 0.9, &pts, &opts);
        let p = forklift.find_path(pts[0], pts[1]).unwrap();
        assert!(p.length > 40.0, "{}", p.length);
        for w in p.points.windows(2) {
            assert!(forklift.segment_free(w[0], w[1]), "{w:?}");
        }
    }

    #[test]
    fn closed_room_has_no_way_out() {
        // A ring of four walls around the origin; the outside point is unreachable.
        let walls: Vec<Obstacle> = [
            [[-3.0, -3.0], [3.0, -3.0], [3.0, -2.8], [-3.0, -2.8]],
            [[-3.0, 2.8], [3.0, 2.8], [3.0, 3.0], [-3.0, 3.0]],
            [[-3.0, -3.0], [-2.8, -3.0], [-2.8, 3.0], [-3.0, 3.0]],
            [[2.8, -3.0], [3.0, -3.0], [3.0, 3.0], [2.8, 3.0]],
        ]
        .iter()
        .enumerate()
        .map(|(i, p)| Obstacle {
            name: format!("parede_{i}"),
            polygon: p.to_vec(),
        })
        .collect();
        let g = NavGrid::new(
            &walls,
            0.3,
            &[[0.0, 0.0], [8.0, 0.0]],
            &GridOptions::default(),
        );
        assert!(g.find_path([0.0, 0.0], [8.0, 0.0]).is_none());
        assert!(!g.connected([0.0, 0.0], [8.0, 0.0]));
        assert!(g.connected([0.0, 0.0], [1.0, 1.0]));
    }

    #[test]
    fn thin_wall_is_never_crossed() {
        let wall = [Obstacle {
            name: "parede".into(),
            polygon: vec![[-0.075, -5.0], [0.075, -5.0], [0.075, 5.0], [-0.075, 5.0]],
        }];
        let g = NavGrid::new(
            &wall,
            0.0,
            &[[-2.0, 0.0], [2.0, 0.0]],
            &GridOptions::default(),
        );
        let p = g.find_path([-2.0, 0.0], [2.0, 0.0]).unwrap();
        for w in p.points.windows(2) {
            assert!(g.crossed(w[0], w[1]).is_empty(), "{w:?}");
        }
        assert!(p.length > 10.0);
    }

    #[test]
    fn order_of_obstacles_does_not_matter() {
        let a = square("a", [0.0, 0.0], 1.0);
        let b = square("b", [3.0, 1.0], 0.8);
        let c = square("c", [-3.0, -1.5], 0.6);
        let pts = [[-6.0, 0.3], [6.0, -0.2]];
        let one = NavGrid::new(
            &[a.clone(), b.clone(), c.clone()],
            0.4,
            &pts,
            &GridOptions::default(),
        );
        let two = NavGrid::new(&[c, a, b], 0.4, &pts, &GridOptions::default());
        assert_eq!(one.find_path(pts[0], pts[1]), two.find_path(pts[0], pts[1]));
    }

    #[test]
    fn start_inside_an_obstacle_leaves_by_the_nearest_free_cell() {
        let obs = [square("m", [0.0, 0.0], 1.0)];
        let g = NavGrid::new(
            &obs,
            0.3,
            &[[0.0, 0.0], [5.0, 0.0]],
            &GridOptions::default(),
        );
        assert!(!g.is_free([0.0, 0.0]));
        let p = g.find_path([0.0, 0.0], [5.0, 0.0]).unwrap();
        assert_eq!(p.points[0], [0.0, 0.0]);
        assert!(g.is_free(p.points[1]) || p.points.len() == 2);
        assert!(p.length >= 5.0 - 1e-9);
    }

    #[test]
    fn big_plants_get_bigger_cells() {
        let opts = GridOptions {
            max_cells: 10_000,
            ..GridOptions::default()
        };
        let g = NavGrid::new(&[], 0.5, &[[0.0, 0.0], [200.0, 100.0]], &opts);
        assert!(g.cell() > 0.25);
        assert!(g.nx * g.ny <= 10_000);
    }
}
