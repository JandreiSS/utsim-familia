//! AGV traffic control (phase 7): edge occupancy, FIFO waiting queues and deadlock
//! detection, shared by all AGVs of one simulation.
//!
//! Blocks only talk through the kernel, so the shared state lives in an
//! `Arc<Mutex<Traffic>>` created by `Model::build` (one per replication: replications
//! stay independent and deterministic). Everything happens inside kernel hooks, which
//! run one at a time in a fixed order, so the mutex is never contended.
//!
//! **Rules.**
//! - Only edges with a `capacity` are controlled; the others are free-flowing.
//! - A `two_way` edge is *one* resource: its capacity is shared by both directions.
//!   (Two one-way edges between the same nodes are two independent resources.)
//! - A vehicle occupies one controlled edge at a time. It keeps its current edge
//!   until the next one is granted, then releases the old one in the same instant.
//! - Waiting queues are FIFO per edge. When an edge is released, the head of its
//!   queue is granted immediately (it becomes a holder before anybody else can ask),
//!   and is woken by a message.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use utsim_core::BlockId;

use crate::network::{NetworkDef, NetworkRouter};

struct EdgeState {
    from: usize,
    to: usize,
    capacity: usize,
    holders: Vec<BlockId>,
    queue: VecDeque<BlockId>,
}

/// One vehicle caught in a deadlock: what it waits for and who holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Waiter {
    pub vehicle: BlockId,
    pub edge: usize,
    pub holders: Vec<BlockId>,
}

pub struct Traffic {
    router: Arc<NetworkRouter>,
    node_names: Vec<String>,
    edges: Vec<EdgeState>,
    /// `(u, v)` -> controlled edge id; both directions for a two-way edge.
    lookup: HashMap<(usize, usize), usize>,
}

impl Traffic {
    pub fn new(def: &NetworkDef, router: Arc<NetworkRouter>) -> Self {
        let mut index = HashMap::new();
        for (i, n) in def.nodes.iter().enumerate() {
            index.entry(n.name.as_str()).or_insert(i);
        }
        let mut edges = Vec::new();
        let mut lookup = HashMap::new();
        for e in &def.edges {
            let (Some(&a), Some(&b), Some(cap)) = (
                index.get(e.from.as_str()),
                index.get(e.to.as_str()),
                e.capacity,
            ) else {
                continue;
            };
            let id = edges.len();
            edges.push(EdgeState {
                from: a,
                to: b,
                capacity: cap.max(1),
                holders: Vec::new(),
                queue: VecDeque::new(),
            });
            lookup.entry((a, b)).or_insert(id);
            if e.two_way {
                lookup.entry((b, a)).or_insert(id);
            }
        }
        Traffic {
            router,
            node_names: def.nodes.iter().map(|n| n.name.clone()).collect(),
            edges,
            lookup,
        }
    }

    pub fn router(&self) -> &NetworkRouter {
        &self.router
    }

    /// Controlled edge used to go from node `u` to node `v`, if there is one.
    pub fn edge_between(&self, u: usize, v: usize) -> Option<usize> {
        self.lookup.get(&(u, v)).copied()
    }

    /// "A->B" (or "A<->B" for a two-way edge), for messages.
    pub fn edge_label(&self, e: usize) -> String {
        let s = &self.edges[e];
        let two_way = self.lookup.get(&(s.to, s.from)) == Some(&e);
        format!(
            "{}{}{}",
            self.node_names[s.from],
            if two_way { "<->" } else { "->" },
            self.node_names[s.to]
        )
    }

    /// Asks for edge `e`. `true` = granted (now a holder); `false` = queued (FIFO).
    pub fn request(&mut self, e: usize, who: BlockId) -> bool {
        let s = &mut self.edges[e];
        if s.holders.contains(&who) {
            return true;
        }
        if s.holders.len() < s.capacity && s.queue.is_empty() {
            s.holders.push(who);
            true
        } else {
            if !s.queue.contains(&who) {
                s.queue.push_back(who);
            }
            false
        }
    }

    /// Releases edge `e`. Returns the waiting vehicle granted in its place, if any.
    pub fn release(&mut self, e: usize, who: BlockId) -> Option<BlockId> {
        let s = &mut self.edges[e];
        let at = s.holders.iter().position(|&h| h == who)?;
        s.holders.remove(at);
        if s.holders.len() < s.capacity {
            if let Some(next) = s.queue.pop_front() {
                s.holders.push(next);
                return Some(next);
            }
        }
        None
    }

    /// Drops everything `who` holds or waits for. Returns the vehicles granted as a
    /// consequence, with the edge each one got.
    pub fn forget(&mut self, who: BlockId) -> Vec<(BlockId, usize)> {
        let mut granted = Vec::new();
        for (e, s) in self.edges.iter_mut().enumerate() {
            s.queue.retain(|&q| q != who);
            if let Some(at) = s.holders.iter().position(|&h| h == who) {
                s.holders.remove(at);
                if s.holders.len() < s.capacity {
                    if let Some(next) = s.queue.pop_front() {
                        s.holders.push(next);
                        granted.push((next, e));
                    }
                }
            }
        }
        granted
    }

    /// Deadlock involving `who`, who has just started to wait.
    ///
    /// A set of waiting vehicles is deadlocked when every holder of every edge they
    /// wait for belongs to the set: nobody in it can ever release anything. The
    /// largest such set is found by repeatedly discarding vehicles that wait for an
    /// edge with a holder that is not waiting (that holder will eventually move on).
    /// Returns the members (ordered by vehicle id) when `who` is one of them.
    pub fn deadlock(&self, who: BlockId) -> Option<Vec<Waiter>> {
        let mut waiting: Vec<(BlockId, usize)> = Vec::new();
        for (e, s) in self.edges.iter().enumerate() {
            waiting.extend(s.queue.iter().map(|&q| (q, e)));
        }
        loop {
            let before = waiting.len();
            let snapshot: Vec<BlockId> = waiting.iter().map(|w| w.0).collect();
            waiting.retain(|&(_, e)| self.edges[e].holders.iter().all(|h| snapshot.contains(h)));
            if waiting.len() == before {
                break;
            }
        }
        if !waiting.iter().any(|w| w.0 == who) {
            return None;
        }
        waiting.sort();
        Some(
            waiting
                .into_iter()
                .map(|(vehicle, edge)| Waiter {
                    vehicle,
                    edge,
                    holders: self.edges[edge].holders.clone(),
                })
                .collect(),
        )
    }
}

/// Duration of each consecutive piece of a route of the given `lengths`, following
/// the trapezoidal profile of the **whole route** (see [`utsim_core::task::travel_time`]):
/// piece `k` takes `t(S_{k+1}) - t(S_k)`, where `t(s)` is the time the profile needs to
/// reach distance `s` and `S_k` the cumulative length. Without waiting, the pieces add
/// up to exactly `travel_time(total)`.
pub fn segment_durations(
    lengths: &[f64],
    speed: f64,
    accel: Option<f64>,
    decel: Option<f64>,
) -> Vec<f64> {
    let total: f64 = lengths.iter().sum();
    if total <= 0.0 || speed <= 0.0 {
        return vec![0.0; lengths.len()];
    }
    let (a, d) = (
        accel.unwrap_or(f64::INFINITY),
        decel.unwrap_or(f64::INFINITY),
    );
    let inv = 1.0 / a + 1.0 / d;
    let peak = if speed * speed / 2.0 * inv <= total {
        speed
    } else {
        (2.0 * total / inv).sqrt()
    };
    let (sa, sd) = (peak * peak / (2.0 * a), peak * peak / (2.0 * d));
    let (ta, td) = (peak / a, peak / d);
    let t_total = ta + (total - sa - sd).max(0.0) / peak + td;
    let time_at = |s: f64| -> f64 {
        if s <= sa {
            if sa > 0.0 { (2.0 * s / a).sqrt() } else { 0.0 }
        } else if s <= total - sd {
            ta + (s - sa) / peak
        } else if sd > 0.0 {
            t_total - (2.0 * (total - s).max(0.0) / d).sqrt()
        } else {
            t_total
        }
    };
    let mut out = Vec::with_capacity(lengths.len());
    let (mut s, mut t) = (0.0, 0.0);
    for (i, len) in lengths.iter().enumerate() {
        s += len;
        let t_next = if i + 1 == lengths.len() {
            t_total
        } else {
            time_at(s)
        };
        out.push(t_next - t);
        t = t_next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_core::task::travel_time;

    #[test]
    fn pieces_add_up_to_the_whole_route() {
        for (lens, v, a, d) in [
            (vec![10.0, 10.0], 2.0, Some(1.0), Some(1.0)),
            (vec![1.0, 1.0], 2.0, Some(1.0), Some(1.0)),
            (vec![3.0, 4.0, 5.0], 2.0, Some(1.0), Some(2.0)),
            (vec![3.0, 4.0], 2.0, None, None),
        ] {
            let pieces = segment_durations(&lens, v, a, d);
            let total: f64 = lens.iter().sum();
            let sum: f64 = pieces.iter().sum();
            assert!((sum - travel_time(total, v, a, d)).abs() < 1e-12);
            assert!(pieces.iter().all(|&p| p >= 0.0));
        }
    }

    #[test]
    fn constant_speed_pieces_are_length_over_speed() {
        assert_eq!(
            segment_durations(&[10.0, 5.0], 2.0, None, None),
            vec![5.0, 2.5]
        );
        // a = d = 1, v = 2, total 20: ramps 2 s each, cruise 16/2 = 8 s; the first
        // 10 m take 2 + (10 - 2)/2 = 6 s, the rest 12 - 6 = 6 s.
        assert_eq!(
            segment_durations(&[10.0, 10.0], 2.0, Some(1.0), Some(1.0)),
            vec![6.0, 6.0]
        );
    }
}
