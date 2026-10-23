//! Pure rate solver for the fluid network.
//!
//! Given the instantaneous "role" of every node (a tank is empty, stocked or full; a
//! converter has its rate limit...), [`solve`] computes one flow rate per link such
//! that:
//!
//! * a source offers at most its rate;
//! * a tank holding fluid offers at most `max_out`; an empty tank only passes through
//!   what it receives; a full tank accepts at most what it sends out (and `max_in`);
//! * pipes, sinks and converters cap their throughput;
//! * the outputs of a node share its flow equally, and what one output refuses is
//!   redistributed among the others (water-filling);
//! * a node that does not store (source, pipe, sink, converter) conserves flow.
//!
//! # Algorithm
//!
//! Every link carries a capacity `cap` (initially infinite). One round has two sweeps
//! over the acyclic graph (nodes in a deterministic topological order, ties broken by
//! node index):
//!
//! 1. **Backward** (reverse order): `accept[i]`, the largest total inflow node `i` can
//!    take, from what its out-links can carry: `W = sum(min(cap, accept[head]))`.
//!    A sink or converter accepts its `max_rate`; a pipe `min(max_rate, W)`; a tank
//!    that is not full `max_in`; a full tank `min(max_in, max_out, W)`.
//! 2. **Forward**: each node first shares its `accept` among its in-links by
//!    water-filling over what the parents offered, then computes its outflow `Q`
//!    (source `min(rate, W)`, pipe `min(max_rate, inflow)`, stocked tank
//!    `min(max_out, W)`, empty tank `min(max_out, inflow, W)`) and splits it over its
//!    out-links by water-filling with capacities `min(cap, accept[head])`.
//!
//! The only way a parent's offer can be refused is contention: several parents feeding
//! a node whose `accept` is smaller than the sum of their offers. When that happens the
//! refused link gets `cap := accepted` and another round runs, which moves the refused
//! share to the sibling outputs and throttles the parent's own inflow. Link capacities
//! never increase and every change lowers one by more than a relative `1e-12`, so the
//! relaxation reaches a fixed point after finitely many rounds (in practice one or two
//! per level of contention); a round limit turns any pathological non-convergence into
//! an error. At the fixed point offers equal acceptances on every link, so the flows
//! satisfy all constraints and conservation. When there is no contention the first
//! round already is the fixed point. The result is feasible and deterministic; with
//! contention it is a greedy allocation, not necessarily the global maximum flow.

/// What a node does right now, as far as the rates are concerned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    Source {
        rate: f64,
    },
    Tank {
        empty: bool,
        full: bool,
        max_in: f64,
        max_out: f64,
    },
    Pipe {
        max_rate: f64,
    },
    /// Terminal absorber (a sink or a `ToItems` converter).
    Drain {
        max_rate: f64,
    },
    /// No continuous flow at all (`FromItems`: only impulses).
    Idle,
}

/// Links and a deterministic topological order.
#[derive(Clone, Debug)]
pub struct Graph {
    pub n: usize,
    pub links: Vec<(usize, usize)>,
    pub order: Vec<usize>,
    pub out: Vec<Vec<usize>>,
    pub inn: Vec<Vec<usize>>,
}

impl Graph {
    /// Builds the graph; `Err` when the links contain a cycle.
    pub fn new(n: usize, links: Vec<(usize, usize)>) -> Result<Graph, String> {
        let mut out = vec![Vec::new(); n];
        let mut inn = vec![Vec::new(); n];
        let mut indeg = vec![0usize; n];
        for (l, &(a, b)) in links.iter().enumerate() {
            out[a].push(l);
            inn[b].push(l);
            indeg[b] += 1;
        }
        let mut order = Vec::with_capacity(n);
        let mut done = vec![false; n];
        while order.len() < n {
            // Smallest index among the ready nodes: deterministic.
            let Some(i) = (0..n).find(|&i| !done[i] && indeg[i] == 0) else {
                return Err("as ligações formam um ciclo".into());
            };
            done[i] = true;
            order.push(i);
            for &l in &out[i] {
                indeg[links[l].1] -= 1;
            }
        }
        Ok(Graph {
            n,
            links,
            order,
            out,
            inn,
        })
    }
}

/// Allocates `total` over `caps` equally, giving back what capped entries cannot take.
pub fn water_fill(caps: &[f64], total: f64) -> Vec<f64> {
    let sum: f64 = caps.iter().sum();
    if total >= sum {
        return caps.to_vec();
    }
    let mut idx: Vec<usize> = (0..caps.len()).collect();
    idx.sort_by(|&a, &b| caps[a].total_cmp(&caps[b]).then(a.cmp(&b)));
    let mut alloc = vec![0.0; caps.len()];
    let mut remaining = total;
    let mut left = caps.len();
    for &i in &idx {
        let share = remaining / left as f64;
        let give = caps[i].min(share);
        alloc[i] = give;
        remaining -= give;
        left -= 1;
    }
    alloc
}

fn max_rounds(g: &Graph) -> usize {
    200 + 50 * g.links.len()
}

/// Solves the rate of every link (indexed like `graph.links`).
pub fn solve(graph: &Graph, roles: &[Role]) -> Result<Vec<f64>, SolveError> {
    let nl = graph.links.len();
    let mut cap = vec![f64::INFINITY; nl];
    let mut accept = vec![0.0; graph.n];
    for _round in 0..max_rounds(graph) {
        // Backward sweep.
        let mut wsum = vec![0.0; graph.n];
        for &i in graph.order.iter().rev() {
            let w: f64 = graph.out[i]
                .iter()
                .map(|&l| cap[l].min(accept[graph.links[l].1]))
                .sum();
            wsum[i] = w;
            accept[i] = match roles[i] {
                Role::Source { .. } | Role::Idle => 0.0,
                Role::Drain { max_rate } => max_rate,
                Role::Pipe { max_rate } => max_rate.min(w),
                Role::Tank {
                    full,
                    max_in,
                    max_out,
                    ..
                } => {
                    if full {
                        max_in.min(max_out).min(w)
                    } else {
                        max_in
                    }
                }
            };
        }
        // Forward sweep.
        let mut offer = vec![0.0; nl];
        let mut accepted = vec![0.0; nl];
        for &i in &graph.order {
            let caps: Vec<f64> = graph.inn[i].iter().map(|&l| offer[l]).collect();
            let alloc = water_fill(&caps, accept[i]);
            for (k, &l) in graph.inn[i].iter().enumerate() {
                accepted[l] = alloc[k];
            }
            let inflow: f64 = alloc.iter().sum();
            let w = wsum[i];
            let q = match roles[i] {
                Role::Source { rate } => rate.min(w),
                Role::Pipe { max_rate } => max_rate.min(inflow),
                Role::Tank { empty, max_out, .. } => {
                    if empty {
                        max_out.min(inflow).min(w)
                    } else {
                        max_out.min(w)
                    }
                }
                Role::Drain { .. } | Role::Idle => 0.0,
            };
            let out_caps: Vec<f64> = graph.out[i]
                .iter()
                .map(|&l| cap[l].min(accept[graph.links[l].1]))
                .collect();
            let alloc = water_fill(&out_caps, q);
            for (k, &l) in graph.out[i].iter().enumerate() {
                offer[l] = alloc[k];
            }
        }
        let mut changed = false;
        for l in 0..nl {
            if accepted[l] < offer[l] - 1e-12 * offer[l].max(1.0) {
                cap[l] = accepted[l];
                changed = true;
            }
        }
        if !changed {
            if let Some(l) = accepted.iter().position(|r| !r.is_finite()) {
                return Err(SolveError::Unbounded(l));
            }
            return Ok(accepted);
        }
    }
    Err(SolveError::NoConvergence)
}

/// Why the rates could not be computed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SolveError {
    /// The flow on this link (index) is unbounded: no limit anywhere along it.
    Unbounded(usize),
    /// The relaxation hit its round limit.
    NoConvergence,
}

#[cfg(test)]
mod tests {
    use super::*;

    const INF: f64 = f64::INFINITY;

    fn tank(empty: bool, full: bool, max_in: f64, max_out: f64) -> Role {
        Role::Tank {
            empty,
            full,
            max_in,
            max_out,
        }
    }

    #[test]
    fn water_fill_redistributes() {
        assert_eq!(water_fill(&[0.5, INF], 2.0), vec![0.5, 1.5]);
        assert_eq!(water_fill(&[INF, INF], 2.0), vec![1.0, 1.0]);
        assert_eq!(water_fill(&[1.0, 1.0], 5.0), vec![1.0, 1.0]);
        assert_eq!(water_fill(&[0.2, 0.3, INF], 3.0), vec![0.2, 0.3, 2.5]);
    }

    #[test]
    fn source_pipe_sink_chain() {
        // 0 source(2) -> 1 pipe(1.5) -> 2 sink(unlimited)
        let g = Graph::new(3, vec![(0, 1), (1, 2)]).unwrap();
        let r = solve(
            &g,
            &[
                Role::Source { rate: 2.0 },
                Role::Pipe { max_rate: 1.5 },
                Role::Drain { max_rate: INF },
            ],
        )
        .unwrap();
        assert_eq!(r, vec![1.5, 1.5]);
    }

    #[test]
    fn parallel_sinks_share_and_redistribute() {
        let g = Graph::new(3, vec![(0, 1), (0, 2)]).unwrap();
        let roles = |cap: f64| {
            vec![
                Role::Source { rate: 2.0 },
                Role::Drain { max_rate: cap },
                Role::Drain { max_rate: INF },
            ]
        };
        assert_eq!(solve(&g, &roles(INF)).unwrap(), vec![1.0, 1.0]);
        assert_eq!(solve(&g, &roles(0.5)).unwrap(), vec![0.5, 1.5]);
    }

    #[test]
    fn contention_throttles_parents() {
        // two sources (2 each) into a sink of 3
        let g = Graph::new(3, vec![(0, 2), (1, 2)]).unwrap();
        let r = solve(
            &g,
            &[
                Role::Source { rate: 2.0 },
                Role::Source { rate: 2.0 },
                Role::Drain { max_rate: 3.0 },
            ],
        )
        .unwrap();
        assert_eq!(r, vec![1.5, 1.5]);
    }

    #[test]
    fn contention_moves_refused_share_to_sibling() {
        // source A (4) feeds sink X (shared) and sink Y; source B (2) feeds X.
        // X accepts 2, Y accepts 10. A splits 2/2; X is fed 2+2 > 2 -> shares 1/1;
        // A's refused 1 goes to Y: A -> X 1, A -> Y 3.
        let g = Graph::new(4, vec![(0, 2), (0, 3), (1, 2)]).unwrap();
        let r = solve(
            &g,
            &[
                Role::Source { rate: 4.0 },
                Role::Source { rate: 2.0 },
                Role::Drain { max_rate: 2.0 },
                Role::Drain { max_rate: 10.0 },
            ],
        )
        .unwrap();
        assert_eq!(r, vec![1.0, 3.0, 1.0]);
    }

    #[test]
    fn tank_modes() {
        // source(2) -> tank -> pipe(1) -> sink
        let g = Graph::new(4, vec![(0, 1), (1, 2), (2, 3)]).unwrap();
        let run = |t: Role| {
            solve(
                &g,
                &[
                    Role::Source { rate: 2.0 },
                    t,
                    Role::Pipe { max_rate: 1.0 },
                    Role::Drain { max_rate: INF },
                ],
            )
            .unwrap()
        };
        // stocked: fills at 2, drains at 1
        assert_eq!(run(tank(false, false, INF, INF)), vec![2.0, 1.0, 1.0]);
        // full: inflow throttled to the outflow
        assert_eq!(run(tank(false, true, INF, INF)), vec![1.0, 1.0, 1.0]);
        // empty: still accepts everything, passes through up to the pipe
        assert_eq!(run(tank(true, false, INF, INF)), vec![2.0, 1.0, 1.0]);
        // max_in limits the inflow; the stocked tank still drains at the pipe's 1
        assert_eq!(run(tank(false, false, 0.5, INF)), vec![0.5, 1.0, 1.0]);
    }

    #[test]
    fn unlimited_flow_is_an_error() {
        let g = Graph::new(2, vec![(0, 1)]).unwrap();
        let e = solve(
            &g,
            &[tank(false, false, INF, INF), Role::Drain { max_rate: INF }],
        )
        .unwrap_err();
        assert_eq!(e, SolveError::Unbounded(0));
    }

    #[test]
    fn order_is_topological_and_deterministic() {
        let g = Graph::new(4, vec![(3, 1), (1, 0), (2, 0)]).unwrap();
        assert_eq!(g.order, vec![2, 3, 1, 0]);
        assert!(Graph::new(2, vec![(0, 1), (1, 0)]).is_err());
    }
}
