//! Pure replay index: answers "what does the world look like at time `t`?" for any `t`.
//!
//! No Bevy types here. The index is built once from a [`Replay`]; every query is a
//! binary search over a sorted per-entity event list (`O(log n)` per entity), so
//! scrubbing anywhere in a long recording costs the same as playing forward.
//!
//! Semantics. A query at time `t` sees every record with `record.t <= t` (all events
//! of an instant are applied). Transfers are instantaneous in the simulation; for the
//! eye, a `Moved` event is drawn as a *transit* from `from` to `to` that starts at the
//! event time and lasts `transit` time units (never past the item's next event). The
//! simulation-true content counts already include a transiting item in `to`.
//!
//! Mobile resources (phase 4). Moves into or out of a transporter are real pick-ups
//! and drop-offs, so they get no visual transit: the item simply sits inside the
//! carrier ([`ItemPos::At`] of the transporter block) and the carrier's own motion,
//! from `Travel` events, shows the movement. The carrier's position is piecewise
//! linear along the path by arc length, and linear in time over the leg's duration.
//! The trapezoidal speed profile (acceleration/deceleration) cannot be inferred from
//! the trace alone, which carries only path and duration, so it is not applied; the
//! leg endpoints and total time are exact, only the mid-leg phase differs.

use std::collections::HashMap;

use utsim_core::{State, TraceEvent};
use utsim_model::replay::Replay;

/// Where an item is, at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemPos {
    /// Not created yet.
    NotYet,
    /// Resting inside block `b`.
    At(usize),
    /// Moving from block `from` to block `to`; `progress` in `0..1`.
    Transit {
        from: usize,
        to: usize,
        progress: f64,
    },
    /// Packed inside another item (dense item index).
    Inside(usize),
    /// Already left the model.
    Gone,
}

/// How long the visual transit of a move lasts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitSetting {
    /// 0.4 x median gap between consecutive events of the item, clamped.
    Auto,
    /// A fixed duration in simulation time units (still cut at the next event).
    Fixed(f64),
}

#[derive(Clone, Copy, Debug)]
enum ItemStep {
    Created(usize),
    Moved {
        from: usize,
        to: usize,
        transit: f64,
    },
    Packed(usize),
    Unpacked(usize),
    Destroyed,
}

#[derive(Clone, Debug, Default)]
struct ItemTrack {
    /// Sorted by time (ties keep trace order).
    steps: Vec<(f64, ItemStep)>,
    created: f64,
    destroyed: f64,
}

/// One `Travel` event of a mobile resource.
#[derive(Clone, Debug)]
struct TravelLeg {
    t0: f64,
    duration: f64,
    path: Vec<[f64; 3]>,
    /// Arc length from the start of the path to each point.
    cum: Vec<f64>,
}

impl TravelLeg {
    fn new(t0: f64, duration: f64, path: &[[f64; 3]]) -> Option<Self> {
        if path.is_empty() {
            return None;
        }
        let mut cum = Vec::with_capacity(path.len());
        let mut total = 0.0;
        cum.push(0.0);
        for w in path.windows(2) {
            total += dist3(w[0], w[1]);
            cum.push(total);
        }
        Some(TravelLeg {
            t0,
            duration,
            path: path.to_vec(),
            cum,
        })
    }

    fn at(&self, t: f64) -> [f64; 3] {
        let last = self.path.len() - 1;
        let total = self.cum[last];
        let elapsed = t - self.t0;
        if self.duration <= 0.0 || elapsed >= self.duration || total <= 0.0 {
            return self.path[last];
        }
        let target = (elapsed / self.duration).clamp(0.0, 1.0) * total;
        let k = self.cum.partition_point(|&c| c <= target).clamp(1, last);
        let (c0, c1) = (self.cum[k - 1], self.cum[k]);
        let f = if c1 > c0 {
            (target - c0) / (c1 - c0)
        } else {
            0.0
        };
        let (a, b) = (self.path[k - 1], self.path[k]);
        [
            a[0] + (b[0] - a[0]) * f,
            a[1] + (b[1] - a[1]) * f,
            a[2] + (b[2] - a[2]) * f,
        ]
    }
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Block kinds that move (their items are carried, not shown on a fixed surface).
pub fn is_mobile_kind(kind: &str) -> bool {
    matches!(kind, "Transporter" | "Dispatcher")
}

#[derive(Clone, Debug, Default)]
struct BlockTrack {
    states: Vec<(f64, State)>,
    /// Content count right after each change (sorted by time).
    content: Vec<(f64, usize)>,
}

/// Interval-stabbing helper: items sorted by creation time with a max-destroy segment
/// tree on top, so "alive at `t`" costs `O(k log n)` for `k` results.
#[derive(Clone, Debug, Default)]
struct LiveIndex {
    /// Item indices sorted by creation time.
    order: Vec<usize>,
    created: Vec<f64>,
    /// Segment tree of max `destroyed`, size `2 * size`.
    tree: Vec<f64>,
    size: usize,
}

impl LiveIndex {
    fn build(items: &[ItemTrack]) -> Self {
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by(|&a, &b| items[a].created.total_cmp(&items[b].created));
        let created: Vec<f64> = order.iter().map(|&i| items[i].created).collect();
        let destroyed: Vec<f64> = order.iter().map(|&i| items[i].destroyed).collect();
        let size = order.len().next_power_of_two().max(1);
        let mut tree = vec![f64::NEG_INFINITY; 2 * size];
        tree[size..size + destroyed.len()].copy_from_slice(&destroyed);
        for i in (1..size).rev() {
            tree[i] = tree[2 * i].max(tree[2 * i + 1]);
        }
        Self {
            order,
            created,
            tree,
            size,
        }
    }

    fn collect(
        &self,
        node: usize,
        lo: usize,
        hi: usize,
        limit: usize,
        t: f64,
        out: &mut Vec<usize>,
    ) {
        // Range [lo, hi) of the node; only positions < limit were created by t.
        if lo >= limit || self.tree[node] <= t {
            return;
        }
        if hi - lo == 1 {
            out.push(self.order[lo]);
            return;
        }
        let mid = (lo + hi) / 2;
        self.collect(2 * node, lo, mid, limit, t, out);
        self.collect(2 * node + 1, mid, hi, limit, t, out);
    }

    /// Items with `created <= t < destroyed`.
    fn alive(&self, t: f64, out: &mut Vec<usize>) {
        let limit = self.created.partition_point(|&c| c <= t);
        if limit > 0 {
            self.collect(1, 0, self.size, limit, t, out);
        }
    }
}

/// The replay index. See the module docs.
#[derive(Clone, Debug)]
pub struct Timeline {
    blocks: Vec<BlockTrack>,
    items: Vec<ItemTrack>,
    live: LiveIndex,
    /// Sorted distinct times of all trace records.
    event_times: Vec<f64>,
    n_records: usize,
    transit_default: f64,
    /// Per block: whether it is a mobile resource.
    mobile: Vec<bool>,
    /// Travel legs per mobile block, in time order.
    legs: HashMap<usize, Vec<TravelLeg>>,
    n_travels: usize,
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[values.len() / 2])
}

impl Timeline {
    pub fn build(replay: &Replay, setting: TransitSetting) -> Self {
        let n_blocks = replay.blocks.len();
        let mut blocks = vec![BlockTrack::default(); n_blocks];
        let mut items: Vec<ItemTrack> = Vec::new();
        let mut index: HashMap<u64, usize> = HashMap::new();
        // Current block of each item (None while packed or gone), for the counters.
        let mut at: Vec<Option<usize>> = Vec::new();
        let mut counts = vec![0usize; n_blocks];
        let mut event_times: Vec<f64> = Vec::new();
        let mobile: Vec<bool> = replay
            .blocks
            .iter()
            .map(|b| is_mobile_kind(&b.kind))
            .collect();
        let is_mobile = |b: usize| mobile.get(b).copied().unwrap_or(false);
        let mut legs: HashMap<usize, Vec<TravelLeg>> = HashMap::new();
        let mut n_travels = 0usize;

        let item_index = |key: u64,
                          index: &mut HashMap<u64, usize>,
                          items: &mut Vec<ItemTrack>,
                          at: &mut Vec<Option<usize>>| {
            *index.entry(key).or_insert_with(|| {
                items.push(ItemTrack {
                    steps: Vec::new(),
                    created: f64::INFINITY,
                    destroyed: f64::INFINITY,
                });
                at.push(None);
                items.len() - 1
            })
        };
        let bump =
            |blocks: &mut Vec<BlockTrack>, counts: &mut Vec<usize>, b: usize, t: f64, up: bool| {
                if b >= counts.len() {
                    return;
                }
                if up {
                    counts[b] += 1;
                } else {
                    counts[b] = counts[b].saturating_sub(1);
                }
                blocks[b].content.push((t, counts[b]));
            };

        for rec in &replay.records {
            let t = rec.t;
            if event_times.last() != Some(&t) {
                event_times.push(t);
            }
            match &rec.event {
                TraceEvent::Created { item, block } => {
                    let i = item_index(*item, &mut index, &mut items, &mut at);
                    items[i].created = items[i].created.min(t);
                    items[i].steps.push((t, ItemStep::Created(*block)));
                    at[i] = Some(*block);
                    bump(&mut blocks, &mut counts, *block, t, true);
                }
                TraceEvent::Moved { item, from, to } => {
                    let i = item_index(*item, &mut index, &mut items, &mut at);
                    if items[i].created.is_infinite() {
                        items[i].created = t;
                    }
                    items[i].steps.push((
                        t,
                        ItemStep::Moved {
                            from: *from,
                            to: *to,
                            transit: 0.0,
                        },
                    ));
                    bump(&mut blocks, &mut counts, *from, t, false);
                    bump(&mut blocks, &mut counts, *to, t, true);
                    at[i] = Some(*to);
                }
                TraceEvent::Packed { item, container } => {
                    let i = item_index(*item, &mut index, &mut items, &mut at);
                    let c = item_index(*container, &mut index, &mut items, &mut at);
                    items[i].steps.push((t, ItemStep::Packed(c)));
                    if let Some(b) = at[i].take() {
                        bump(&mut blocks, &mut counts, b, t, false);
                    }
                }
                TraceEvent::Unpacked { item, block } => {
                    let i = item_index(*item, &mut index, &mut items, &mut at);
                    items[i].steps.push((t, ItemStep::Unpacked(*block)));
                    at[i] = Some(*block);
                    bump(&mut blocks, &mut counts, *block, t, true);
                }
                TraceEvent::Destroyed { item } => {
                    let i = item_index(*item, &mut index, &mut items, &mut at);
                    items[i].steps.push((t, ItemStep::Destroyed));
                    items[i].destroyed = items[i].destroyed.min(t);
                    if let Some(b) = at[i].take() {
                        bump(&mut blocks, &mut counts, b, t, false);
                    }
                }
                TraceEvent::State { block, state } => {
                    if let Some(track) = blocks.get_mut(*block) {
                        track.states.push((t, *state));
                    }
                }
                TraceEvent::Travel {
                    carrier,
                    path,
                    duration,
                } => {
                    n_travels += 1;
                    if let Some(leg) = TravelLeg::new(t, *duration, path) {
                        legs.entry(*carrier).or_default().push(leg);
                    }
                }
            }
        }
        // Items only seen as containers (no own event) never exist visually.
        for it in &mut items {
            if it.created.is_infinite() {
                it.destroyed = f64::NEG_INFINITY;
            }
        }

        let span = event_times
            .last()
            .copied()
            .unwrap_or(0.0)
            .max(replay.stop_time.min(1e300));
        let span = if span.is_finite() { span } else { 0.0 };
        let (lo, hi) = ((span * 1e-4).max(1e-9), (span / 50.0).max(1e-9));

        // Global fallback: median positive gap over all items.
        let mut all_gaps: Vec<f64> = Vec::new();
        for it in &items {
            all_gaps.extend(positive_gaps(&it.steps));
        }
        let global_default = (0.4 * median(&mut all_gaps).unwrap_or(hi)).clamp(lo, hi);

        for it in &mut items {
            let base = match setting {
                TransitSetting::Fixed(v) => v.max(0.0),
                TransitSetting::Auto => {
                    let mut gaps = positive_gaps(&it.steps);
                    median(&mut gaps).map_or(global_default, |m| (0.4 * m).clamp(lo, hi))
                }
            };
            let times: Vec<f64> = it.steps.iter().map(|s| s.0).collect();
            for (k, step) in it.steps.iter_mut().enumerate() {
                if let ItemStep::Moved { from, to, transit } = &mut step.1 {
                    // Pick-ups and drop-offs by a carrier are not tweened (see module docs).
                    if is_mobile(*from) || is_mobile(*to) {
                        *transit = 0.0;
                        continue;
                    }
                    let gap = times
                        .get(k + 1)
                        .map_or(f64::INFINITY, |next| next - times[k]);
                    *transit = base.min(gap);
                }
            }
        }

        let live = LiveIndex::build(&items);
        let transit_default = match setting {
            TransitSetting::Fixed(v) => v,
            TransitSetting::Auto => global_default,
        };
        Timeline {
            blocks,
            items,
            live,
            event_times,
            n_records: replay.records.len(),
            transit_default,
            mobile,
            legs,
            n_travels,
        }
    }

    /// Whether block `b` is a mobile resource (transporter or dispatcher).
    pub fn is_mobile(&self, b: usize) -> bool {
        self.mobile.get(b).copied().unwrap_or(false)
    }

    /// Number of mobile resources (transporters and dispatchers).
    pub fn mobile_count(&self) -> usize {
        self.mobile.iter().filter(|m| **m).count()
    }

    /// Number of `Travel` events in the trace.
    pub fn travel_count(&self) -> usize {
        self.n_travels
    }

    /// Model-space position `(x, y, z)` of mobile block `b` at `t`, or `None` while it
    /// has not started any travel (then it rests at its layout position). Stationary at
    /// the last point of its latest leg between and after travels.
    pub fn carrier_pos(&self, b: usize, t: f64) -> Option<[f64; 3]> {
        let legs = self.legs.get(&b)?;
        let k = legs.partition_point(|l| l.t0 <= t).checked_sub(1)?;
        Some(legs[k].at(t))
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn record_count(&self) -> usize {
        self.n_records
    }

    /// Global default transit length (informational, for the UI).
    pub fn transit_default(&self) -> f64 {
        self.transit_default
    }

    /// Time of the last trace record (0 when empty).
    pub fn last_event_time(&self) -> f64 {
        self.event_times.last().copied().unwrap_or(0.0)
    }

    /// First event time strictly after `t`.
    pub fn next_event_time(&self, t: f64) -> Option<f64> {
        let k = self.event_times.partition_point(|&e| e <= t);
        self.event_times.get(k).copied()
    }

    /// Last event time strictly before `t`.
    pub fn prev_event_time(&self, t: f64) -> Option<f64> {
        let k = self.event_times.partition_point(|&e| e < t);
        k.checked_sub(1).map(|k| self.event_times[k])
    }

    /// State of block `b` at `t` (`None` before its first state change).
    pub fn block_state(&self, b: usize, t: f64) -> Option<State> {
        let states = &self.blocks.get(b)?.states;
        let k = states.partition_point(|s| s.0 <= t);
        k.checked_sub(1).map(|k| states[k].1)
    }

    /// Simulation-true content count of block `b` at `t`.
    pub fn block_content(&self, b: usize, t: f64) -> usize {
        let Some(track) = self.blocks.get(b) else {
            return 0;
        };
        let k = track.content.partition_point(|s| s.0 <= t);
        k.checked_sub(1).map_or(0, |k| track.content[k].1)
    }

    /// Creation block of an item (for coloring), if known.
    pub fn item_origin(&self, item: usize) -> Option<usize> {
        match self.items.get(item)?.steps.first()?.1 {
            ItemStep::Created(b) => Some(b),
            ItemStep::Moved { from, .. } => Some(from),
            _ => None,
        }
    }

    pub fn item_pos(&self, item: usize, t: f64) -> ItemPos {
        self.item_pos_since(item, t).0
    }

    /// Position of the item plus the time of the event that put it there
    /// (used to keep queue order: older items sit closer to the exit).
    pub fn item_pos_since(&self, item: usize, t: f64) -> (ItemPos, f64) {
        let Some(track) = self.items.get(item) else {
            return (ItemPos::NotYet, f64::INFINITY);
        };
        let k = track.steps.partition_point(|s| s.0 <= t);
        let Some(k) = k.checked_sub(1) else {
            return (ItemPos::NotYet, f64::INFINITY);
        };
        let (te, step) = track.steps[k];
        let pos = match step {
            ItemStep::Created(b) | ItemStep::Unpacked(b) => ItemPos::At(b),
            ItemStep::Moved { from, to, transit } => {
                if transit > 0.0 && t < te + transit {
                    ItemPos::Transit {
                        from,
                        to,
                        progress: ((t - te) / transit).clamp(0.0, 1.0),
                    }
                } else {
                    ItemPos::At(to)
                }
            }
            ItemStep::Packed(c) => ItemPos::Inside(c),
            ItemStep::Destroyed => ItemPos::Gone,
        };
        (pos, te)
    }

    /// Appends the dense indices of the items that exist at `t`.
    pub fn live_items(&self, t: f64, out: &mut Vec<usize>) {
        self.live.alive(t, out);
    }
}

/// Positive gaps between consecutive step times.
fn positive_gaps(steps: &[(f64, ItemStep)]) -> Vec<f64> {
    steps
        .windows(2)
        .map(|w| w[1].0 - w[0].0)
        .filter(|g| *g > 0.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_model::Model;

    const MODEL: &str = r#"Model(
        name: "t",
        run: (stop_time: 20.0, seed: 7, replications: 1, sample_interval: Some(0.37)),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1.0)), position: (0.0, 0.0, 0.0)),
            (name: "q", kind: Queue(), position: (3.0, 0.0, 0.0)),
            (name: "proc", kind: Processor(process: Const(1.5)), position: (6.0, 0.0, 0.0)),
            (name: "sink", kind: Sink, position: (9.0, 0.0, 0.0)),
        ],
        connections: [("src", "q"), ("q", "proc"), ("proc", "sink")],
    )"#;

    fn replay_of(text: &str) -> Replay {
        Model::parse(text).unwrap().replay(0).unwrap()
    }

    fn compare_series(rep: &Replay) {
        let tl = Timeline::build(rep, TransitSetting::Auto);
        let mut checked = 0;
        for (b, br) in rep.report.blocks.iter().enumerate() {
            assert!(!br.series.is_empty(), "{} has no series", br.name);
            for p in &br.series {
                assert_eq!(
                    tl.block_content(b, p.t),
                    p.content,
                    "content of {} at t={}",
                    br.name,
                    p.t
                );
                let state = tl.block_state(b, p.t).map_or("none", State::name);
                assert_eq!(state, p.state, "state of {} at t={}", br.name, p.t);
                checked += 1;
            }
        }
        assert!(checked > 50);
    }

    #[test]
    fn content_and_state_match_the_sampled_series() {
        compare_series(&replay_of(MODEL));
    }

    #[test]
    fn combiner_example_matches_the_sampled_series() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/montagem_paletes.ron"
        ))
        .unwrap();
        let mut model = Model::parse(&text).unwrap();
        model.run.stop_time = 300.0;
        model.run.warmup = 0.0;
        model.run.sample_interval = Some(0.37);
        compare_series(&model.replay(0).unwrap());
    }

    #[test]
    fn chosen_instants_of_the_deterministic_model() {
        let rep = replay_of(MODEL);
        let tl = Timeline::build(&rep, TransitSetting::Fixed(0.2));
        let (q, p, s) = (1, 2, 3);
        // Items are created at t = 1, 2, 3...; the processor takes 1.5 each.
        assert_eq!(tl.block_content(p, 0.5), 0);
        assert_eq!(tl.block_state(p, 0.5), Some(State::Idle));
        assert_eq!(tl.block_content(p, 1.5), 1);
        assert_eq!(tl.block_state(p, 1.5), Some(State::Processing));
        assert_eq!(tl.block_content(q, 1.5), 0);
        // t = 2.2: item 1 is still being processed (until 2.5), item 2 waits in the queue.
        assert_eq!(tl.block_content(q, 2.2), 1);
        assert_eq!(tl.block_content(p, 2.2), 1);
        // t = 2.6: item 2 took over the processor and the queue is empty again.
        assert_eq!(tl.block_content(q, 2.6), 0);
        assert_eq!(tl.block_content(p, 2.6), 1);
        // The sink destroys what arrives, so it never holds anything.
        assert_eq!(tl.block_content(s, 2.6), 0);
        assert_eq!(tl.block_content(q, 100.0), tl.block_content(q, 20.0));
    }

    #[test]
    fn transit_is_bounded_and_ends_at_destination() {
        let rep = replay_of(MODEL);
        let tl = Timeline::build(&rep, TransitSetting::Fixed(0.4));
        // Find the first item that sits in the queue and later moves to the processor.
        let mut seen = false;
        for i in 0..tl.item_count() {
            let track = &tl.items[i];
            for (k, (te, step)) in track.steps.iter().enumerate() {
                let ItemStep::Moved { from, to, transit } = step else {
                    continue;
                };
                if let Some(next) = track.steps.get(k + 1) {
                    assert!(
                        te + transit <= next.0 + 1e-12,
                        "transit overlaps next event"
                    );
                }
                if *transit > 0.0 {
                    match tl.item_pos(i, te + transit * 0.5) {
                        ItemPos::Transit {
                            from: f,
                            to: d,
                            progress,
                        } => {
                            assert_eq!((f, d), (*from, *to));
                            assert!((progress - 0.5).abs() < 1e-9);
                        }
                        other => panic!("expected a transit, got {other:?}"),
                    }
                    assert_eq!(tl.item_pos(i, te + transit), ItemPos::At(*to));
                    seen = true;
                }
            }
        }
        assert!(seen);
    }

    #[test]
    fn auto_transit_is_positive_and_clamped() {
        let rep = replay_of(MODEL);
        let tl = Timeline::build(&rep, TransitSetting::Auto);
        let span = tl.last_event_time();
        assert!(tl.transit_default() >= span * 1e-4 - 1e-12);
        assert!(tl.transit_default() <= span / 50.0 + 1e-12);
    }

    #[test]
    fn live_items_match_brute_force_everywhere() {
        let rep = replay_of(MODEL);
        let tl = Timeline::build(&rep, TransitSetting::Auto);
        let mut t = 0.0;
        while t < 21.0 {
            let mut fast = Vec::new();
            tl.live_items(t, &mut fast);
            fast.sort_unstable();
            let slow: Vec<usize> = (0..tl.item_count())
                .filter(|&i| !matches!(tl.item_pos(i, t), ItemPos::NotYet | ItemPos::Gone))
                .collect();
            assert_eq!(fast, slow, "at t={t}");
            t += 0.173;
        }
    }

    #[test]
    fn event_navigation() {
        let rep = replay_of(MODEL);
        let tl = Timeline::build(&rep, TransitSetting::Auto);
        assert_eq!(tl.next_event_time(-1.0), Some(0.0));
        let n = tl.next_event_time(0.0).unwrap();
        assert_eq!(tl.prev_event_time(n), Some(0.0));
        assert_eq!(tl.next_event_time(1e9), None);
        assert_eq!(tl.prev_event_time(0.0), None);
    }

    #[test]
    fn packed_items_are_inside_their_container() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/montagem_paletes.ron"
        ))
        .unwrap();
        let mut model = Model::parse(&text).unwrap();
        model.run.stop_time = 100.0;
        model.run.warmup = 0.0;
        let rep = model.replay(0).unwrap();
        let tl = Timeline::build(&rep, TransitSetting::Auto);
        let packed = rep
            .records
            .iter()
            .find_map(|r| match r.event {
                TraceEvent::Packed { .. } => Some(r.t),
                _ => None,
            })
            .expect("the example packs items");
        let mut inside = 0;
        let mut live = Vec::new();
        tl.live_items(packed, &mut live);
        for i in live {
            if let ItemPos::Inside(c) = tl.item_pos(i, packed) {
                assert_ne!(c, i);
                inside += 1;
            }
        }
        assert!(inside > 0);
    }

    const CARRIER_MODEL: &str = r#"Model(
        name: "t",
        run: (stop_time: 40.0, seed: 1, replications: 1),
        objects: [
            (name: "src", kind: Source(interarrival: Const(5.0), first_at_zero: true, max_arrivals: 2),
                position: (0.0, 0.0, 0.0), transport: Some("emp")),
            (name: "proc", kind: Processor(process: Const(1.0)), position: (10.0, 0.0, 0.0)),
            (name: "sink", kind: Sink, position: (20.0, 0.0, 0.0)),
            (name: "emp", kind: Transporter(speed: 2.0), position: (0.0, 4.0, 0.0)),
        ],
        connections: [("src", "proc"), ("proc", "sink")],
    )"#;

    #[test]
    fn carrier_moves_along_its_travels_and_carries_items() {
        let rep = replay_of(CARRIER_MODEL);
        let tl = Timeline::build(&rep, TransitSetting::Fixed(0.3));
        let emp = rep.blocks.iter().position(|b| b.name == "emp").unwrap();
        assert!(tl.is_mobile(emp));
        assert_eq!(tl.mobile_count(), 1);
        assert!(tl.travel_count() > 0);
        // Before the first travel it has no trace position.
        assert_eq!(tl.carrier_pos(emp, -1.0), None);
        let (t0, path, duration) = rep
            .records
            .iter()
            .find_map(|r| match &r.event {
                TraceEvent::Travel {
                    carrier,
                    path,
                    duration,
                } if *carrier == emp && *duration > 0.0 => Some((r.t, path.clone(), *duration)),
                _ => None,
            })
            .unwrap();
        let start = tl.carrier_pos(emp, t0).unwrap();
        assert_eq!(start, path[0]);
        let end = tl.carrier_pos(emp, t0 + duration).unwrap();
        assert_eq!(end, *path.last().unwrap());
        // Items moved into/out of the carrier have no visual transit.
        let mut checked = false;
        for r in &rep.records {
            if let TraceEvent::Moved { to, from, .. } = r.event
                && (to == emp || from == emp)
            {
                for i in 0..tl.item_count() {
                    let p = tl.item_pos(i, r.t + 1e-6);
                    assert!(
                        !matches!(p, ItemPos::Transit { from: f, to: d, .. } if f == emp || d == emp),
                        "{p:?}"
                    );
                }
                checked = true;
            }
        }
        assert!(checked, "the example must move items through the carrier");
        // Some item sits inside the carrier at some instant.
        let mut inside = false;
        let mut t = 0.0;
        while t < 40.0 {
            let mut live = Vec::new();
            tl.live_items(t, &mut live);
            inside |= live.iter().any(|&i| tl.item_pos(i, t) == ItemPos::At(emp));
            t += 0.05;
        }
        assert!(inside);
    }

    #[test]
    fn travel_leg_interpolates_by_arc_length() {
        let leg = TravelLeg::new(
            10.0,
            4.0,
            &[[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 1.0, 0.0]],
        )
        .unwrap();
        assert_eq!(leg.at(10.0), [0.0, 0.0, 0.0]);
        assert_eq!(leg.at(13.0), [3.0, 0.0, 0.0]);
        assert_eq!(leg.at(11.0), [1.0, 0.0, 0.0]);
        assert_eq!(leg.at(100.0), [3.0, 1.0, 0.0]);
        assert!(TravelLeg::new(0.0, 1.0, &[]).is_none());
        let still = TravelLeg::new(0.0, 0.0, &[[1.0, 2.0, 3.0]]).unwrap();
        assert_eq!(still.at(5.0), [1.0, 2.0, 3.0]);
    }
}
