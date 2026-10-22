//! The simulation kernel: clock, future event list, item arena and flow resolution.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashSet, VecDeque};
use std::sync::Arc;

use rand::Rng;
use slotmap::SlotMap;

use crate::Time;
use crate::block::{Behavior, BlockId, Downtime, OutputRule, ShiftTable, State};
use crate::dist::Dist;
use crate::item::{Item, ItemId, Value};
use crate::report::{BlockReport, RunReport, SeriesPoint, TallySummary};
use crate::rng::{self, SimRng};
use crate::stats::{StateTracker, Tally, TimeWeighted};
use crate::task::{MSG_TRANSPORT_RESERVED, RequestId, Router, StraightLine, TaskKind, TaskRequest};
use crate::trace::{Observer, TraceEvent, TraceRecord, item_key};

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub stop_time: Time,
    /// Statistics are cleared at this time (0 = no warm-up).
    pub warmup: Time,
    pub seed: u64,
    pub replication: u32,
    /// Sample every block's content and state at this interval, from the warm-up time on.
    pub sample_interval: Option<Time>,
    /// Keep raw observations and build histograms with this many bins.
    pub histogram_bins: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("erro de modelo em t={time}: {}", messages.join("; "))]
    Model { time: Time, messages: Vec<String> },
}

/// Handle to a scheduled event, used to cancel it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventId(u64);

#[derive(Clone, Copy, Debug)]
enum EventKind {
    Block {
        block: BlockId,
        tag: u32,
        item: Option<ItemId>,
    },
    Fail(BlockId),
    Repair(BlockId),
    /// Start of off-shift window `k` of the block's shift table.
    ShiftOff(BlockId, usize),
    ShiftOn(BlockId),
    EndWarmup,
    /// Records the time series; internal, not counted in `events_processed`.
    Sample,
}

struct Scheduled {
    time: Time,
    seq: u64,
    kind: EventKind,
}

// Min-heap on (time, seq): simultaneous events run in the order they were scheduled.
impl Ord for Scheduled {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .time
            .total_cmp(&self.time)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for Scheduled {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Scheduled {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Scheduled {}

/// A released item waiting to leave, with the port chosen at release time.
#[derive(Clone, Copy, Debug)]
struct Pending {
    item: ItemId,
    port: Option<usize>,
}

#[derive(Default)]
struct BlockStats {
    input: u64,
    output: u64,
    content: TimeWeighted,
    staytime: Tally,
    states: StateTracker,
    extra: BTreeMap<String, Tally>,
}

impl BlockStats {
    fn reset(&mut self, t: Time) {
        self.input = 0;
        self.output = 0;
        self.content.reset(t);
        self.staytime = self.staytime.cleared();
        self.states.reset(t);
        self.extra
            .values_mut()
            .for_each(|tally| *tally = tally.cleared());
    }
}

struct BlockRuntime {
    name: String,
    kind: &'static str,
    inputs: Vec<BlockId>,
    /// Downstream block and the input port number on that block.
    outputs: Vec<(BlockId, usize)>,
    rule: OutputRule,
    downtime: Option<Downtime>,
    /// Separate stream so enabling breakdowns does not shift the block's own draws.
    downtime_rng: SimRng,
    shifts: Option<ShiftTable>,
    /// Off-shift windows currently open. A counter, not a flag: when one window ends
    /// at the instant the next begins, both events share the same time.
    open_windows: u32,
    /// Active stop reasons (bit set of `BREAKDOWN`, `OFF_SHIFT`); 0 = running.
    stops: u8,
    /// State the block asked for while stopped; restored when it runs again.
    state_when_up: Option<State>,
    contents: Vec<ItemId>,
    outbox: VecDeque<Pending>,
    stats: BlockStats,
    series: Vec<SeriesPoint>,
    rng: SimRng,
    rr_next: usize,
    dirty: bool,
    /// Layout position, used for travel distances.
    position: [f64; 3],
    /// Mobile resource (or dispatcher) that carries this block's items out.
    transport: Option<BlockId>,
    /// Items on their way here (room reserved by a transport request).
    incoming: usize,
}

/// Work that involves another block's hooks, run by the kernel after the current
/// hook returns (a hook can only reach its own block).
enum KernelOp {
    Task {
        to: BlockId,
        request: TaskRequest,
    },
    Message {
        to: BlockId,
        from: BlockId,
        tag: u32,
        payload: u64,
    },
    PickUp {
        carrier: BlockId,
        item: ItemId,
    },
    DropOff {
        carrier: BlockId,
        item: ItemId,
        to: BlockId,
        port: usize,
    },
}

/// Kernel state that blocks can reach through [`Ctx`].
pub struct Core {
    now: Time,
    seq: u64,
    events: BinaryHeap<Scheduled>,
    items: SlotMap<ItemId, Item>,
    blocks: Vec<BlockRuntime>,
    /// Blocks whose outbox may be able to move.
    dirty: VecDeque<BlockId>,
    /// Items destroyed during the current callback.
    doomed: Vec<ItemId>,
    errors: Vec<String>,
    events_processed: u64,
    /// Sequence numbers of cancelled events still sitting in the heap.
    cancelled: HashSet<u64>,
    /// Whether tallies keep raw observations (histograms on).
    keep_raw: bool,
    /// Number of samples taken so far; sample `k` happens at `warmup + k * interval`.
    samples_taken: u64,
    /// Recorded trace, when enabled with [`Simulation::enable_trace`].
    trace: Option<Vec<TraceRecord>>,
    observer: Option<Box<dyn Observer>>,
    ops: VecDeque<KernelOp>,
    next_request: u64,
    router: Arc<dyn Router>,
    /// Origin of the move in progress, between `leave` and `arrive` (for the trace).
    pending_move_from: Option<BlockId>,
}

impl Core {
    fn schedule_at(&mut self, time: Time, kind: EventKind) -> EventId {
        self.seq += 1;
        self.events.push(Scheduled {
            time,
            seq: self.seq,
            kind,
        });
        EventId(self.seq)
    }

    fn take_item_from(&mut self, b: BlockId, item: ItemId) {
        let rt = &mut self.blocks[b.0];
        if let Some(pos) = rt.contents.iter().position(|&x| x == item) {
            rt.contents.remove(pos);
        }
        rt.outbox.retain(|p| p.item != item);
        self.update_content(b);
    }

    fn put_item_into(&mut self, b: BlockId, item: ItemId) {
        let now = self.now;
        let it = &mut self.items[item];
        it.location = Some(b);
        it.container = None;
        it.entered_at = now;
        self.blocks[b.0].contents.push(item);
        self.update_content(b);
    }

    fn emit(&mut self, event: TraceEvent) {
        if let Some(observer) = &mut self.observer {
            observer.on_event(self.now, &event);
        }
        if let Some(trace) = &mut self.trace {
            trace.push(TraceRecord { t: self.now, event });
        }
    }

    fn set_block_state(&mut self, b: BlockId, state: State) {
        self.blocks[b.0].stats.states.set(self.now, state);
        self.emit(TraceEvent::State { block: b.0, state });
    }

    fn mark_dirty(&mut self, b: BlockId) {
        let rt = &mut self.blocks[b.0];
        if !rt.dirty {
            rt.dirty = true;
            self.dirty.push_back(b);
        }
    }

    fn update_content(&mut self, b: BlockId) {
        let rt = &mut self.blocks[b.0];
        rt.stats.content.set(self.now, rt.contents.len() as f64);
    }
}

/// What a block sees of the kernel while one of its hooks runs.
pub struct Ctx<'a> {
    core: &'a mut Core,
    me: BlockId,
}

impl Ctx<'_> {
    pub fn now(&self) -> Time {
        self.core.now
    }

    pub fn me(&self) -> BlockId {
        self.me
    }

    pub fn name(&self) -> &str {
        &self.core.blocks[self.me.0].name
    }

    /// Schedules [`Behavior::on_event`] on this block after `delay`.
    pub fn schedule(&mut self, delay: Time, tag: u32, item: Option<ItemId>) -> EventId {
        let time = self.core.now + delay.max(0.0);
        self.core.schedule_at(
            time,
            EventKind::Block {
                block: self.me,
                tag,
                item,
            },
        )
    }

    /// Cancels an event scheduled earlier. Cancelling a fired event is a no-op.
    pub fn cancel(&mut self, id: EventId) {
        if id.0 <= self.core.seq {
            self.core.cancelled.insert(id.0);
        }
    }

    /// Whether this block is currently stopped (breakdown or off shift).
    pub fn is_down(&self) -> bool {
        self.core.blocks[self.me.0].stops != 0
    }

    pub fn input_count(&self) -> usize {
        self.core.blocks[self.me.0].inputs.len()
    }

    pub fn output_count(&self) -> usize {
        self.core.blocks[self.me.0].outputs.len()
    }

    pub fn rng(&mut self) -> &mut SimRng {
        &mut self.core.blocks[self.me.0].rng
    }

    pub fn sample(&mut self, dist: &Dist) -> f64 {
        dist.sample(self.rng())
    }

    /// Samples a duration, clamping negative draws (e.g. from a Normal) to zero.
    pub fn sample_duration(&mut self, dist: &Dist) -> Time {
        self.sample(dist).max(0.0)
    }

    /// Sets the block state. While the block is down the kernel keeps showing
    /// `down` and applies this state when the repair finishes.
    pub fn set_state(&mut self, state: State) {
        let rt = &mut self.core.blocks[self.me.0];
        if rt.stops != 0 {
            rt.state_when_up = Some(state);
        } else {
            self.core.set_block_state(self.me, state);
        }
    }

    pub fn content_len(&self) -> usize {
        self.core.blocks[self.me.0].contents.len()
    }

    pub fn contents(&self) -> &[ItemId] {
        &self.core.blocks[self.me.0].contents
    }

    pub fn item(&self, id: ItemId) -> &Item {
        &self.core.items[id]
    }

    pub fn item_mut(&mut self, id: ItemId) -> &mut Item {
        &mut self.core.items[id]
    }

    /// Creates a new item inside this block.
    pub fn spawn_item(&mut self) -> ItemId {
        let now = self.core.now;
        let id = self.core.items.insert(Item {
            labels: BTreeMap::new(),
            created_at: now,
            entered_at: now,
            location: Some(self.me),
            container: None,
            contents: Vec::new(),
            awaiting_transport: false,
        });
        self.core.blocks[self.me.0].contents.push(id);
        self.core.update_content(self.me);
        let block = self.me.0;
        self.core.emit(TraceEvent::Created {
            item: item_key(id),
            block,
        });
        id
    }

    /// Creates a copy of `id` (labels and creation time, not contents) inside this block.
    pub fn clone_item(&mut self, id: ItemId) -> ItemId {
        let (labels, created_at) = {
            let src = &self.core.items[id];
            (src.labels.clone(), src.created_at)
        };
        let copy = self.spawn_item();
        let it = &mut self.core.items[copy];
        it.labels = labels;
        it.created_at = created_at;
        copy
    }

    /// Removes the item (and anything packed in it) once the current hook returns.
    pub fn destroy(&mut self, id: ItemId) {
        self.core.doomed.push(id);
    }

    /// Moves `item`, which must be in this block, into `container`.
    /// The packed item no longer counts as block content.
    pub fn pack(&mut self, container: ItemId, item: ItemId) {
        if self.core.items[item].location != Some(self.me) || container == item {
            self.error("pack: o item não está neste objeto");
            return;
        }
        self.core.take_item_from(self.me, item);
        let it = &mut self.core.items[item];
        it.location = None;
        it.container = Some(container);
        self.core.items[container].contents.push(item);
        self.core.emit(TraceEvent::Packed {
            item: item_key(item),
            container: item_key(container),
        });
    }

    /// Takes everything out of `container` and places it in this block.
    pub fn unpack(&mut self, container: ItemId) -> Vec<ItemId> {
        let items = std::mem::take(&mut self.core.items[container].contents);
        for &item in &items {
            self.core.put_item_into(self.me, item);
            let block = self.me.0;
            self.core.emit(TraceEvent::Unpacked {
                item: item_key(item),
                block,
            });
        }
        items
    }

    /// Marks the item as ready to leave, behind items released earlier.
    pub fn release(&mut self, id: ItemId) {
        let pending = self.route_output(id);
        self.core.blocks[self.me.0].outbox.push_back(pending);
        self.core.mark_dirty(self.me);
    }

    /// Marks the item as ready to leave, ahead of items released earlier.
    pub fn release_front(&mut self, id: ItemId) {
        let pending = self.route_output(id);
        self.core.blocks[self.me.0].outbox.push_front(pending);
        self.core.mark_dirty(self.me);
    }

    /// Adds an observation to a named per-block tally (shown in the report).
    pub fn record(&mut self, name: &str, value: f64) {
        let keep_raw = self.core.keep_raw;
        let extra = &mut self.core.blocks[self.me.0].stats.extra;
        extra
            .entry(name.to_string())
            .or_insert_with(|| {
                if keep_raw {
                    Tally::with_raw()
                } else {
                    Tally::default()
                }
            })
            .record(value);
    }

    /// Reports a modelling error; the run stops after the current event.
    pub fn error(&mut self, message: impl Into<String>) {
        let message = format!("{}: {}", self.name(), message.into());
        self.core.errors.push(message);
    }

    /// Layout position of any block.
    pub fn position(&self, block: BlockId) -> [f64; 3] {
        self.core.blocks[block.0].position
    }

    pub fn block_name(&self, block: BlockId) -> &str {
        &self.core.blocks[block.0].name
    }

    /// Path between two points (straight line unless a path network is set).
    pub fn route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]> {
        self.core.router.route(from, to)
    }

    /// Records that this block starts moving along `path` for `duration` (for the 3D view).
    pub fn trace_travel(&mut self, path: Vec<[f64; 3]>, duration: Time) {
        let carrier = self.me.0;
        self.core.emit(TraceEvent::Travel {
            carrier,
            path,
            duration,
        });
    }

    /// Asks block `to` (a mobile resource or dispatcher) to do some work for this block.
    pub fn request_task(&mut self, to: BlockId, kind: TaskKind, priority: i32) -> RequestId {
        self.core.next_request += 1;
        let id = RequestId(self.core.next_request);
        let request = TaskRequest {
            id,
            kind,
            requester: self.me,
            priority,
        };
        self.core.ops.push_back(KernelOp::Task { to, request });
        id
    }

    /// Passes a task on unchanged (dispatchers).
    pub fn forward_task(&mut self, to: BlockId, request: TaskRequest) {
        self.core.ops.push_back(KernelOp::Task { to, request });
    }

    /// Sends a message to another block, delivered right after the current hook.
    pub fn send(&mut self, to: BlockId, tag: u32, payload: u64) {
        let from = self.me;
        self.core.ops.push_back(KernelOp::Message {
            to,
            from,
            tag,
            payload,
        });
    }

    /// Takes `item` (awaiting transport, anywhere) into this block, as a carrier.
    pub fn pick_up(&mut self, item: ItemId) {
        let carrier = self.me;
        self.core.ops.push_back(KernelOp::PickUp { carrier, item });
    }

    /// Delivers a carried `item` into input `port` of `to`, using the room reserved for it.
    pub fn drop_off(&mut self, item: ItemId, to: BlockId, port: usize) {
        let carrier = self.me;
        self.core.ops.push_back(KernelOp::DropOff {
            carrier,
            item,
            to,
            port,
        });
    }

    fn route_output(&mut self, id: ItemId) -> Pending {
        let core = &mut *self.core;
        let rt = &mut core.blocks[self.me.0];
        let n = rt.outputs.len();
        let port = match &rt.rule {
            OutputRule::FirstAvailable => None,
            _ if n == 0 => None,
            OutputRule::RoundRobin => {
                let port = rt.rr_next % n;
                rt.rr_next = (port + 1) % n;
                Some(port)
            }
            OutputRule::Random => Some(rt.rng.random_range(0..n)),
            OutputRule::Weighted(weights) => {
                let total: f64 = weights.iter().take(n).sum();
                let mut x = rt.rng.random::<f64>() * total;
                let mut port = n - 1;
                for (k, w) in weights.iter().take(n).enumerate() {
                    if x < *w {
                        port = k;
                        break;
                    }
                    x -= w;
                }
                Some(port)
            }
            OutputRule::ByLabel(label) => {
                match core.items[id].label(label).and_then(Value::as_num) {
                    Some(v) if v.fract() == 0.0 && v >= 1.0 && v as usize <= n => {
                        Some(v as usize - 1)
                    }
                    other => {
                        let message = format!(
                            "{}: rótulo '{label}' = {other:?} não é uma porta de saída válida (1..={n})",
                            rt.name
                        );
                        core.errors.push(message);
                        None
                    }
                }
            }
        };
        Pending { item: id, port }
    }
}

/// Offset of the breakdown random streams, far above any block index.
const DOWNTIME_STREAMS: u64 = 1 << 40;

/// Stop reasons. A block runs only when no reason is active.
const BREAKDOWN: u8 = 1;
const OFF_SHIFT: u8 = 2;

/// A runnable model instance: one replication.
pub struct Simulation {
    core: Core,
    behaviors: Vec<Box<dyn Behavior>>,
    config: RunConfig,
}

impl Simulation {
    pub fn new(config: RunConfig) -> Self {
        Simulation {
            core: Core {
                now: 0.0,
                seq: 0,
                events: BinaryHeap::new(),
                items: SlotMap::with_key(),
                blocks: Vec::new(),
                dirty: VecDeque::new(),
                doomed: Vec::new(),
                errors: Vec::new(),
                events_processed: 0,
                cancelled: HashSet::new(),
                keep_raw: config.histogram_bins.is_some_and(|b| b > 0),
                samples_taken: 0,
                trace: None,
                observer: None,
                ops: VecDeque::new(),
                next_request: 0,
                router: Arc::new(StraightLine),
                pending_move_from: None,
            },
            behaviors: Vec::new(),
            config,
        }
    }

    pub fn add_block(
        &mut self,
        name: impl Into<String>,
        rule: OutputRule,
        behavior: Box<dyn Behavior>,
    ) -> BlockId {
        let id = BlockId(self.behaviors.len());
        self.core.blocks.push(BlockRuntime {
            name: name.into(),
            kind: behavior.kind(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            rule,
            downtime: None,
            downtime_rng: rng::stream(
                self.config.seed,
                self.config.replication,
                DOWNTIME_STREAMS + id.0 as u64,
            ),
            shifts: None,
            open_windows: 0,
            stops: 0,
            state_when_up: None,
            contents: Vec::new(),
            outbox: VecDeque::new(),
            stats: if self.core.keep_raw {
                BlockStats {
                    staytime: Tally::with_raw(),
                    ..BlockStats::default()
                }
            } else {
                BlockStats::default()
            },
            series: Vec::new(),
            rng: rng::stream(self.config.seed, self.config.replication, id.0 as u64),
            rr_next: 0,
            dirty: false,
            position: [0.0; 3],
            transport: None,
            incoming: 0,
        });
        self.behaviors.push(behavior);
        id
    }

    /// Connects an output port of `from` to an input port of `to`.
    /// Port numbers follow connection order, which is also the pull priority.
    pub fn connect(&mut self, from: BlockId, to: BlockId) {
        let in_port = self.core.blocks[to.0].inputs.len();
        self.core.blocks[from.0].outputs.push((to, in_port));
        self.core.blocks[to.0].inputs.push(from);
    }

    /// Enables random breakdowns on a block.
    pub fn set_downtime(&mut self, block: BlockId, downtime: Downtime) {
        self.core.blocks[block.0].downtime = Some(downtime);
    }

    pub fn set_position(&mut self, block: BlockId, position: [f64; 3]) {
        self.core.blocks[block.0].position = position;
    }

    /// Items leaving `block` are carried by `carrier` (a mobile resource or dispatcher).
    pub fn set_transport(&mut self, block: BlockId, carrier: BlockId) {
        self.core.blocks[block.0].transport = Some(carrier);
    }

    /// Replaces the straight-line router (e.g. by a path network).
    pub fn set_router(&mut self, router: Arc<dyn Router>) {
        self.core.router = router;
    }

    /// Enables a repeating shift table (scheduled off-shift windows) on a block.
    pub fn set_shifts(&mut self, block: BlockId, shifts: ShiftTable) {
        self.core.blocks[block.0].shifts = Some(shifts);
    }

    pub fn block_id(&self, name: &str) -> Option<BlockId> {
        self.core
            .blocks
            .iter()
            .position(|b| b.name == name)
            .map(BlockId)
    }

    /// Records every [`TraceEvent`] of the run; read it with [`Simulation::take_trace`].
    pub fn enable_trace(&mut self) {
        self.core.trace.get_or_insert_with(Vec::new);
    }

    pub fn take_trace(&mut self) -> Vec<TraceRecord> {
        self.core.trace.take().unwrap_or_default()
    }

    /// Streams every [`TraceEvent`] to `observer` while the run progresses.
    pub fn set_observer(&mut self, observer: Box<dyn Observer>) {
        self.core.observer = Some(observer);
    }

    pub fn now(&self) -> Time {
        self.core.now
    }

    pub fn run(&mut self) -> Result<RunReport, SimError> {
        self.start()?;
        let stop = self.config.stop_time;
        while self.core.events.peek().is_some_and(|ev| ev.time <= stop) {
            let ev = self.core.events.pop().expect("peeked");
            if self.core.cancelled.remove(&ev.seq) {
                continue;
            }
            self.core.now = ev.time;
            if !matches!(ev.kind, EventKind::Sample) {
                self.core.events_processed += 1;
            }
            self.dispatch(ev.kind);
            self.resolve();
            self.check()?;
        }
        if stop.is_finite() {
            self.core.now = stop;
        }
        Ok(self.report())
    }

    fn start(&mut self) -> Result<(), SimError> {
        for b in 0..self.behaviors.len() {
            let me = BlockId(b);
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[b].reset(&mut Ctx { core, me });
            self.flush_doomed();
        }
        if self.config.warmup > 0.0 {
            self.core
                .schedule_at(self.config.warmup, EventKind::EndWarmup);
        }
        for b in 0..self.core.blocks.len() {
            self.schedule_failure(BlockId(b));
            self.schedule_shifts(BlockId(b));
        }
        if self.sample_interval().is_some() {
            self.core.schedule_at(self.config.warmup, EventKind::Sample);
        }
        self.resolve();
        self.check()
    }

    fn sample_interval(&self) -> Option<Time> {
        self.config
            .sample_interval
            .filter(|dt| dt.is_finite() && *dt > 0.0)
    }

    /// Records every block's content and state, then schedules the next sample.
    fn sample(&mut self) {
        let now = self.core.now;
        for rt in &mut self.core.blocks {
            let state = rt.stats.states.current().map_or("none", State::name);
            rt.series.push(SeriesPoint {
                t: now,
                content: rt.contents.len(),
                state: state.to_string(),
            });
        }
        self.core.samples_taken += 1;
        if let Some(dt) = self.sample_interval() {
            let next = self.config.warmup + self.core.samples_taken as f64 * dt;
            self.core.schedule_at(next, EventKind::Sample);
        }
    }

    fn schedule_failure(&mut self, b: BlockId) {
        let rt = &mut self.core.blocks[b.0];
        if let Some(dt) = &rt.downtime {
            let delay = dt.mtbf.sample(&mut rt.downtime_rng).max(0.0);
            let time = self.core.now + delay;
            self.core.schedule_at(time, EventKind::Fail(b));
        }
    }

    fn fail(&mut self, b: BlockId) {
        let now = self.core.now;
        let rt = &mut self.core.blocks[b.0];
        let repair = rt
            .downtime
            .as_ref()
            .map_or(0.0, |dt| dt.mttr.sample(&mut rt.downtime_rng).max(0.0));
        self.core.schedule_at(now + repair, EventKind::Repair(b));
        self.stop(b, BREAKDOWN);
    }

    fn repair(&mut self, b: BlockId) {
        self.resume(b, BREAKDOWN);
        self.schedule_failure(b);
    }

    fn schedule_shifts(&mut self, b: BlockId) {
        let Some(table) = &self.core.blocks[b.0].shifts else {
            return;
        };
        let starts: Vec<Time> = table.off.iter().map(|w| w.start).collect();
        for (k, start) in starts.into_iter().enumerate() {
            self.core.schedule_at(start, EventKind::ShiftOff(b, k));
        }
    }

    fn shift_off(&mut self, b: BlockId, k: usize) {
        let now = self.core.now;
        let Some(table) = &self.core.blocks[b.0].shifts else {
            return;
        };
        let (period, duration) = (table.period, table.off[k].duration);
        self.core.schedule_at(now + duration, EventKind::ShiftOn(b));
        self.core
            .schedule_at(now + period, EventKind::ShiftOff(b, k));
        self.core.blocks[b.0].open_windows += 1;
        self.stop(b, OFF_SHIFT);
    }

    fn shift_on(&mut self, b: BlockId) {
        let rt = &mut self.core.blocks[b.0];
        rt.open_windows = rt.open_windows.saturating_sub(1);
        if rt.open_windows == 0 {
            self.resume(b, OFF_SHIFT);
        }
    }

    /// State shown while stopped: a breakdown wins over being off shift.
    fn stopped_state(stops: u8) -> State {
        if stops & BREAKDOWN != 0 {
            State::Down
        } else {
            State::OffShift
        }
    }

    /// Adds a stop reason. Ports close and `on_stop` runs only on the first one.
    fn stop(&mut self, b: BlockId, reason: u8) {
        let rt = &mut self.core.blocks[b.0];
        let was_running = rt.stops == 0;
        if was_running {
            rt.state_when_up = rt.stats.states.current();
        }
        rt.stops |= reason;
        let shown = Self::stopped_state(rt.stops);
        self.core.set_block_state(b, shown);
        if was_running {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[b.0].on_stop(&mut Ctx { core, me: b });
            self.flush_doomed();
        }
    }

    /// Clears a stop reason; the block runs again once no reason is left.
    fn resume(&mut self, b: BlockId, reason: u8) {
        let rt = &mut self.core.blocks[b.0];
        rt.stops &= !reason;
        if rt.stops != 0 {
            let shown = Self::stopped_state(rt.stops);
            self.core.set_block_state(b, shown);
            return;
        }
        if let Some(state) = rt.state_when_up.take() {
            self.core.set_block_state(b, state);
        }
        {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[b.0].on_resume(&mut Ctx { core, me: b });
        }
        self.flush_doomed();
        // Both sides may move again.
        self.core.mark_dirty(b);
        for k in 0..self.core.blocks[b.0].inputs.len() {
            let up = self.core.blocks[b.0].inputs[k];
            self.core.mark_dirty(up);
        }
    }

    fn check(&mut self) -> Result<(), SimError> {
        if self.core.errors.is_empty() {
            Ok(())
        } else {
            Err(SimError::Model {
                time: self.core.now,
                messages: std::mem::take(&mut self.core.errors),
            })
        }
    }

    fn dispatch(&mut self, kind: EventKind) {
        match kind {
            EventKind::Block { block, tag, item } => {
                if item.is_some_and(|id| !self.core.items.contains_key(id)) {
                    return;
                }
                let Simulation {
                    core, behaviors, ..
                } = self;
                behaviors[block.0].on_event(&mut Ctx { core, me: block }, tag, item);
                self.flush_doomed();
            }
            EventKind::Fail(b) => self.fail(b),
            EventKind::Repair(b) => self.repair(b),
            EventKind::ShiftOff(b, k) => self.shift_off(b, k),
            EventKind::ShiftOn(b) => self.shift_on(b),
            EventKind::Sample => self.sample(),
            EventKind::EndWarmup => {
                let now = self.core.now;
                self.core
                    .blocks
                    .iter_mut()
                    .for_each(|rt| rt.stats.reset(now));
            }
        }
    }

    /// Moves released items until nothing else can move at the current instant.
    fn resolve(&mut self) {
        loop {
            if let Some(op) = self.core.ops.pop_front() {
                self.run_op(op);
            } else if let Some(b) = self.core.dirty.pop_front() {
                self.core.blocks[b.0].dirty = false;
                self.try_send(b);
            } else {
                break;
            }
        }
    }

    fn run_op(&mut self, op: KernelOp) {
        match op {
            KernelOp::Task { to, request } => {
                let Simulation {
                    core, behaviors, ..
                } = self;
                behaviors[to.0].on_task(&mut Ctx { core, me: to }, request);
            }
            KernelOp::Message {
                to,
                from,
                tag,
                payload,
            } => {
                let Simulation {
                    core, behaviors, ..
                } = self;
                behaviors[to.0].on_message(&mut Ctx { core, me: to }, from, tag, payload);
            }
            KernelOp::PickUp { carrier, item } => {
                let from = self.core.items.get(item).and_then(|it| it.location);
                match from {
                    Some(from) if self.core.items[item].awaiting_transport => {
                        self.core.items[item].awaiting_transport = false;
                        self.leave(item, from);
                        self.arrive(item, carrier, 0);
                    }
                    _ => {
                        let name = self.core.blocks[carrier.0].name.clone();
                        self.core.errors.push(format!(
                            "{name}: pick_up de um item que não aguarda transporte"
                        ));
                    }
                }
            }
            KernelOp::DropOff {
                carrier,
                item,
                to,
                port,
            } => {
                if self.core.items.get(item).and_then(|it| it.location) != Some(carrier) {
                    let name = self.core.blocks[carrier.0].name.clone();
                    self.core
                        .errors
                        .push(format!("{name}: drop_off de um item que não está com ele"));
                    return;
                }
                let rt = &mut self.core.blocks[to.0];
                rt.incoming = rt.incoming.saturating_sub(1);
                self.leave(item, carrier);
                self.arrive(item, to, port);
            }
        }
        self.flush_doomed();
    }

    fn try_send(&mut self, from: BlockId) {
        if self.core.blocks[from.0].stops != 0 {
            return;
        }
        let mut i = 0;
        while i < self.core.blocks[from.0].outbox.len() {
            let pending = self.core.blocks[from.0].outbox[i];
            match self.find_destination(from, pending) {
                Some(to) => {
                    self.core.blocks[from.0].outbox.remove(i);
                    match self.core.blocks[from.0].transport {
                        Some(carrier) => self.reserve_transport(pending.item, from, to, carrier),
                        None => self.transfer(pending.item, from, to),
                    }
                }
                None => i += 1,
            }
        }
    }

    fn find_destination(&self, from: BlockId, pending: Pending) -> Option<(BlockId, usize)> {
        let outputs = &self.core.blocks[from.0].outputs;
        let item = &self.core.items[pending.item];
        let open = |&(to, in_port): &(BlockId, usize)| {
            let rt = &self.core.blocks[to.0];
            let content = rt.contents.len() + rt.incoming;
            rt.stops == 0 && self.behaviors[to.0].accepts(item, content, in_port)
        };
        match pending.port {
            Some(port) => outputs.get(port).copied().filter(open),
            None => outputs.iter().copied().find(open),
        }
    }

    fn transfer(&mut self, item: ItemId, from: BlockId, (to, in_port): (BlockId, usize)) {
        self.leave(item, from);
        self.arrive(item, to, in_port);
    }

    /// Room in `to` is reserved; the item stays in `from` until the carrier picks it up.
    fn reserve_transport(
        &mut self,
        item: ItemId,
        from: BlockId,
        (to, port): (BlockId, usize),
        carrier: BlockId,
    ) {
        self.core.blocks[to.0].incoming += 1;
        self.core.items[item].awaiting_transport = true;
        let cx = &mut Ctx {
            core: &mut self.core,
            me: from,
        };
        cx.request_task(
            carrier,
            TaskKind::Transport {
                item,
                from,
                to,
                port,
            },
            0,
        );
        cx.send(from, MSG_TRANSPORT_RESERVED, item_key(item));
    }

    /// First half of a move: the item leaves `from` (stats, `on_exit`, upstream retry).
    fn leave(&mut self, item: ItemId, from: BlockId) {
        let now = self.core.now;
        let entered = self.core.items[item].entered_at;
        self.core.take_item_from(from, item);
        let stats = &mut self.core.blocks[from.0].stats;
        stats.output += 1;
        stats.staytime.record(now - entered);
        {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[from.0].on_exit(&mut Ctx { core, me: from }, item);
        }
        // `from` has room again: let its upstream blocks retry, in input-port order.
        for k in 0..self.core.blocks[from.0].inputs.len() {
            let up = self.core.blocks[from.0].inputs[k];
            self.core.mark_dirty(up);
        }
        self.core.pending_move_from = Some(from);
    }

    /// Second half of a move: the item enters `to` through input `port`.
    fn arrive(&mut self, item: ItemId, to: BlockId, port: usize) {
        let from = self.core.pending_move_from.take().map_or(to.0, |b| b.0);
        self.core.put_item_into(to, item);
        self.core.blocks[to.0].stats.input += 1;
        self.core.emit(TraceEvent::Moved {
            item: item_key(item),
            from,
            to: to.0,
        });
        {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[to.0].on_entry(&mut Ctx { core, me: to }, item, port);
        }
        self.flush_doomed();
    }

    fn flush_doomed(&mut self) {
        while let Some(id) = self.core.doomed.pop() {
            let Some(item) = self.core.items.remove(id) else {
                continue;
            };
            if let Some(b) = item.location {
                self.core.take_item_from(b, id);
            }
            if let Some(c) = item.container.and_then(|c| self.core.items.get_mut(c)) {
                c.contents.retain(|&x| x != id);
            }
            self.core.emit(TraceEvent::Destroyed { item: item_key(id) });
            self.core.doomed.extend(item.contents);
        }
    }

    pub fn report(&self) -> RunReport {
        let now = self.core.now;
        let bins = self.config.histogram_bins;
        let blocks = self
            .core
            .blocks
            .iter()
            .map(|rt| BlockReport {
                name: rt.name.clone(),
                kind: rt.kind.to_string(),
                input: rt.stats.input,
                output: rt.stats.output,
                content_now: rt.contents.len(),
                content_mean: rt.stats.content.mean(now),
                content_max: rt.stats.content.max(),
                staytime: TallySummary::with_histogram(&rt.stats.staytime, bins),
                states: rt
                    .stats
                    .states
                    .fractions(now)
                    .into_iter()
                    .map(|(s, f)| (s.name().to_string(), f))
                    .collect(),
                extra: rt
                    .stats
                    .extra
                    .iter()
                    .map(|(k, t)| (k.clone(), TallySummary::with_histogram(t, bins)))
                    .collect(),
                series: rt.series.clone(),
            })
            .collect();
        RunReport {
            replication: self.config.replication,
            seed: self.config.seed,
            stats_from: if now >= self.config.warmup {
                self.config.warmup
            } else {
                0.0
            },
            end_time: now,
            events: self.core.events_processed,
            blocks,
        }
    }
}
