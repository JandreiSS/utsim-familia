//! Roteadores da simulação: modo livre (menor caminho no piso) e modo rede (rede de caminhos
//! com ligação livre até o ponto de acesso). Ambos implementam [`utsim_core::Router`].

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use utsim_core::Router;
use utsim_objects::network::NetworkRouter;

use crate::geom::Pt;
use crate::grid::NavGrid;

type Key = [u64; 6];

fn key(from: [f64; 3], to: [f64; 3]) -> Key {
    [
        from[0].to_bits(),
        from[1].to_bits(),
        from[2].to_bits(),
        to[0].to_bits(),
        to[1].to_bits(),
        to[2].to_bits(),
    ]
}

/// Menor caminho livre no piso, com os destinos levados aos pontos de acesso.
///
/// Um destino igual (bit a bit) à posição de um objeto conhecido vira o ponto de acesso
/// dele: o recurso para ao lado da máquina, na frente do porta-paletes, e não no centro. A
/// origem nunca é trocada (é onde o recurso está). Sem caminho (área fechada), a viagem é
/// em linha reta, e `Model::warnings` avisa antes.
///
/// **Cache.** Cada par origem-destino é calculado uma vez e guardado num `BTreeMap` cuja
/// chave são os bits exatos das coordenadas; o caminho é função pura dos dados, então o
/// cache (compartilhado entre replicações em paralelo) não muda nenhum resultado.
pub struct FreeRouter {
    grid: Arc<NavGrid>,
    /// Posição do objeto → ponto de acesso, na ordem dos objetos (o primeiro vence).
    targets: Vec<([f64; 3], Pt)>,
    cache: Mutex<BTreeMap<Key, Arc<Vec<[f64; 3]>>>>,
}

impl FreeRouter {
    pub fn new(grid: Arc<NavGrid>, targets: Vec<([f64; 3], Pt)>) -> Self {
        FreeRouter {
            grid,
            targets,
            cache: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn grid(&self) -> &NavGrid {
        &self.grid
    }

    /// Onde o recurso para quando vai a `to`: o ponto de acesso, se `to` é um objeto.
    pub fn target(&self, to: [f64; 3]) -> [f64; 3] {
        self.targets
            .iter()
            .find(|(p, _)| key(*p, *p) == key(to, to))
            .map_or(to, |(_, a)| [a[0], a[1], to[2]])
    }

    /// Caminho livre de `from` a `to`, sem trocar o destino pelo ponto de acesso.
    pub fn free_route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]> {
        if from == to {
            return vec![from];
        }
        let k = key(from, to);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = cache.get(&k) {
            return p.as_ref().clone();
        }
        let path = match self.grid.find_path([from[0], from[1]], [to[0], to[1]]) {
            Some(p) => {
                let last = p.points.len() - 1;
                p.points
                    .iter()
                    .enumerate()
                    .map(|(i, q)| {
                        let z = if i == 0 {
                            from[2]
                        } else if i == last {
                            to[2]
                        } else {
                            from[2]
                        };
                        [q[0], q[1], z]
                    })
                    .collect()
            }
            None => vec![from, to],
        };
        cache.insert(k, Arc::new(path.clone()));
        path
    }

    /// Pares guardados no cache (para os testes).
    pub fn cached(&self) -> usize {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

impl Router for FreeRouter {
    fn route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]> {
        self.free_route(from, self.target(to))
    }
}

/// Modo rede: o destino vira o ponto de acesso; da origem ao nó mais próximo dela e do nó
/// mais próximo do ponto de acesso até ele, o caminho é o livre; entre os nós, o menor
/// caminho da rede (como `NetworkRouter`). Sem ligação pela rede, vale o caminho livre.
pub struct NetworkModeRouter {
    network: Arc<NetworkRouter>,
    free: Arc<FreeRouter>,
}

impl NetworkModeRouter {
    pub fn new(network: Arc<NetworkRouter>, free: Arc<FreeRouter>) -> Self {
        NetworkModeRouter { network, free }
    }
}

fn append(path: &mut Vec<[f64; 3]>, piece: Vec<[f64; 3]>) {
    for p in piece {
        if path.last() != Some(&p) {
            path.push(p);
        }
    }
}

impl Router for NetworkModeRouter {
    fn route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]> {
        let target = self.free.target(to);
        if from == target {
            return vec![from];
        }
        let Some(nr) = self.network.node_route(from, target) else {
            return self.free.free_route(from, target);
        };
        let first = self.network.node_position(nr.nodes[0]);
        let last = self
            .network
            .node_position(*nr.nodes.last().expect("one node"));
        let mut path = vec![from];
        append(&mut path, self.free.free_route(from, first));
        for &n in &nr.nodes[1..] {
            append(&mut path, vec![self.network.node_position(n)]);
        }
        append(&mut path, self.free.free_route(last, target));
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::Obstacle;
    use crate::geom::rectangle;
    use crate::grid::GridOptions;
    use utsim_core::task::path_length;
    use utsim_objects::network::{EdgeDef, NetworkDef, NodeDef};

    fn router() -> FreeRouter {
        let machine = Obstacle {
            name: "m".into(),
            polygon: rectangle([5.0, 0.0], [0.9, 0.9], 0.0),
        };
        let grid = NavGrid::new(
            &[machine],
            0.5,
            &[[0.0, 0.0], [10.0, 0.0]],
            &GridOptions::default(),
        );
        FreeRouter::new(Arc::new(grid), vec![([5.0, 0.0, 0.0], [6.0, -1.45])])
    }

    #[test]
    fn going_to_an_object_stops_at_its_access_point() {
        let r = router();
        let p = r.route([0.0, -1.45, 0.0], [5.0, 0.0, 0.0]);
        assert_eq!(p, vec![[0.0, -1.45, 0.0], [6.0, -1.45, 0.0]]);
        assert_eq!(path_length(&p), 6.0);
        // Points that are not objects are kept.
        assert_eq!(r.target([1.0, 2.0, 0.0]), [1.0, 2.0, 0.0]);
    }

    #[test]
    fn cache_gives_the_same_path() {
        let r = router();
        let a = r.route([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]);
        assert_eq!(r.cached(), 1);
        let b = r.route([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]);
        assert_eq!(a, b);
        assert_eq!(r.cached(), 1);
        // A fresh router (no cache) gives the very same path.
        assert_eq!(router().route([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]), a);
        assert!(path_length(&a) > 10.0);
    }

    #[test]
    fn network_mode_reaches_the_access_point_through_free_connectors() {
        let net = NetworkDef {
            nodes: vec![
                NodeDef {
                    name: "a".into(),
                    position: [0.0, -3.0, 0.0],
                },
                NodeDef {
                    name: "b".into(),
                    position: [10.0, -3.0, 0.0],
                },
            ],
            edges: vec![EdgeDef {
                from: "a".into(),
                to: "b".into(),
                two_way: true,
                capacity: None,
            }],
            generated: None,
        };
        let free = Arc::new(router());
        let r = NetworkModeRouter::new(Arc::new(NetworkRouter::new(&net)), free);
        let p = r.route([0.0, 0.0, 0.0], [5.0, 0.0, 0.0]);
        // Start → node a (3 m), a → b (10 m), b → access point of m (√(16 + 1.55²)).
        let expected = 3.0 + 10.0 + (16.0f64 + 1.55 * 1.55).sqrt();
        assert!((path_length(&p) - expected).abs() < 1e-9, "{p:?}");
        assert_eq!(*p.last().unwrap(), [6.0, -1.45, 0.0]);
    }
}
