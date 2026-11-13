//! Material handling in the viewer (phase 5): conveyors and racks. Pure data and
//! kinematics, no Bevy.
//!
//! **Conveyor positions are rebuilt from the trace.** The trace says only when an item
//! entered the belt (`Moved` into it), when it left (`Moved` out of it) and when the
//! belt stopped or resumed (the block's `down`/`off_shift` states). From these, plus
//! the model's `length`, `speed`, `item_length` and `accumulating`, [`BeltTrack`]
//! replays the same kinematics as `utsim_objects::Conveyor`:
//!
//! - an item advances at `speed` from the entry and never goes past
//!   `length - k * item_length` (`k` = its index counted from the front), so it stops
//!   one `item_length` behind the item ahead;
//! - the front item reaching `length` is "waiting"; it keeps waiting until its exit
//!   event. On a non-accumulating belt the whole belt stops meanwhile;
//! - a breakdown or shift stops the belt, which resumes from where it was.
//!
//! The arrival of the front item is not in the trace (only its exit is); it is
//! computed. When the trace says the front item left before the computed arrival, the
//! item is snapped to the end and the difference is recorded ([`BeltTrack::max_snap`]),
//! which stays at rounding level when the reconstruction matches the engine.
//!
//! The result is, per belt visit of an item, a piecewise linear distance-versus-time
//! curve, so a query is a binary search plus one interpolation.
//!
//! Racks need no reconstruction: the `Slot` events give each item its position
//! (`level * bays + bay`); this module holds the geometry of a slot.

use std::collections::HashMap;

use utsim_model::ObjectKind;

/// Belt parameters as in the model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeltParams {
    pub length: f64,
    pub speed: f64,
    pub item_length: f64,
    pub accumulating: bool,
}

impl Default for BeltParams {
    /// Used when the model is not available (a `.json` replay): the editor default.
    fn default() -> Self {
        BeltParams {
            length: 5.0,
            speed: 1.0,
            item_length: 1.0,
            accumulating: true,
        }
    }
}

/// Width of the belt surface, in scene units.
pub const BELT_WIDTH: f32 = 1.1;
/// Thickness of the belt slab.
pub const BELT_THICKNESS: f32 = 0.25;
/// Largest belt drawn (model values beyond it are clamped for drawing only).
pub const MAX_BELT_LENGTH: f64 = 400.0;
/// Most bays and levels drawn by a rack.
pub const MAX_BAYS: usize = 60;
pub const MAX_LEVELS: usize = 20;
/// Thickness of a rack beam; items rest on top of it.
pub const BEAM: f32 = 0.08;
/// Depth of a rack (along the aisle direction, Bevy Z).
pub const RACK_DEPTH: f32 = 1.0;

fn finite_or(v: f64, default: f64) -> f64 {
    if v.is_finite() && v > 0.0 { v } else { default }
}

impl BeltParams {
    /// Values safe to draw (invalid ones, which the validator rejects, are replaced).
    pub fn sanitized(self) -> Self {
        let item_length = finite_or(self.item_length, 1.0);
        BeltParams {
            length: finite_or(self.length, 5.0).clamp(item_length.min(0.5), MAX_BELT_LENGTH),
            speed: finite_or(self.speed, 1.0),
            item_length,
            accumulating: self.accumulating,
        }
    }

    /// Edge of the cube drawn for an item on this belt.
    pub fn item_size(&self) -> f32 {
        ((0.8 * self.item_length) as f32).clamp(0.1, 0.75)
    }
}

/// Rack parameters (`bay_width`/`level_height` default to 1.0 for replays without a model).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RackParams {
    pub bays: usize,
    pub levels: usize,
    pub bay_width: f64,
    pub level_height: f64,
}

impl RackParams {
    /// A rack guessed from the highest slot seen in a trace (the trace does not carry
    /// the shape): about 1.5 times wider than tall, spacing 1.0.
    pub fn guess(max_slot: Option<usize>) -> Self {
        let total = max_slot.map_or(1, |m| m + 1);
        let bays = (((total as f64) * 1.5).sqrt().ceil() as usize).max(1);
        RackParams {
            bays,
            levels: total.div_ceil(bays).max(1),
            bay_width: 1.0,
            level_height: 1.0,
        }
    }

    pub fn sanitized(self) -> Self {
        RackParams {
            bays: self.bays.clamp(1, MAX_BAYS),
            levels: self.levels.clamp(1, MAX_LEVELS),
            bay_width: finite_or(self.bay_width, 1.0),
            level_height: finite_or(self.level_height, 1.0),
        }
    }

    /// Edge of the cube drawn for a stored item.
    pub fn item_size(&self) -> f32 {
        ((0.6 * self.bay_width) as f32)
            .min((0.6 * self.level_height) as f32)
            .clamp(0.1, 0.7)
    }

    /// `(bay, level)` of a storage slot (`slot = level * bays + bay`).
    pub fn bay_level(&self, slot: usize) -> (usize, usize) {
        (slot % self.bays, slot / self.bays)
    }

    pub fn total_width(&self) -> f32 {
        (self.bays as f64 * self.bay_width) as f32
    }

    pub fn total_height(&self) -> f32 {
        (self.levels as f64 * self.level_height) as f32 + BEAM
    }

    /// Center of the item resting in `slot`, relative to the rack's ground center:
    /// `(x along the bays, y above the ground)`.
    pub fn slot_center(&self, slot: usize) -> (f32, f32) {
        let (bay, level) = self.bay_level(slot);
        let x = ((bay as f64 + 0.5) * self.bay_width) as f32 - self.total_width() / 2.0;
        let y = (level as f64 * self.level_height) as f32 + BEAM + self.item_size() / 2.0;
        (x, y)
    }
}

/// What a block handles, when it is a conveyor or a rack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handling {
    Conveyor(BeltParams),
    Rack(RackParams),
}

impl Handling {
    pub fn of(kind: &ObjectKind) -> Option<Self> {
        match kind {
            ObjectKind::Conveyor {
                item_length,
                accumulating,
                ..
            } => {
                // A drawn belt: the length of its route and the speed that gives the same
                // transit time, as the simulation uses them.
                let (length, speed) =
                    utsim_model::trajeto::belt_params_of(kind).unwrap_or((5.0, 1.0));
                Some(Handling::Conveyor(BeltParams {
                    length,
                    speed,
                    item_length: *item_length,
                    accumulating: *accumulating,
                }))
            }
            ObjectKind::Rack {
                bays,
                levels,
                bay_width,
                level_height,
                ..
            } => Some(Handling::Rack(RackParams {
                bays: *bays,
                levels: *levels,
                bay_width: *bay_width,
                level_height: *level_height,
            })),
            _ => None,
        }
    }
}

/// Something that changed the belt, in trace order. `item` is a dense item index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BeltEv {
    In {
        t: f64,
        item: usize,
    },
    Out {
        t: f64,
        item: usize,
    },
    /// The belt stopped (`true`) or resumed (`false`).
    Halt {
        t: f64,
        on: bool,
    },
}

impl BeltEv {
    pub fn time(&self) -> f64 {
        match *self {
            BeltEv::In { t, .. } | BeltEv::Out { t, .. } | BeltEv::Halt { t, .. } => t,
        }
    }
}

/// One stay of an item on the belt: breakpoints `(time, distance from the start)`.
#[derive(Clone, Debug, Default)]
struct Visit {
    points: Vec<(f64, f64)>,
}

impl Visit {
    /// Adds a breakpoint, merging it with the previous segment when collinear.
    fn add(&mut self, t: f64, p: f64, scale: f64) {
        if let Some(&(lt, _)) = self.points.last()
            && t <= lt
        {
            if t == lt {
                self.points.last_mut().expect("non-empty").1 = p;
            }
            return;
        }
        let n = self.points.len();
        if n >= 2 {
            let (t1, p1) = self.points[n - 2];
            let (t2, p2) = self.points[n - 1];
            // Cross product of the two segments: zero when they are collinear.
            if ((p2 - p1) * (t - t2) - (p - p2) * (t2 - t1)).abs() <= 1e-12 * scale * (t - t1) {
                self.points[n - 1] = (t, p);
                return;
            }
        }
        self.points.push((t, p));
    }

    fn at(&self, t: f64) -> Option<f64> {
        let first = self.points.first()?;
        if t < first.0 {
            return None;
        }
        let k = self.points.partition_point(|p| p.0 <= t);
        let (t0, p0) = self.points[k - 1];
        let Some(&(t1, p1)) = self.points.get(k) else {
            return Some(p0);
        };
        Some(p0 + (p1 - p0) * (t - t0) / (t1 - t0))
    }
}

/// Reconstructed motion of every item that rode one belt.
#[derive(Clone, Debug, Default)]
pub struct BeltTrack {
    params: BeltParams,
    /// Per item (dense index), its visits in time order.
    visits: HashMap<usize, Vec<Visit>>,
    /// `(item, time)` the item reached the end of the belt (computed, or the exit if earlier).
    arrivals: Vec<(usize, f64)>,
    /// `(item, time)` the item left the belt (from the trace).
    exits: Vec<(usize, f64)>,
    max_snap: f64,
    /// How far the belt surface itself has moved, `(time, metres)` at every change: it
    /// stops with a breakdown or shift, and a non-accumulating belt stops while its front
    /// item waits at the end (the 3D view scrolls the belt by it).
    travel: Vec<(f64, f64)>,
    /// The same, counting only while there are items on the belt (rollers turn under
    /// the items of a gravity belt).
    loaded: Vec<(f64, f64)>,
}

struct Builder {
    p: BeltParams,
    /// Items on the belt, front first, with their distance as of `since`.
    items: Vec<usize>,
    pos: Vec<f64>,
    since: f64,
    halted: bool,
    front_arrived: bool,
    track: BeltTrack,
    /// Index of each item's open visit.
    open: HashMap<usize, usize>,
}

impl Builder {
    fn moving(&self) -> bool {
        !self.halted && !(self.front_arrived && !self.p.accumulating)
    }

    fn target(&self, k: usize) -> f64 {
        self.p.length - k as f64 * self.p.item_length
    }

    fn visit(&mut self, item: usize) -> Option<&mut Visit> {
        let at = *self.open.get(&item)?;
        self.track.visits.get_mut(&item)?.get_mut(at)
    }

    /// Brings every item up to time `t` (never moves time backwards).
    fn sync(&mut self, t: f64) {
        if t <= self.since {
            self.since = self.since.max(t);
            return;
        }
        let dt = t - self.since;
        let moving = self.moving();
        let scale = self.p.length.max(1.0);
        let run = if moving { self.p.speed * dt } else { 0.0 };
        let last = |v: &Vec<(f64, f64)>| v.last().map_or(0.0, |p| p.1);
        let s = last(&self.track.travel) + run;
        self.track.travel.push((t, s));
        let l = last(&self.track.loaded) + if self.items.is_empty() { 0.0 } else { run };
        self.track.loaded.push((t, l));
        for k in 0..self.items.len() {
            let (p0, target) = (self.pos[k], self.target(k));
            let (p1, clamp_at) = if moving {
                let free = p0 + self.p.speed * dt;
                if free > target {
                    (
                        target.min(free),
                        (p0 < target).then(|| self.since + (target - p0) / self.p.speed),
                    )
                } else {
                    (free, None)
                }
            } else {
                (p0, None)
            };
            let item = self.items[k];
            if let Some(v) = self.visit(item) {
                if let Some(tc) = clamp_at {
                    v.add(tc.min(t), target, scale);
                }
                v.add(t, p1, scale);
            }
            self.pos[k] = p1;
        }
        self.since = t;
    }

    /// Handles the front item reaching the end up to time `t`.
    fn advance_to(&mut self, t: f64) {
        while self.moving() && !self.front_arrived && !self.items.is_empty() {
            let ta = self.since + (self.p.length - self.pos[0]) / self.p.speed;
            if ta > t {
                break;
            }
            self.sync(ta);
            self.arrive(ta);
        }
    }

    fn arrive(&mut self, t: f64) {
        let len = self.p.length;
        self.pos[0] = len;
        let item = self.items[0];
        let scale = len.max(1.0);
        if let Some(v) = self.visit(item) {
            v.add(t, len, scale);
        }
        self.front_arrived = true;
        self.track.arrivals.push((item, t));
    }

    fn enter(&mut self, t: f64, item: usize) {
        self.advance_to(t);
        self.sync(t);
        self.items.push(item);
        self.pos.push(0.0);
        let visits = self.track.visits.entry(item).or_default();
        visits.push(Visit {
            points: vec![(t, 0.0)],
        });
        self.open.insert(item, visits.len() - 1);
    }

    fn leave(&mut self, t: f64, item: usize) {
        self.advance_to(t);
        self.sync(t);
        let Some(k) = self.items.iter().position(|&i| i == item) else {
            return;
        };
        if k == 0 {
            if !self.front_arrived {
                // The trace says it left before the computed arrival: snap to the end.
                self.track.max_snap = self.track.max_snap.max((self.p.length - self.pos[0]).abs());
                self.arrive(t);
            }
            self.front_arrived = false;
        } else {
            // Not the front (only possible if the model changed since the run): drop it.
            self.track.max_snap = self.track.max_snap.max(self.p.length);
        }
        self.track.exits.push((item, t));
        self.items.remove(k);
        self.pos.remove(k);
        self.open.remove(&item);
    }
}

impl BeltTrack {
    /// Replays `events` (trace order) up to time `end` with the belt `params`.
    pub fn build(params: BeltParams, events: &[BeltEv], end: f64) -> Self {
        let p = params.sanitized();
        let mut b = Builder {
            p,
            items: Vec::new(),
            pos: Vec::new(),
            since: events.first().map_or(0.0, BeltEv::time),
            halted: false,
            front_arrived: false,
            track: BeltTrack {
                params: p,
                ..Default::default()
            },
            open: HashMap::new(),
        };
        b.track.travel.push((b.since, 0.0));
        b.track.loaded.push((b.since, 0.0));
        for ev in events {
            match *ev {
                BeltEv::In { t, item } => b.enter(t, item),
                BeltEv::Out { t, item } => b.leave(t, item),
                BeltEv::Halt { t, on } => {
                    b.advance_to(t);
                    b.sync(t);
                    b.halted = on;
                }
            }
        }
        if end.is_finite() && end > b.since {
            b.advance_to(end);
            b.sync(end);
        }
        b.track
    }

    pub fn params(&self) -> BeltParams {
        self.params
    }

    /// How far the belt surface has moved by `t` (m); `loaded`: only while carrying items.
    pub fn surface_travel(&self, t: f64, loaded: bool) -> f64 {
        let v = if loaded { &self.loaded } else { &self.travel };
        let k = v.partition_point(|p| p.0 <= t);
        match (k.checked_sub(1).map(|i| v[i]), v.get(k)) {
            (Some(a), Some(b)) if b.0 > a.0 => a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0),
            (Some(a), _) => a.1,
            (None, _) => 0.0,
        }
    }

    /// Distance from the start of the belt of `item` at `t`, if it is on the belt then.
    /// After its exit the answer is the end of the belt.
    pub fn position_of(&self, item: usize, t: f64) -> Option<f64> {
        let visits = self.visits.get(&item)?;
        let k = visits
            .partition_point(|v| v.points.first().is_some_and(|p| p.0 <= t))
            .checked_sub(1)?;
        visits[k].at(t)
    }

    /// Largest distance from the end of the belt that an item had when the trace said
    /// it left (0 when the reconstruction matches the engine, up to rounding).
    pub fn max_snap(&self) -> f64 {
        self.max_snap
    }

    /// `(item, arrival time at the end, exit time)` for every item that left the belt.
    pub fn arrivals_and_exits(&self) -> Vec<(usize, f64, f64)> {
        self.exits
            .iter()
            .filter_map(|&(item, te)| {
                self.arrivals
                    .iter()
                    .rev()
                    .find(|&&(i, ta)| i == item && ta <= te + 1e-9)
                    .map(|&(_, ta)| (item, ta, te))
            })
            .collect()
    }

    /// Items with a recorded visit.
    pub fn items(&self) -> impl Iterator<Item = usize> + '_ {
        self.visits.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn belt(accumulating: bool) -> BeltParams {
        BeltParams {
            length: 10.0,
            speed: 2.0,
            item_length: 2.0,
            accumulating,
        }
    }

    fn close(a: Option<f64>, b: f64) {
        let a = a.expect("on the belt");
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn free_travel_takes_length_over_speed() {
        // One item, entry at 1, no obstruction: arrives at 1 + 10 / 2 = 6 and leaves then.
        let ev = [
            BeltEv::In { t: 1.0, item: 7 },
            BeltEv::Out { t: 6.0, item: 7 },
        ];
        let tr = BeltTrack::build(belt(true), &ev, 20.0);
        close(tr.position_of(7, 1.0), 0.0);
        close(tr.position_of(7, 3.0), 4.0);
        close(tr.position_of(7, 6.0), 10.0);
        assert!(tr.position_of(7, 0.5).is_none());
        assert!(tr.position_of(8, 3.0).is_none());
        assert!(tr.max_snap() < 1e-9);
        let (_, ta, te) = tr.arrivals_and_exits()[0];
        assert!((ta - 6.0).abs() < 1e-9 && te == 6.0);
    }

    #[test]
    fn accumulating_items_stack_behind_a_waiting_front() {
        // Items 1, 2, 3 enter at 0, 1.2, 2.4 and the front leaves only at 20.
        let ev = [
            BeltEv::In { t: 0.0, item: 1 },
            BeltEv::In { t: 1.2, item: 2 },
            BeltEv::In { t: 2.4, item: 3 },
            BeltEv::Out { t: 20.0, item: 1 },
            BeltEv::Out { t: 24.0, item: 2 },
        ];
        let tr = BeltTrack::build(belt(true), &ev, 30.0);
        close(tr.position_of(1, 5.0), 10.0);
        close(tr.position_of(2, 3.0), 3.6);
        // Item 2 stops one item length behind the front (8), item 3 two behind (6).
        close(tr.position_of(2, 15.0), 8.0);
        close(tr.position_of(3, 15.0), 6.0);
        // When 1 leaves, 2 advances from 8 to 10 in 1 time unit and waits until 24.
        close(tr.position_of(2, 20.5), 9.0);
        close(tr.position_of(2, 22.0), 10.0);
        close(tr.position_of(3, 22.0), 8.0);
        assert!(tr.max_snap() < 1e-9);
    }

    #[test]
    fn non_accumulating_belt_stops_while_the_front_waits() {
        // Item 1 enters at 0 and waits at the end from 5 to 9; item 2 enters at 3.
        let ev = [
            BeltEv::In { t: 0.0, item: 1 },
            BeltEv::In { t: 3.0, item: 2 },
            BeltEv::Out { t: 9.0, item: 1 },
        ];
        let tr = BeltTrack::build(belt(false), &ev, 20.0);
        // At 5 item 2 is at 4 and the belt stops until 9.
        close(tr.position_of(2, 5.0), 4.0);
        close(tr.position_of(2, 8.9), 4.0);
        // Resumes at 9: reaches the end 3 time units later.
        close(tr.position_of(2, 10.0), 6.0);
        close(tr.position_of(2, 12.0), 10.0);
    }

    #[test]
    fn a_halted_belt_keeps_positions_and_resumes_from_there() {
        let ev = [
            BeltEv::In { t: 0.0, item: 1 },
            BeltEv::Halt { t: 2.0, on: true },
            BeltEv::Halt { t: 5.0, on: false },
            BeltEv::Out { t: 8.0, item: 1 },
        ];
        let tr = BeltTrack::build(belt(true), &ev, 20.0);
        close(tr.position_of(1, 2.0), 4.0);
        close(tr.position_of(1, 4.0), 4.0);
        close(tr.position_of(1, 6.0), 6.0);
        // 3 time units of travel were lost: arrival at 5 + 3 = 8.
        close(tr.position_of(1, 8.0), 10.0);
        assert!(tr.max_snap() < 1e-9);
    }

    #[test]
    fn an_early_exit_is_snapped_and_reported() {
        // The trace says the item left at 3, but it could not have reached the end.
        let ev = [
            BeltEv::In { t: 0.0, item: 1 },
            BeltEv::Out { t: 3.0, item: 1 },
        ];
        let tr = BeltTrack::build(belt(true), &ev, 10.0);
        assert!((tr.max_snap() - 4.0).abs() < 1e-9);
        close(tr.position_of(1, 3.0), 10.0);
    }

    #[test]
    fn collinear_breakpoints_are_merged() {
        let ev = [
            BeltEv::In { t: 0.0, item: 1 },
            BeltEv::In { t: 1.0, item: 2 },
            BeltEv::In { t: 2.0, item: 3 },
            BeltEv::Out { t: 5.0, item: 1 },
            BeltEv::Out { t: 6.0, item: 2 },
            BeltEv::Out { t: 7.0, item: 3 },
        ];
        let tr = BeltTrack::build(belt(true), &ev, 20.0);
        // Each item is entry + exit only: free flow adds no breakpoints.
        for item in 1..=3 {
            assert_eq!(tr.visits[&item][0].points.len(), 2, "item {item}");
        }
    }

    #[test]
    fn rack_slots_are_laid_out_level_by_level() {
        let rack = RackParams {
            bays: 3,
            levels: 2,
            bay_width: 2.0,
            level_height: 1.5,
        };
        assert_eq!(rack.bay_level(0), (0, 0));
        assert_eq!(rack.bay_level(4), (1, 1));
        let (x0, y0) = rack.slot_center(0);
        let (x2, _) = rack.slot_center(2);
        let (_, y3) = rack.slot_center(3);
        assert!((x0 + 2.0).abs() < 1e-6 && (x2 - 2.0).abs() < 1e-6);
        assert!(y3 - y0 > 1.49 && y3 - y0 < 1.51);
        assert!(rack.total_height() > 3.0);
        let guess = RackParams::guess(Some(23));
        assert!(guess.bays * guess.levels >= 24);
    }

    #[test]
    fn invalid_values_are_replaced_for_drawing() {
        let p = BeltParams {
            length: f64::NAN,
            speed: -1.0,
            item_length: 0.0,
            accumulating: true,
        }
        .sanitized();
        assert!(p.length > 0.0 && p.speed > 0.0 && p.item_length > 0.0);
        let r = RackParams {
            bays: 0,
            levels: 10_000,
            bay_width: f64::INFINITY,
            level_height: 0.0,
        }
        .sanitized();
        assert_eq!((r.bays, r.levels), (1, MAX_LEVELS));
        assert!(r.bay_width.is_finite() && r.level_height > 0.0);
    }

    #[test]
    fn a_non_accumulating_belt_stops_while_its_front_item_waits() {
        // 4 m at 1 m/s: the item arrives at t = 4 and leaves at t = 10; the belt stands
        // still from 4 to 10 and runs again after.
        let p = BeltParams {
            length: 4.0,
            speed: 1.0,
            item_length: 1.0,
            accumulating: false,
        };
        let ev = [
            BeltEv::In { t: 0.0, item: 0 },
            BeltEv::Out { t: 10.0, item: 0 },
        ];
        let tr = BeltTrack::build(p, &ev, 12.0);
        assert!((tr.surface_travel(2.0, false) - 2.0).abs() < 1e-9);
        assert!((tr.surface_travel(7.0, false) - 4.0).abs() < 1e-9);
        assert!((tr.surface_travel(12.0, false) - 6.0).abs() < 1e-9);
        // Loaded only while the item rode: 4 m.
        assert!((tr.surface_travel(12.0, true) - 4.0).abs() < 1e-9);
        let mut acc = p;
        acc.accumulating = true;
        let tr = BeltTrack::build(acc, &ev, 12.0);
        assert!((tr.surface_travel(7.0, false) - 7.0).abs() < 1e-9);
    }
}
