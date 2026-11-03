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

// ---------------------------------------------------------------------------------------
// Pedestrian × vehicle crossings (aisles, movement `Aisles`)
// ---------------------------------------------------------------------------------------

/// A crossing between aisles of different kinds: where its conflict zone is.
#[derive(Clone, Debug, PartialEq)]
pub struct CrossingSpec {
    pub name: String,
    pub position: [f64; 3],
    /// Half length (m) of the conflict zone along any path through the crossing.
    pub radius: f64,
}

#[derive(Clone, Copy, Debug)]
struct Booking {
    who: BlockId,
    vehicle: bool,
    t_in: f64,
    t_out: f64,
}

/// Tolerance of the time comparisons (s).
const T_EPS: f64 = 1e-9;

/// Reservations of the crossings of one simulation (shared like [`Traffic`]).
///
/// **Rule (pedestrian priority, deterministic, no physics).** A vehicle always stops
/// `stop` seconds at the edge of the zone. Then it enters only if no pedestrian booking
/// overlaps its own crossing time; otherwise it waits (state `Blocked`) until the last
/// overlapping pedestrian leaves, and checks again. A vehicle books the zone only when it
/// enters. A pedestrian arriving at the zone books it at once: if a vehicle is inside it
/// waits until it leaves (the booking starts then), else it walks straight through. So a
/// pedestrian at or in the crossing always goes before a vehicle that has not entered,
/// and a vehicle already inside clears it first. Order of decisions = order of events.
pub struct Crossings {
    specs: Vec<CrossingSpec>,
    stop: f64,
    bookings: Vec<Vec<Booking>>,
    /// Vehicle waits per crossing (count, total seconds), beyond the mandatory stop.
    waits: Vec<(u64, f64)>,
}

impl Crossings {
    pub fn new(specs: Vec<CrossingSpec>, stop: f64) -> Self {
        let n = specs.len();
        Crossings {
            specs,
            stop: stop.max(0.0),
            bookings: vec![Vec::new(); n],
            waits: vec![(0, 0.0); n],
        }
    }

    /// Mandatory stop of a vehicle before a crossing (s).
    pub fn stop(&self) -> f64 {
        self.stop
    }

    pub fn specs(&self) -> &[CrossingSpec] {
        &self.specs
    }

    /// Crossing at `p` (within 1 mm on the floor).
    pub fn at(&self, p: [f64; 3]) -> Option<usize> {
        self.specs.iter().position(|s| {
            (s.position[0] - p[0]).abs() < 1e-3 && (s.position[1] - p[1]).abs() < 1e-3
        })
    }

    pub fn radius(&self, k: usize) -> f64 {
        self.specs[k].radius
    }

    /// Vehicle waits at crossing `k` beyond the stop: (count, total seconds).
    pub fn waits(&self, k: usize) -> (u64, f64) {
        self.waits[k]
    }

    fn prune(&mut self, k: usize, now: f64) {
        self.bookings[k].retain(|b| b.t_out > now - T_EPS);
    }

    /// A vehicle wants to cross `k` now, taking `dur`. `Ok` = booked, it enters;
    /// `Err(t)` = a pedestrian is (or will be) in the zone meanwhile: try again at `t`.
    pub fn vehicle_enter(&mut self, k: usize, who: BlockId, now: f64, dur: f64) -> Result<(), f64> {
        self.prune(k, now);
        let end = now + dur;
        let blocking = self.bookings[k]
            .iter()
            .filter(|b| !b.vehicle && b.t_in < end - T_EPS && b.t_out > now + T_EPS)
            .map(|b| b.t_out)
            .fold(f64::NEG_INFINITY, f64::max);
        if blocking.is_finite() {
            return Err(blocking);
        }
        self.bookings[k].push(Booking {
            who,
            vehicle: true,
            t_in: now,
            t_out: end,
        });
        Ok(())
    }

    /// Records a vehicle wait of `secs` at `k` (for the waiting metric).
    pub fn record_wait(&mut self, k: usize, secs: f64) {
        self.waits[k].0 += 1;
        self.waits[k].1 += secs;
    }

    /// A pedestrian reaches the zone of `k` now and takes `dur` to cross it: books it and
    /// returns when it enters (now, or when the vehicles inside leave).
    pub fn pedestrian_enter(&mut self, k: usize, who: BlockId, now: f64, dur: f64) -> f64 {
        self.prune(k, now);
        let t_in = self.bookings[k]
            .iter()
            .filter(|b| b.vehicle && b.t_out > now + T_EPS)
            .map(|b| b.t_out)
            .fold(now, f64::max);
        self.bookings[k].push(Booking {
            who,
            vehicle: false,
            t_in,
            t_out: t_in + dur,
        });
        t_in
    }

    /// Drops the bookings of `who` that have not started (a trip given up).
    pub fn forget(&mut self, who: BlockId, now: f64) {
        for list in &mut self.bookings {
            list.retain(|b| b.who != who || b.t_in <= now + T_EPS);
        }
    }
}

/// One piece of a trip through crossings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub dur: f64,
    /// First piece inside the zone of a crossing: (crossing, time to cross the zone).
    pub cross: Option<(usize, f64)>,
    /// Standing still (mandatory stop before a crossing).
    pub stop: bool,
}

/// Splits a travel `path` at the conflict zones of the crossings it passes through (its
/// vertices on a crossing). `None` when it passes none (the travel is unchanged).
///
/// A pedestrian keeps the trapezoidal profile of the whole path (the pieces add up to
/// [`utsim_core::task::travel_time`]). A vehicle stops at the edge of every zone: the
/// path becomes runs from rest to rest (start → first zone, zone → next zone, ...), each
/// with its own profile, and a stop piece of [`Crossings::stop`] seconds before each zone.
pub fn crossing_plan(
    path: &[[f64; 3]],
    board: &Crossings,
    vehicle: bool,
    speed: f64,
    accel: Option<f64>,
    decel: Option<f64>,
) -> Option<Vec<Piece>> {
    use utsim_core::task::distance;
    if path.len() < 2 {
        return None;
    }
    let mut cum = vec![0.0];
    for w in path.windows(2) {
        cum.push(cum.last().copied().unwrap_or(0.0) + distance(w[0], w[1]));
    }
    let total = *cum.last()?;
    if total <= 0.0 {
        return None;
    }
    let mut zones: Vec<(f64, f64, usize)> = Vec::new();
    let mut last_end = 0.0;
    for (i, p) in path.iter().enumerate() {
        let Some(k) = board.at(*p) else { continue };
        if zones.last().is_some_and(|z| z.2 == k) {
            continue;
        }
        let r = board.radius(k);
        let (s0, s1) = ((cum[i] - r).max(last_end), (cum[i] + r).min(total));
        if s1 - s0 <= 1e-9 {
            continue;
        }
        zones.push((s0, s1, k));
        last_end = s1;
    }
    if zones.is_empty() {
        return None;
    }
    let mut cuts: Vec<f64> = cum.clone();
    for z in &zones {
        cuts.extend([z.0, z.1]);
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    let point_at = |s: f64| -> [f64; 3] {
        for i in 0..path.len() - 1 {
            if s <= cum[i + 1] + 1e-12 {
                let seg = cum[i + 1] - cum[i];
                let f = if seg > 0.0 { (s - cum[i]) / seg } else { 1.0 };
                if f <= 1e-12 {
                    return path[i];
                }
                if f >= 1.0 - 1e-12 {
                    return path[i + 1];
                }
                let (a, b) = (path[i], path[i + 1]);
                return [
                    a[0] + (b[0] - a[0]) * f,
                    a[1] + (b[1] - a[1]) * f,
                    a[2] + (b[2] - a[2]) * f,
                ];
            }
        }
        path[path.len() - 1]
    };
    // Elementary pieces: (start, end, zone index).
    let elems: Vec<(f64, f64, Option<usize>)> = cuts
        .windows(2)
        .filter(|w| w[1] - w[0] > 1e-9)
        .map(|w| {
            let z = zones
                .iter()
                .position(|z| w[0] >= z.0 - 1e-9 && w[1] <= z.1 + 1e-9);
            (w[0], w[1], z)
        })
        .collect();
    let first_of_zone: Vec<bool> = elems
        .iter()
        .enumerate()
        .map(|(j, e)| e.2.is_some() && (j == 0 || elems[j - 1].2 != e.2))
        .collect();
    // Runs: the whole path for a pedestrian; rest to rest for a vehicle.
    let mut durs = vec![0.0; elems.len()];
    let mut start = 0;
    for j in 0..=elems.len() {
        let boundary = j == elems.len() || (vehicle && j > 0 && first_of_zone[j]);
        if boundary {
            let lens: Vec<f64> = elems[start..j].iter().map(|e| e.1 - e.0).collect();
            let d = segment_durations(&lens, speed, accel, decel);
            durs[start..j].copy_from_slice(&d);
            start = j;
        }
    }
    let mut out = Vec::new();
    for (j, e) in elems.iter().enumerate() {
        let mut cross = None;
        if first_of_zone[j] {
            let z = e.2.expect("zone");
            let zone_dur: f64 = (j..elems.len())
                .take_while(|&i| elems[i].2 == Some(z))
                .map(|i| durs[i])
                .sum();
            if vehicle && board.stop() > 0.0 {
                let p = point_at(e.0);
                out.push(Piece {
                    a: p,
                    b: p,
                    dur: board.stop(),
                    cross: None,
                    stop: true,
                });
            }
            cross = Some((zones[z].2, zone_dur));
        }
        out.push(Piece {
            a: point_at(e.0),
            b: point_at(e.1),
            dur: durs[j],
            cross,
            stop: false,
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_core::task::travel_time;

    fn board() -> Crossings {
        Crossings::new(
            vec![CrossingSpec {
                name: "x".into(),
                position: [10.0, 0.0, 0.0],
                radius: 1.0,
            }],
            2.0,
        )
    }

    #[test]
    fn paths_without_crossings_are_unchanged() {
        let p = [[0.0, 5.0, 0.0], [20.0, 5.0, 0.0]];
        assert!(crossing_plan(&p, &board(), true, 1.0, None, None).is_none());
    }

    #[test]
    fn a_vehicle_stops_before_the_zone_and_a_pedestrian_does_not() {
        let p = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0]];
        let v = crossing_plan(&p, &board(), true, 1.0, None, None).unwrap();
        // 0→9, stop 2 s, 9→10 (zone), 10→11 (zone), 11→20.
        let durs: Vec<f64> = v.iter().map(|x| x.dur).collect();
        assert_eq!(durs, vec![9.0, 2.0, 1.0, 1.0, 9.0]);
        assert!(v[1].stop && v[1].a == [9.0, 0.0, 0.0]);
        assert_eq!(v[2].cross, Some((0, 2.0)));
        let w = crossing_plan(&p, &board(), false, 1.0, None, None).unwrap();
        let sum: f64 = w.iter().map(|x| x.dur).sum();
        assert_eq!(sum, 20.0);
        assert!(w.iter().all(|x| !x.stop));
        // With acceleration, the pedestrian keeps the whole-route profile.
        let w = crossing_plan(&p, &board(), false, 2.0, Some(1.0), Some(1.0)).unwrap();
        let sum: f64 = w.iter().map(|x| x.dur).sum();
        assert!((sum - travel_time(20.0, 2.0, Some(1.0), Some(1.0))).abs() < 1e-12);
    }

    #[test]
    fn vehicles_yield_to_pedestrians_in_the_zone() {
        let mut b = board();
        let (car, man) = (BlockId(0), BlockId(1));
        // Pedestrian enters at 5, crossing takes 2 s.
        assert_eq!(b.pedestrian_enter(0, man, 5.0, 2.0), 5.0);
        // Vehicle at 6 needs 2 s: overlap, retry at 7.
        assert_eq!(b.vehicle_enter(0, car, 6.0, 2.0), Err(7.0));
        // Vehicle wanting to go at 3 for 2 s would still be inside at 5: retry at 7.
        assert_eq!(b.vehicle_enter(0, car, 3.0, 2.5), Err(7.0));
        assert_eq!(b.vehicle_enter(0, car, 7.0, 2.0), Ok(()));
        // A pedestrian arriving at 8 waits for the vehicle inside (until 9).
        assert_eq!(b.pedestrian_enter(0, man, 8.0, 2.0), 9.0);
    }
}

#[cfg(test)]
mod durations_tests {
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
