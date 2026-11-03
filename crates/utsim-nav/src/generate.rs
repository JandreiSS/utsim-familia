//! Geração de uma rede de caminhos (`NetworkDef`) a partir do caminho livre, para gravar no
//! modelo: reprodutível sem a planta, revisável e editável à mão.

use std::collections::{BTreeMap, BTreeSet};

use utsim_objects::network::{EdgeDef, GeneratedNet, NetworkDef, NodeDef};

use crate::geom::Pt;
use crate::grid::NavGrid;

/// Rede gerada e os pontos que ficaram de fora.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub network: NetworkDef,
    /// Nomes dos pontos sem caminho até o grupo principal (ficam fora da rede).
    pub unreachable: Vec<String>,
}

fn quantize(p: Pt) -> (i64, i64) {
    (
        (p[0] * 1000.0).round() as i64,
        (p[1] * 1000.0).round() as i64,
    )
}

/// Rede que liga os `points` (nome, posição) pelos caminhos livres entre cada par.
///
/// Cada caminho livre suavizado entra com os seus vértices como nós (vértices a menos de
/// 1 mm viram o mesmo nó) e os seus trechos como arestas de mão dupla, sem repetição. Por
/// construção, o menor caminho na rede entre dois pontos é o caminho livre entre eles, e
/// nenhum trecho chega a menos da folga de um obstáculo. Os pontos se agrupam pela
/// alcançabilidade; fica o maior grupo (empate: o do primeiro ponto) e os outros vão para
/// `unreachable`. Nós `g1`, `g2`... na ordem em que aparecem; tudo determinístico.
pub fn generate_network(grid: &NavGrid, points: &[(String, Pt)]) -> Generated {
    // Repeated positions are one point.
    let mut unique: Vec<(String, Pt)> = Vec::new();
    for (name, p) in points {
        if !unique.iter().any(|(_, q)| quantize(*q) == quantize(*p)) {
            unique.push((name.clone(), *p));
        }
    }
    // Groups by reachability.
    let mut group = vec![usize::MAX; unique.len()];
    let mut sizes: Vec<usize> = Vec::new();
    for i in 0..unique.len() {
        if group[i] != usize::MAX {
            continue;
        }
        let g = sizes.len();
        group[i] = g;
        let mut size = 1;
        for j in i + 1..unique.len() {
            if group[j] == usize::MAX && grid.connected(unique[i].1, unique[j].1) {
                group[j] = g;
                size += 1;
            }
        }
        sizes.push(size);
    }
    let main = (0..sizes.len())
        .max_by(|a, b| sizes[*a].cmp(&sizes[*b]).then(b.cmp(a)))
        .unwrap_or(0);
    let unreachable = unique
        .iter()
        .zip(&group)
        .filter(|(_, g)| **g != main)
        .map(|((n, _), _)| n.clone())
        .collect();
    let kept: Vec<Pt> = unique
        .iter()
        .zip(&group)
        .filter(|(_, g)| **g == main)
        .map(|((_, p), _)| *p)
        .collect();

    let mut index: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    let mut nodes: Vec<NodeDef> = Vec::new();
    let mut node_of = |p: Pt, nodes: &mut Vec<NodeDef>| -> usize {
        *index.entry(quantize(p)).or_insert_with(|| {
            nodes.push(NodeDef {
                name: format!("g{}", nodes.len() + 1),
                position: [p[0], p[1], 0.0],
            });
            nodes.len() - 1
        })
    };
    let mut seen: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut edges: Vec<EdgeDef> = Vec::new();
    for p in &kept {
        node_of(*p, &mut nodes);
    }
    for i in 0..kept.len() {
        for j in i + 1..kept.len() {
            let Some(path) = grid.find_path(kept[i], kept[j]) else {
                continue;
            };
            for w in path.points.windows(2) {
                let (a, b) = (node_of(w[0], &mut nodes), node_of(w[1], &mut nodes));
                if a == b || !seen.insert((a.min(b), a.max(b))) {
                    continue;
                }
                edges.push(EdgeDef {
                    from: nodes[a].name.clone(),
                    to: nodes[b].name.clone(),
                    two_way: true,
                    capacity: None,
                });
            }
        }
    }
    Generated {
        network: NetworkDef {
            nodes,
            edges,
            generated: Some(GeneratedNet {
                fingerprint: grid.fingerprint(),
                clearance: grid.clearance(),
            }),
        },
        unreachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::Obstacle;
    use crate::geom::{dist, rectangle};
    use crate::grid::GridOptions;
    use utsim_core::Router;
    use utsim_core::task::path_length;
    use utsim_objects::network::NetworkRouter;

    /// Small deterministic generator (no external crates): xorshift.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn random_plant(seed: u64) -> (Vec<Obstacle>, Vec<(String, Pt)>) {
        let mut rng = Rng(seed);
        let mut obs = Vec::new();
        for k in 0..8 {
            let c = [rng.next() * 30.0, rng.next() * 20.0];
            let h = [0.5 + rng.next() * 2.0, 0.5 + rng.next() * 1.5];
            obs.push(Obstacle {
                name: format!("o{k}"),
                polygon: rectangle(c, h, rng.next() * 3.0),
            });
        }
        let mut pts = Vec::new();
        while pts.len() < 6 {
            let p = [rng.next() * 30.0, rng.next() * 20.0];
            let grid = NavGrid::new(&obs, 0.5, &[p], &GridOptions::default());
            if grid.is_free(p) {
                pts.push((format!("p{}", pts.len()), p));
            }
        }
        (obs, pts)
    }

    #[test]
    fn generated_network_never_crosses_an_obstacle() {
        for seed in [1u64, 7, 42, 2026] {
            let (obs, pts) = random_plant(seed);
            let all: Vec<Pt> = pts.iter().map(|p| p.1).collect();
            let grid = NavGrid::new(&obs, 0.5, &all, &GridOptions::default());
            let g = generate_network(&grid, &pts);
            let pos: BTreeMap<&str, [f64; 3]> = g
                .network
                .nodes
                .iter()
                .map(|n| (n.name.as_str(), n.position))
                .collect();
            for e in &g.network.edges {
                let (a, b) = (pos[e.from.as_str()], pos[e.to.as_str()]);
                assert!(
                    grid.segment_free([a[0], a[1]], [b[0], b[1]]),
                    "semente {seed}: {} -> {} cruza um obstáculo",
                    e.from,
                    e.to
                );
            }
            if g.unreachable.is_empty() {
                assert!(g.network.validate().is_empty(), "semente {seed}");
                // Shortest path in the network = free path between the points.
                let router = NetworkRouter::new(&g.network);
                let (a, b) = (pts[0].1, pts[1].1);
                let free = grid.find_path(a, b).unwrap().length;
                let net = path_length(&router.route([a[0], a[1], 0.0], [b[0], b[1], 0.0]));
                assert!(net <= free + 1e-6, "semente {seed}: {net} > {free}");
            }
            // Same input, same network.
            assert_eq!(g, generate_network(&grid, &pts));
        }
    }

    #[test]
    fn points_cut_off_are_reported() {
        let walls: Vec<Obstacle> = [
            [[-3.0, -3.0], [3.0, -3.0], [3.0, -2.8], [-3.0, -2.8]],
            [[-3.0, 2.8], [3.0, 2.8], [3.0, 3.0], [-3.0, 3.0]],
            [[-3.0, -3.0], [-2.8, -3.0], [-2.8, 3.0], [-3.0, 3.0]],
            [[2.8, -3.0], [3.0, -3.0], [3.0, 3.0], [2.8, 3.0]],
        ]
        .iter()
        .enumerate()
        .map(|(i, p)| Obstacle {
            name: format!("w{i}"),
            polygon: p.to_vec(),
        })
        .collect();
        let pts = vec![
            ("dentro".to_string(), [0.0, 0.0]),
            ("fora_1".to_string(), [8.0, 0.0]),
            ("fora_2".to_string(), [8.0, 5.0]),
        ];
        let all: Vec<Pt> = pts.iter().map(|p| p.1).collect();
        let grid = NavGrid::new(&walls, 0.3, &all, &GridOptions::default());
        let g = generate_network(&grid, &pts);
        assert_eq!(g.unreachable, vec!["dentro".to_string()]);
        assert_eq!(g.network.nodes.len(), 2);
        assert_eq!(g.network.edges.len(), 1);
        assert!((dist([8.0, 0.0], [8.0, 5.0]) - 5.0).abs() < 1e-12);
        let info = g.network.generated.unwrap();
        assert_eq!(info.fingerprint, grid.fingerprint());
        assert_eq!(info.clearance, 0.3);
    }
}
