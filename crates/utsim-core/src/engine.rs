//! The simulation kernel: clock, future event list, item arena and flow resolution.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashSet, VecDeque};
use std::sync::Arc;

use rand::Rng;
use slotmap::SlotMap;

use crate::Time;
use crate::block::{
    Behavior, BlockId, Downtime, OutputRule, ShiftEnd, ShiftTable, State, TransferDef, TransferPath,
};
use crate::dist::Dist;
use crate::init::{self, BlockState, ItemState, StartState};
use crate::item::{Item, ItemId, Value};
use crate::report::{BlockReport, RunReport, SeriesPoint, TallySummary};
use crate::rng::{self, SimRng};
use crate::script::{ItemScript, PortScript, ScriptEnv};
use crate::stats::{StateTracker, Tally, TimeWeighted};
use crate::task::{
    MSG_ITEM_ENTERED, MSG_ITEM_EXITED, MSG_TRANSPORT_RESERVED, RequestId, Router, StraightLine,
    TaskKind, TaskRequest,
};
use crate::trace::{Observer, TraceEvent, TraceRecord, item_key};

mod maint;

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
    /// An item finishes a timed direct connection and enters `to` through `port`.
    Arrive {
        item: ItemId,
        from: BlockId,
        to: BlockId,
        port: usize,
    },
    /// Records the time series; internal, not counted in `events_processed`.
    Sample,
    /// Maintenance of a block ([`crate::maintenance`]).
    Maint(BlockId, maint::MaintEvent),
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
    /// Start of the breakdown in progress and end of the last repair (saved states).
    down_since: Time,
    up_since: Time,
    /// An off-shift window with `ShiftEnd::Finish` started while work was in progress:
    /// the inputs are closed and the block runs until the work is done (overtime).
    closing: bool,
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
    /// Items on their way here (room reserved by a transport request or a timed transfer).
    incoming: usize,
    /// Travel time of items leaving through direct connections (None = instantaneous).
    transfer: Option<TransferDef>,
    /// Blocks notified when items enter (`.0`) or leave (`.1`) this block.
    subscribers: Vec<(BlockId, bool, bool)>,
    entry_script: Option<Box<dyn ItemScript>>,
    port_script: Option<Box<dyn PortScript>>,
    /// Maintenance (crew, wear, preventive plans); `None` = plain breakdowns.
    maint: Option<Box<maint::MaintState>>,
    /// A preventive is due: the inputs are closed and the block finishes its work.
    draining: bool,
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
    Inject {
        item: ItemId,
        into: BlockId,
        port: usize,
        /// Who to tell if the target refuses (`None`: refusal is a modelling error).
        notify: Option<BlockId>,
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
    /// Consecutive events at the same simulated instant (stall detection).
    same_instant: u64,
    /// Sequence numbers of cancelled events still sitting in the heap.
    cancelled: HashSet<u64>,
    /// Whether tallies keep raw observations (histograms on).
    keep_raw: bool,
    /// `(block, tally)` whose observations keep their instant ([`Simulation::keep_timed`]).
    timed: Vec<(usize, String)>,
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
    /// Decisions waiting for an external answer, oldest first.
    decisions: VecDeque<Decision>,
    next_decision: u64,
    /// Blocks finishing their work before going off shift (`ShiftEnd::Finish`).
    closing: Vec<BlockId>,
    /// Time-between-two-points meters, in registration order.
    meters: Vec<Meter>,
    /// Calendar instant at `t = 0` (see [`crate::init`]); 0 without an initial state.
    calendar: Time,
    /// Blocks finishing their work before a preventive maintenance.
    draining: Vec<BlockId>,
}

/// Measures, per item, the time from leaving `from` to entering `to`; each measurement
/// is recorded in the tally `value` of block `into`.
#[derive(Clone, Copy, Debug)]
struct Meter {
    from: BlockId,
    to: BlockId,
    into: BlockId,
}

impl Core {
    /// Adds an observation to the named tally of `block`.
    fn record_extra(&mut self, block: BlockId, name: &str, value: f64) {
        let keep_raw = self.keep_raw;
        let now = self.now;
        let timed_list = &self.timed;
        let extra = &mut self.blocks[block.0].stats.extra;
        // Hot path (a sink records once per item): look up first, allocate the key once.
        if let Some(tally) = extra.get_mut(name) {
            tally.record_at(now, value);
            return;
        }
        extra
            .entry(name.to_string())
            .or_insert_with(|| {
                let mut tally = if keep_raw {
                    Tally::with_raw()
                } else {
                    Tally::default()
                };
                if timed_list.iter().any(|(b, n)| *b == block.0 && n == name) {
                    tally.keep_timed();
                }
                tally
            })
            .record_at(now, value);
    }

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
        if self.blocks[b.0].maint.is_some() {
            self.maint_refresh(b);
        }
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

    /// Whether this block is finishing its work before going off shift
    /// (`ShiftEnd::Finish`): it runs, but must not start new work.
    pub fn is_closing(&self) -> bool {
        let rt = &self.core.blocks[self.me.0];
        rt.closing || rt.draining
    }

    /// Whether `block` is finishing its shift or off shift under `ShiftEnd::Finish`:
    /// it takes no new work (dispatchers skip such team members). Always false under
    /// `ShiftEnd::Pause`, where a stopped member may still be given work that waits.
    pub fn away_on_finish(&self, block: BlockId) -> bool {
        let rt = &self.core.blocks[block.0];
        rt.closing
            || (rt.stops & OFF_SHIFT != 0
                && rt
                    .shifts
                    .as_ref()
                    .is_some_and(|t| t.on_end == ShiftEnd::Finish))
    }

    /// Items held by this block that were not released yet and do not wait for a
    /// transporter (work in progress, for [`Behavior::work_in_progress`]).
    pub fn unreleased_count(&self) -> usize {
        let rt = &self.core.blocks[self.me.0];
        rt.contents
            .iter()
            .filter(|&&id| {
                !self.core.items[id].awaiting_transport && !rt.outbox.iter().any(|p| p.item == id)
            })
            .count()
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

    /// Calendar instant that corresponds to `t = 0` (0 unless the run starts from an
    /// initial state, see [`crate::init`]). Cycles such as arrival schedules start at
    /// this point of the calendar.
    pub fn calendar_start(&self) -> Time {
        self.core.calendar
    }

    /// Time left of an activity with duration `dist` that already ran for `elapsed`,
    /// from this block's stream (see [`Dist::sample_residual`]).
    pub fn sample_residual(&mut self, dist: &Dist, elapsed: Time) -> Time {
        dist.sample_residual(elapsed, self.rng())
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
            marks: Vec::new(),
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
        let me = self.me;
        self.core.record_extra(me, name, value);
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

    /// The input side reopened for a reason other than an item leaving (e.g. a
    /// conveyor's entry zone cleared): lets the upstream blocks retry now.
    pub fn wake_inputs(&mut self) {
        for k in 0..self.core.blocks[self.me.0].inputs.len() {
            let up = self.core.blocks[self.me.0].inputs[k];
            self.core.mark_dirty(up);
        }
    }

    /// Records how many tokens activity `activity` of this flow block now holds.
    pub fn trace_tokens(&mut self, activity: usize, count: u32) {
        let flow = self.me.0;
        self.core.emit(TraceEvent::Tokens {
            flow,
            activity,
            count,
        });
    }

    /// Records a fluid node's level and rate (fluid networks), for the 3D view.
    pub fn trace_fluid(&mut self, node: usize, level: f64, rate: f64) {
        let net = self.me.0;
        self.core.emit(TraceEvent::Fluid {
            net,
            node,
            level,
            rate,
        });
    }

    /// Records the storage position of an item in this block (racks), for the 3D view.
    pub fn trace_slot(&mut self, item: ItemId, slot: usize) {
        let block = self.me.0;
        self.core.emit(TraceEvent::Slot {
            item: item_key(item),
            block,
            slot,
        });
    }

    /// Asks to be told (`MSG_ITEM_ENTERED` / `MSG_ITEM_EXITED` messages) when items
    /// enter and/or leave `block`. Used by token flows to wait for events.
    pub fn subscribe(&mut self, block: BlockId, entry: bool, exit: bool) {
        let me = self.me;
        let subs = &mut self.core.blocks[block.0].subscribers;
        match subs.iter_mut().find(|(b, _, _)| *b == me) {
            Some(s) => {
                s.1 |= entry;
                s.2 |= exit;
            }
            None => subs.push((me, entry, exit)),
        }
    }

    /// The live item with this trace key (as carried by `MSG_ITEM_*` payloads).
    pub fn item_by_key(&self, key: u64) -> Option<ItemId> {
        let id = ItemId::from(slotmap::KeyData::from_ffi(key));
        self.core.items.contains_key(id).then_some(id)
    }

    /// Creates an item and places it into input `port` of `into` right after the
    /// current hook (as if it arrived there). The target must accept it, otherwise the
    /// run stops with a modelling error. Returns the new item so labels can be set now.
    pub fn inject_item(&mut self, into: BlockId, port: usize) -> ItemId {
        self.inject(into, port, None)
    }

    /// Like [`Ctx::inject_item`], but a refusal (target full or stopped) is not an
    /// error: the item is discarded and this block receives `MSG_INJECT_REFUSED`
    /// (payload: the target's index), so it can wait and retry (e.g. on the target's
    /// exits, via [`Ctx::subscribe`]).
    pub fn try_inject_item(&mut self, into: BlockId, port: usize) -> ItemId {
        let me = self.me;
        self.inject(into, port, Some(me))
    }

    fn inject(&mut self, into: BlockId, port: usize, notify: Option<BlockId>) -> ItemId {
        let now = self.core.now;
        let id = self.core.items.insert(Item {
            labels: BTreeMap::new(),
            created_at: now,
            entered_at: now,
            location: None,
            container: None,
            contents: Vec::new(),
            awaiting_transport: false,
            marks: Vec::new(),
        });
        self.core.emit(TraceEvent::Created {
            item: item_key(id),
            block: into.0,
        });
        self.core.ops.push_back(KernelOp::Inject {
            item: id,
            into,
            port,
            notify,
        });
        id
    }

    /// Asks the external agent to choose one of `options`; the run pauses after the
    /// current event and the answer arrives as a `MSG_DECISION` message (payload =
    /// option index). Several requests are answered in order.
    pub fn request_decision(&mut self, options: usize, observation: Vec<f64>) -> u64 {
        self.core.next_decision += 1;
        let id = self.core.next_decision;
        let decision = Decision {
            id,
            block: self.me,
            time: self.core.now,
            options,
            observation,
        };
        self.core.decisions.push_back(decision);
        id
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
            OutputRule::Script(_) => {
                let name = rt.name.clone();
                match rt.port_script.as_mut() {
                    Some(script) => {
                        let mut env = ScriptEnv {
                            now: core.now,
                            rng: &mut rt.rng,
                            block: &name,
                        };
                        match script.port(&core.items[id], n, &mut env) {
                            Ok(p) if p < n => Some(p),
                            Ok(p) => {
                                core.errors.push(format!(
                                    "{name}: o script de saída devolveu a porta {} (existem 1..={n})",
                                    p + 1
                                ));
                                None
                            }
                            Err(e) => {
                                core.errors
                                    .push(format!("{name}: erro no script de saída: {e}"));
                                None
                            }
                        }
                    }
                    None => {
                        core.errors
                            .push(format!("{name}: regra Script sem script compilado"));
                        None
                    }
                }
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

/// Events at one instant after which the run is declared stuck.
const STALL_LIMIT: u64 = 2_000_000;

/// Offset of the breakdown random streams, far above any block index.
const DOWNTIME_STREAMS: u64 = 1 << 40;

/// Stop reasons. A block runs only when no reason is active.
const BREAKDOWN: u8 = 1;
const OFF_SHIFT: u8 = 2;
/// Stopped from outside the model (emulation, an operator pressing "stop").
const EXTERNAL: u8 = 4;
/// In preventive maintenance (or waiting for its technician).
const MAINTENANCE: u8 = 8;

/// Offset of the maintenance random streams (preventive durations).
const MAINTENANCE_STREAMS: u64 = 1 << 41;

/// A question the simulation asks an external agent (reinforcement learning,
/// a human): pick one of `options`. The run pauses until [`Simulation::decide`].
#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub id: u64,
    pub block: BlockId,
    pub time: Time,
    pub options: usize,
    /// Numbers describing the situation (chosen by the model).
    pub observation: Vec<f64>,
}

/// Outcome of advancing the simulation step by step.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Reached the requested time with no pending decision.
    Paused(Time),
    /// Waiting for [`Simulation::decide`].
    Decision(Decision),
    /// No events left before the stop time (call [`Simulation::finish`]).
    Finished,
}

/// A runnable model instance: one replication.
pub struct Simulation {
    core: Core,
    behaviors: Vec<Box<dyn Behavior>>,
    config: RunConfig,
    started: bool,
    /// Applied when the run starts (see [`Simulation::set_start_state`]).
    start_state: Option<StartState>,
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
                same_instant: 0,
                cancelled: HashSet::new(),
                keep_raw: config.histogram_bins.is_some_and(|b| b > 0),
                timed: Vec::new(),
                samples_taken: 0,
                trace: None,
                observer: None,
                ops: VecDeque::new(),
                next_request: 0,
                router: Arc::new(StraightLine),
                pending_move_from: None,
                decisions: VecDeque::new(),
                next_decision: 0,
                closing: Vec::new(),
                meters: Vec::new(),
                calendar: 0.0,
                draining: Vec::new(),
            },
            behaviors: Vec::new(),
            config,
            started: false,
            start_state: None,
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
            down_since: 0.0,
            up_since: 0.0,
            closing: false,
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
            transfer: None,
            subscribers: Vec::new(),
            entry_script: None,
            port_script: None,
            maint: None,
            draining: false,
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

    /// Items leaving `block` through direct connections take `delay + distance / speed`
    /// to arrive; room is reserved at the destination while they travel.
    pub fn set_transfer(&mut self, block: BlockId, transfer: TransferDef) {
        self.core.blocks[block.0].transfer = Some(transfer);
    }

    /// Replaces the straight-line router (e.g. by a path network).
    pub fn set_router(&mut self, router: Arc<dyn Router>) {
        self.core.router = router;
    }

    /// Script run on every item entering `block`, before the block's `on_entry`.
    pub fn set_entry_script(&mut self, block: BlockId, script: Box<dyn ItemScript>) {
        self.core.blocks[block.0].entry_script = Some(script);
    }

    /// Script choosing the output port of `block` (used by `OutputRule::Script`).
    pub fn set_port_script(&mut self, block: BlockId, script: Box<dyn PortScript>) {
        self.core.blocks[block.0].port_script = Some(script);
    }

    /// Enables a repeating shift table (scheduled off-shift windows) on a block.
    pub fn set_shifts(&mut self, block: BlockId, shifts: ShiftTable) {
        self.core.blocks[block.0].shifts = Some(shifts);
    }

    /// Measures the time each item takes from leaving block `from` to entering block
    /// `to`, recording it in the tally `value` of block `into` (a block that holds no
    /// items, made only to carry the measurement). An item that never reaches `to`
    /// is not counted, and one that passes `from` again restarts the clock.
    pub fn add_meter(&mut self, from: BlockId, to: BlockId, into: BlockId) {
        self.core.meters.push(Meter { from, to, into });
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

    /// Keeps the instant of every observation of tally `measure` of `block`
    /// (`"staytime"` or one of the block's own tallies, e.g. `"time_in_system"`): the
    /// report then carries them in [`crate::TallySummary::observations`]. Off by
    /// default; it only adds memory and never changes any statistic. Call it before
    /// the run starts.
    pub fn keep_timed(&mut self, block: BlockId, measure: &str) {
        let Some(rt) = self.core.blocks.get_mut(block.0) else {
            return;
        };
        if measure == "staytime" {
            rt.stats.staytime.keep_timed();
        } else if let Some(tally) = rt.stats.extra.get_mut(measure) {
            tally.keep_timed();
        } else if !self
            .core
            .timed
            .iter()
            .any(|(b, n)| *b == block.0 && n == measure)
        {
            self.core.timed.push((block.0, measure.to_string()));
        }
    }

    /// Starts the run from `state` instead of an empty system (see [`crate::init`]).
    /// Call it before the run starts; problems with the state are reported by the first
    /// call that starts the run ([`Simulation::begin`], `run_until`, `run`).
    pub fn set_start_state(&mut self, state: StartState) {
        if !self.started {
            self.start_state = Some(state);
        }
    }

    /// Calendar instant that corresponds to `t = 0` (0 without an initial state).
    pub fn calendar_start(&self) -> Time {
        self.core.calendar
    }

    /// The state of every block now, ready to start another run from here (see
    /// [`crate::init`]): items held with their times and labels, work in progress,
    /// breakdowns, positions. `clock` is the calendar instant now. Blocks with nothing
    /// to say are left out. The notes list what cannot be represented and is lost
    /// (items in transit, packed items, items carried by transporters...).
    pub fn save_state(&mut self) -> (StartState, Vec<String>) {
        let now = self.core.now;
        let mut notes = Vec::new();
        let transit = self
            .core
            .items
            .values()
            .filter(|it| it.location.is_none() && it.container.is_none())
            .count();
        if transit > 0 {
            notes.push(format!(
                "{transit} item(ns) em trânsito entre objetos não entram no estado gravado"
            ));
        }
        let mut blocks = Vec::new();
        for b in 0..self.core.blocks.len() {
            let mut state = BlockState::default();
            let mut packed = 0;
            {
                let rt = &self.core.blocks[b];
                for &id in &rt.contents {
                    let it = &self.core.items[id];
                    packed += it.contents.len();
                    let stay = now - it.entered_at;
                    state.items.push(ItemState {
                        elapsed: stay,
                        stay: Some(stay),
                        age: Some(now - it.created_at),
                        labels: it.labels.clone(),
                        ..ItemState::default()
                    });
                }
                if rt.downtime.is_some() && rt.maint.is_none() {
                    if rt.stops & BREAKDOWN != 0 {
                        state.down = Some(now - rt.down_since);
                    } else {
                        state.up = Some(now - rt.up_since);
                    }
                }
            }
            let name = self.core.blocks[b].name.clone();
            if packed > 0 {
                notes.push(format!(
                    "{name}: {packed} item(ns) embalados em contêineres não entram no estado gravado (o contêiner entra vazio)"
                ));
            }
            let note = {
                let Simulation {
                    core, behaviors, ..
                } = self;
                let cx = Ctx {
                    core,
                    me: BlockId(b),
                };
                behaviors[b].save_state(&cx, &mut state)
            };
            if let Some(note) = note {
                notes.push(format!("{name}: {note}"));
            }
            if let Some(note) = self.maint_save(BlockId(b), &mut state) {
                notes.push(format!("{name}: {note}"));
            }
            state.items.iter_mut().for_each(ItemState::tidy);
            if !state.is_empty() {
                blocks.push((BlockId(b), state));
            }
        }
        let state = StartState {
            clock: self.core.calendar + now,
            blocks,
        };
        (state, notes)
    }

    /// Streams every [`TraceEvent`] to `observer` while the run progresses.
    pub fn set_observer(&mut self, observer: Box<dyn Observer>) {
        self.core.observer = Some(observer);
    }

    pub fn now(&self) -> Time {
        self.core.now
    }

    /// Runs the whole replication. A model that asks for external decisions cannot
    /// run this way: use [`Simulation::run_until`] and [`Simulation::decide`].
    pub fn run(&mut self) -> Result<RunReport, SimError> {
        match self.run_until(f64::INFINITY)? {
            Step::Decision(d) => Err(SimError::Model {
                time: d.time,
                messages: vec![format!(
                    "{}: o modelo pede uma decisão externa (agente); rode passo a passo",
                    self.core.blocks[d.block.0].name
                )],
            }),
            _ => Ok(self.finish()),
        }
    }

    /// Initializes the run (blocks' `reset`). Called automatically by the stepping API.
    pub fn begin(&mut self) -> Result<(), SimError> {
        if self.started {
            return Ok(());
        }
        self.started = true;
        self.start()
    }

    /// Processes events up to time `t` (and never beyond the stop time). Stops early
    /// when a decision is pending.
    pub fn run_until(&mut self, t: Time) -> Result<Step, SimError> {
        self.begin()?;
        if let Some(d) = self.core.decisions.front() {
            return Ok(Step::Decision(d.clone()));
        }
        let limit = t.min(self.config.stop_time);
        while self.core.events.peek().is_some_and(|ev| ev.time <= limit) {
            let ev = self.core.events.pop().expect("peeked");
            if !self.core.cancelled.is_empty() && self.core.cancelled.remove(&ev.seq) {
                continue;
            }
            if ev.time == self.core.now {
                self.core.same_instant += 1;
                if self.core.same_instant > STALL_LIMIT {
                    return Err(SimError::Model {
                        time: ev.time,
                        messages: vec![format!(
                            "mais de {STALL_LIMIT} eventos no mesmo instante: o tempo não avança (laço no modelo, por exemplo tempos de processo ou intervalos de chegada iguais a zero)"
                        )],
                    });
                }
            } else {
                self.core.same_instant = 0;
            }
            self.core.now = ev.time;
            if !matches!(ev.kind, EventKind::Sample) {
                self.core.events_processed += 1;
            }
            self.dispatch(ev.kind);
            self.resolve();
            if !self.core.closing.is_empty() {
                self.check_closing();
            }
            if !self.core.draining.is_empty() {
                self.maint_check_drains();
            }
            self.check()?;
            if let Some(d) = self.core.decisions.front() {
                return Ok(Step::Decision(d.clone()));
            }
        }
        let pending =
            self.core.events.iter().any(|ev| {
                ev.time <= self.config.stop_time && !self.core.cancelled.contains(&ev.seq)
            });
        if !pending || limit >= self.config.stop_time {
            return Ok(Step::Finished);
        }
        if t.is_finite() && t > self.core.now {
            self.core.now = t;
        }
        Ok(Step::Paused(self.core.now))
    }

    /// The oldest decision waiting for an answer.
    pub fn pending_decision(&self) -> Option<&Decision> {
        self.core.decisions.front()
    }

    /// Answers the oldest pending decision with option `choice` (0-based).
    pub fn decide(&mut self, choice: usize) -> Result<(), SimError> {
        let Some(d) = self.core.decisions.pop_front() else {
            return Err(SimError::Model {
                time: self.core.now,
                messages: vec!["nenhuma decisão pendente".into()],
            });
        };
        if choice >= d.options {
            let name = self.core.blocks[d.block.0].name.clone();
            self.core.decisions.push_front(d);
            return Err(SimError::Model {
                time: self.core.now,
                messages: vec![format!("{name}: opção fora do intervalo")],
            });
        }
        self.core.ops.push_back(KernelOp::Message {
            to: d.block,
            from: d.block,
            tag: crate::task::MSG_DECISION,
            payload: choice as u64,
        });
        self.resolve();
        self.check()
    }

    /// Ends the run at the stop time and returns the report.
    pub fn finish(&mut self) -> RunReport {
        if self.config.stop_time.is_finite() && self.core.now < self.config.stop_time {
            self.core.now = self.config.stop_time;
        }
        self.report()
    }

    /// Stops a block from outside the model (emulation): ports close, timers pause.
    pub fn stop_block(&mut self, block: BlockId) {
        if self.core.blocks[block.0].stops & EXTERNAL == 0 {
            self.stop(block, EXTERNAL);
            self.resolve();
        }
    }

    pub fn resume_block(&mut self, block: BlockId) {
        if self.core.blocks[block.0].stops & EXTERNAL != 0 {
            self.resume(block, EXTERNAL);
            self.resolve();
        }
    }

    /// Current simulation time.
    pub fn time(&self) -> Time {
        self.core.now
    }

    pub fn stop_time(&self) -> Time {
        self.config.stop_time
    }

    /// Live view of every block (name, state, content), for emulation and agents.
    pub fn block_status(&self) -> Vec<(String, Option<State>, usize)> {
        self.core
            .blocks
            .iter()
            .map(|rt| {
                (
                    rt.name.clone(),
                    rt.stats.states.current(),
                    rt.contents.len(),
                )
            })
            .collect()
    }

    fn start(&mut self) -> Result<(), SimError> {
        let n = self.core.blocks.len();
        let mut states: Vec<Option<BlockState>> = vec![None; n];
        if let Some(start) = self.start_state.take() {
            if start.clock.is_finite() {
                self.core.calendar = start.clock;
            } else {
                self.core.errors.push(format!(
                    "estado inicial: o relógio (clock) deve ser finito (está {})",
                    start.clock
                ));
            }
            for (b, state) in start.blocks {
                if let Some(slot) = states.get_mut(b.0) {
                    *slot = Some(state);
                }
            }
        }
        for b in 0..self.behaviors.len() {
            let me = BlockId(b);
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[b].reset(&mut Ctx { core, me });
            self.flush_doomed();
        }
        for (b, state) in states.iter().enumerate() {
            if let Some(state) = state {
                self.restore_block(BlockId(b), state);
            }
        }
        self.check()?;
        if self.config.warmup > 0.0 {
            self.core
                .schedule_at(self.config.warmup, EventKind::EndWarmup);
        }
        for (b, state) in states.iter().enumerate() {
            let b = BlockId(b);
            if self.core.blocks[b.0].maint.is_some() {
                self.maint_start(b, state.as_ref());
            } else {
                match state.as_ref().map(|s| (s.down, s.up)) {
                    Some((Some(elapsed), _)) => self.start_down(b, elapsed),
                    Some((None, Some(elapsed))) => self.schedule_failure_after(b, elapsed),
                    _ => self.schedule_failure(b),
                }
            }
            self.schedule_shifts(b);
            if let Some(expected) = state.as_ref().and_then(|s| s.off_shift) {
                let rt = &self.core.blocks[b.0];
                if rt.shifts.is_some() && (rt.open_windows > 0) != expected {
                    let message = format!(
                        "estado inicial de {}: off_shift = {expected}, mas no relógio {} o calendário de turnos diz o contrário",
                        rt.name, self.core.calendar
                    );
                    self.core.errors.push(message);
                }
            }
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

    /// Initial state: the block has run `elapsed` since its last repair; the time to the
    /// next failure is the residual of `mtbf`.
    fn schedule_failure_after(&mut self, b: BlockId, elapsed: Time) {
        let now = self.core.now;
        let rt = &mut self.core.blocks[b.0];
        if let Some(dt) = &rt.downtime {
            let delay = dt.mtbf.sample_residual(elapsed, &mut rt.downtime_rng);
            rt.up_since = now - elapsed;
            self.core.schedule_at(now + delay, EventKind::Fail(b));
        }
    }

    /// Initial state: the block has been down for `elapsed`; the repair takes the
    /// residual of `mttr`.
    fn start_down(&mut self, b: BlockId, elapsed: Time) {
        let now = self.core.now;
        let rt = &mut self.core.blocks[b.0];
        let Some(dt) = &rt.downtime else { return };
        let repair = dt.mttr.sample_residual(elapsed, &mut rt.downtime_rng);
        rt.down_since = now - elapsed;
        self.core.schedule_at(now + repair, EventKind::Repair(b));
        self.stop(b, BREAKDOWN);
    }

    fn fail(&mut self, b: BlockId) {
        let now = self.core.now;
        let rt = &mut self.core.blocks[b.0];
        let repair = rt
            .downtime
            .as_ref()
            .map_or(0.0, |dt| dt.mttr.sample(&mut rt.downtime_rng).max(0.0));
        rt.down_since = now;
        self.core.schedule_at(now + repair, EventKind::Repair(b));
        self.stop(b, BREAKDOWN);
    }

    fn repair(&mut self, b: BlockId) {
        self.core.blocks[b.0].up_since = self.core.now;
        self.resume(b, BREAKDOWN);
        self.schedule_failure(b);
    }

    /// Schedules every off-shift window of the block. With a calendar start `c`, the
    /// cycle is at `c mod period` at `t = 0`: a window already open starts at once with
    /// the time it has left. Without one (`c = 0`) the windows start at their offsets.
    fn schedule_shifts(&mut self, b: BlockId) {
        let Some(table) = &self.core.blocks[b.0].shifts else {
            return;
        };
        let period = table.period;
        let phase = self.core.calendar.rem_euclid(period);
        let windows: Vec<(Time, Time)> = table.off.iter().map(|w| (w.start, w.duration)).collect();
        let now = self.core.now;
        for (k, (start, duration)) in windows.into_iter().enumerate() {
            let off = start - phase;
            if off >= 0.0 {
                self.core.schedule_at(now + off, EventKind::ShiftOff(b, k));
            } else if off + duration > 0.0 {
                self.open_window(b, k, off + duration, off + period);
            } else {
                self.core
                    .schedule_at(now + off + period, EventKind::ShiftOff(b, k));
            }
        }
    }

    fn shift_off(&mut self, b: BlockId, k: usize) {
        let Some(table) = &self.core.blocks[b.0].shifts else {
            return;
        };
        let (period, duration) = (table.period, table.off[k].duration);
        self.open_window(b, k, duration, period);
    }

    /// Window `k` opens now: it closes after `closes_in` and opens again after `next_in`.
    fn open_window(&mut self, b: BlockId, k: usize, closes_in: Time, next_in: Time) {
        let now = self.core.now;
        let Some(table) = &self.core.blocks[b.0].shifts else {
            return;
        };
        let on_end = table.on_end;
        self.core
            .schedule_at(now + closes_in, EventKind::ShiftOn(b));
        self.core
            .schedule_at(now + next_in, EventKind::ShiftOff(b, k));
        let rt = &mut self.core.blocks[b.0];
        rt.open_windows += 1;
        // `Finish` only matters for a running block: a stopped one has no progress to make.
        if on_end == ShiftEnd::Finish && rt.stops == 0 {
            if !rt.closing {
                rt.closing = true;
                self.core.closing.push(b);
            }
            self.finish_closing(b);
        } else {
            self.stop(b, OFF_SHIFT);
        }
    }

    fn shift_on(&mut self, b: BlockId) {
        let rt = &mut self.core.blocks[b.0];
        rt.open_windows = rt.open_windows.saturating_sub(1);
        if rt.open_windows > 0 {
            return;
        }
        if rt.closing {
            // The overtime outlasted the window: the block simply keeps working and
            // opens its inputs again.
            rt.closing = false;
            self.core.closing.retain(|&c| c != b);
            self.reopen(b);
        } else if rt.stops & OFF_SHIFT != 0 {
            self.resume(b, OFF_SHIFT);
        }
    }

    /// A closing block whose work is done goes off shift (it stays closing while
    /// stopped, e.g. by a breakdown during the overtime).
    fn finish_closing(&mut self, b: BlockId) {
        if self.core.blocks[b.0].stops != 0 {
            return;
        }
        let busy = {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[b.0].work_in_progress(&Ctx { core, me: b })
        };
        if !busy {
            self.core.blocks[b.0].closing = false;
            self.core.closing.retain(|&c| c != b);
            self.stop(b, OFF_SHIFT);
            self.resolve();
        }
    }

    /// Checks every closing block after an event.
    fn check_closing(&mut self) {
        let closing = self.core.closing.clone();
        for b in closing {
            self.finish_closing(b);
        }
    }

    /// Inputs of `b` open again: its upstream blocks may send.
    fn reopen(&mut self, b: BlockId) {
        self.core.mark_dirty(b);
        for k in 0..self.core.blocks[b.0].inputs.len() {
            let up = self.core.blocks[b.0].inputs[k];
            self.core.mark_dirty(up);
        }
    }

    /// State shown while stopped: a breakdown wins over being off shift.
    fn stopped_state(stops: u8) -> State {
        if stops & BREAKDOWN != 0 {
            State::Down
        } else if stops & MAINTENANCE != 0 {
            State::Maintenance
        } else if stops & EXTERNAL != 0 {
            State::Stopped
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
        let shown = self.shown_stopped(b);
        self.core.set_block_state(b, shown);
        if was_running {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[b.0].on_stop(&mut Ctx { core, me: b });
            self.flush_doomed();
        }
        self.core.maint_refresh(b);
    }

    /// Clears a stop reason; the block runs again once no reason is left.
    fn resume(&mut self, b: BlockId, reason: u8) {
        let rt = &mut self.core.blocks[b.0];
        rt.stops &= !reason;
        if rt.stops != 0 {
            let shown = self.shown_stopped(b);
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
        if self.core.blocks[b.0].maint.is_some() {
            self.core.maint_refresh(b);
            self.maint_try_pm(b);
        }
    }

    /// State shown while stopped, with the maintenance states when the block has them.
    fn shown_stopped(&self, b: BlockId) -> State {
        self.core
            .maint_shown(b)
            .unwrap_or_else(|| Self::stopped_state(self.core.blocks[b.0].stops))
    }

    /// Places the items of an initial state in block `b` and lets the block rebuild its
    /// timers ([`Behavior::restore`]). Problems become modelling errors.
    fn restore_block(&mut self, b: BlockId, state: &BlockState) {
        const MAX_ITEMS: usize = 10_000_000;
        let name = self.core.blocks[b.0].name.clone();
        let mut problems = Vec::new();
        if let Err(e) = init::check_times(state) {
            problems.push(e);
        }
        let rt = &self.core.blocks[b.0];
        if (state.down.is_some() || state.up.is_some()) && rt.downtime.is_none() {
            problems.push("'down' e 'up' exigem quebras (downtime) no objeto".to_string());
        }
        if let Some(p) = self.maint_check_state(b, state) {
            problems.push(p);
        }
        let rt = &self.core.blocks[b.0];
        if state.off_shift.is_some() && rt.shifts.is_none() {
            problems.push("'off_shift' exige turnos (shifts) no objeto".to_string());
        }
        if state.count.is_some_and(|n| n > MAX_ITEMS) || state.items.len() > MAX_ITEMS {
            problems.push(format!("mais de {MAX_ITEMS} itens"));
        }
        if !problems.is_empty() {
            for p in problems {
                self.core
                    .errors
                    .push(format!("estado inicial de {name}: {p}"));
            }
            return;
        }
        let mut state = state.clone();
        if let Some(n) = state.count.take() {
            state.items = vec![ItemState::default(); n];
        }
        let now = self.core.now;
        for it in &state.items {
            let id = self.core.items.insert(Item {
                labels: it.labels.clone(),
                created_at: now - it.age_time(),
                entered_at: now - it.stay_time(),
                location: Some(b),
                container: None,
                contents: Vec::new(),
                awaiting_transport: false,
                marks: Vec::new(),
            });
            self.core.blocks[b.0].contents.push(id);
            self.core.emit(TraceEvent::Created {
                item: item_key(id),
                block: b.0,
            });
        }
        self.core.update_content(b);
        let result = {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[b.0].restore(&mut Ctx { core, me: b }, &state)
        };
        self.flush_doomed();
        if let Err(e) = result {
            self.core
                .errors
                .push(format!("estado inicial de {name}: {e}"));
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
            EventKind::Arrive {
                item,
                from,
                to,
                port,
            } => {
                if !self.core.items.contains_key(item) {
                    return;
                }
                let rt = &mut self.core.blocks[to.0];
                rt.incoming = rt.incoming.saturating_sub(1);
                self.core.pending_move_from = Some(from);
                self.arrive(item, to, port);
            }
            EventKind::Fail(b) => self.fail(b),
            EventKind::Repair(b) => self.repair(b),
            EventKind::ShiftOff(b, k) => self.shift_off(b, k),
            EventKind::ShiftOn(b) => self.shift_on(b),
            EventKind::Sample => self.sample(),
            EventKind::Maint(b, ev) => self.maint_event(b, ev),
            EventKind::EndWarmup => {
                let now = self.core.now;
                self.core.blocks.iter_mut().for_each(|rt| {
                    rt.stats.reset(now);
                    if let Some(m) = rt.maint.as_deref_mut() {
                        m.reset_stats(now);
                    }
                });
                for b in 0..self.behaviors.len() {
                    let Simulation {
                        core, behaviors, ..
                    } = self;
                    behaviors[b].on_warmup(&mut Ctx {
                        core,
                        me: BlockId(b),
                    });
                }
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
                let for_maintenance = tag == crate::task::MSG_ARRIVED
                    && self.core.blocks[to.0]
                        .maint
                        .as_deref()
                        .is_some_and(|m| m.owns(payload));
                if for_maintenance {
                    self.maint_arrived(to, from);
                } else {
                    let Simulation {
                        core, behaviors, ..
                    } = self;
                    behaviors[to.0].on_message(&mut Ctx { core, me: to }, from, tag, payload);
                }
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
            KernelOp::Inject {
                item,
                into,
                port,
                notify,
            } => {
                let accepted = {
                    let rt = &self.core.blocks[into.0];
                    let content = rt.contents.len() + rt.incoming;
                    rt.stops == 0
                        && !rt.closing
                        && !rt.draining
                        && self.behaviors[into.0].accepts(&self.core.items[item], content, port)
                };
                if accepted {
                    self.core.pending_move_from = Some(into);
                    self.arrive(item, into, port);
                } else {
                    self.core.items.remove(item);
                    self.core.emit(TraceEvent::Destroyed {
                        item: item_key(item),
                    });
                    match notify {
                        Some(requester) => self.core.ops.push_back(KernelOp::Message {
                            to: requester,
                            from: into,
                            tag: crate::task::MSG_INJECT_REFUSED,
                            payload: into.0 as u64,
                        }),
                        None => {
                            let name = self.core.blocks[into.0].name.clone();
                            self.core.errors.push(format!(
                                "{name}: não aceitou o item criado pelo fluxo (cheio ou parado)"
                            ));
                        }
                    }
                }
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
                    if let Some(m) = self.core.blocks[from.0].maint.as_deref_mut() {
                        m.count_output(to.0, &self.core.items[pending.item].labels);
                    }
                    let rt = &self.core.blocks[from.0];
                    match (rt.transport, rt.transfer) {
                        (Some(carrier), _) => {
                            self.reserve_transport(pending.item, from, to, carrier);
                        }
                        (None, Some(spec)) => self.timed_transfer(pending.item, from, to, spec),
                        (None, None) => self.transfer(pending.item, from, to),
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
            rt.stops == 0
                && !rt.closing
                && !rt.draining
                && self.behaviors[to.0].accepts(item, content, in_port)
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

    /// Direct connection with a travel time: the item leaves `from` now and enters `to`
    /// after `delay + distance / speed`. Room in `to` is reserved meanwhile, and the item
    /// belongs to no block while it travels. A zero total time is a plain instant move.
    fn timed_transfer(
        &mut self,
        item: ItemId,
        from: BlockId,
        (to, port): (BlockId, usize),
        spec: TransferDef,
    ) {
        let a = self.core.blocks[from.0].position;
        let b = self.core.blocks[to.0].position;
        let path = match spec.path {
            TransferPath::Straight => vec![a, b],
            TransferPath::Network => self.core.router.route(a, b),
        };
        let duration = crate::task::path_length(&path) / spec.speed;
        if spec.delay + duration <= 0.0 {
            self.transfer(item, from, (to, port));
            return;
        }
        self.core.blocks[to.0].incoming += 1;
        self.leave(item, from);
        self.core.pending_move_from = None;
        self.core.items[item].location = None;
        self.core.emit(TraceEvent::Transit {
            item: item_key(item),
            from: from.0,
            to: to.0,
            delay: spec.delay,
            duration,
            path,
        });
        let time = self.core.now + spec.delay + duration;
        self.core.schedule_at(
            time,
            EventKind::Arrive {
                item,
                from,
                to,
                port,
            },
        );
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
        for k in 0..self.core.meters.len() {
            if self.core.meters[k].from == from {
                let marks = &mut self.core.items[item].marks;
                marks.retain(|(m, _)| *m != k);
                marks.push((k, now));
            }
        }
        if self.core.blocks[from.0].maint.is_some() {
            self.core.maint_cycle(from);
        }
        let stats = &mut self.core.blocks[from.0].stats;
        stats.output += 1;
        stats.staytime.record_at(now, now - entered);
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
        self.notify(from, MSG_ITEM_EXITED, item);
        self.core.pending_move_from = Some(from);
    }

    /// Second half of a move: the item enters `to` through input `port`.
    fn arrive(&mut self, item: ItemId, to: BlockId, port: usize) {
        let from = self.core.pending_move_from.take().map_or(to.0, |b| b.0);
        self.core.put_item_into(to, item);
        self.core.blocks[to.0].stats.input += 1;
        if !self.core.meters.is_empty() {
            let now = self.core.now;
            for k in 0..self.core.meters.len() {
                let meter = self.core.meters[k];
                if meter.to != to {
                    continue;
                }
                let marks = &mut self.core.items[item].marks;
                if let Some(at) = marks.iter().position(|(m, _)| *m == k) {
                    let t0 = marks.remove(at).1;
                    self.core.record_extra(meter.into, "value", now - t0);
                }
            }
        }
        if from != to.0 {
            self.core.emit(TraceEvent::Moved {
                item: item_key(item),
                from,
                to: to.0,
            });
        }
        self.run_entry_script(item, to);
        {
            let Simulation {
                core, behaviors, ..
            } = self;
            behaviors[to.0].on_entry(&mut Ctx { core, me: to }, item, port);
        }
        self.notify(to, MSG_ITEM_ENTERED, item);
        self.flush_doomed();
    }

    fn run_entry_script(&mut self, item: ItemId, block: BlockId) {
        let core = &mut self.core;
        let rt = &mut core.blocks[block.0];
        let Some(script) = rt.entry_script.as_mut() else {
            return;
        };
        let mut env = ScriptEnv {
            now: core.now,
            rng: &mut rt.rng,
            block: &rt.name,
        };
        if let Err(e) = script.run(&mut core.items[item], &mut env) {
            let message = format!("{}: erro no script de entrada: {e}", rt.name);
            core.errors.push(message);
        }
    }

    /// Tells the subscribers of `block` that `item` entered or left it.
    fn notify(&mut self, block: BlockId, tag: u32, item: ItemId) {
        let entry = tag == MSG_ITEM_ENTERED;
        for k in 0..self.core.blocks[block.0].subscribers.len() {
            let (sub, on_entry, on_exit) = self.core.blocks[block.0].subscribers[k];
            if (entry && on_entry) || (!entry && on_exit) {
                self.core.ops.push_back(KernelOp::Message {
                    to: sub,
                    from: block,
                    tag,
                    payload: item_key(item),
                });
            }
        }
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
            .zip(&self.behaviors)
            .enumerate()
            .map(|(i, (rt, behavior))| BlockReport {
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
                custom: behavior
                    .custom_metrics(now)
                    .into_iter()
                    .chain(self.maint_metrics(BlockId(i), now))
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
