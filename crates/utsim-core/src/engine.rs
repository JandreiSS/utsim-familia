//! The simulation kernel: clock, future event list, item arena and flow resolution.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

use rand::Rng;
use slotmap::SlotMap;

use crate::Time;
use crate::block::{Behavior, BlockId, OutputRule, State};
use crate::dist::Dist;
use crate::item::{Item, ItemId, Value};
use crate::report::{BlockReport, RunReport, TallySummary};
use crate::rng::{self, SimRng};
use crate::stats::{StateTracker, Tally, TimeWeighted};

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub stop_time: Time,
    /// Statistics are cleared at this time (0 = no warm-up).
    pub warmup: Time,
    pub seed: u64,
    pub replication: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("erro de modelo em t={time}: {}", messages.join("; "))]
    Model { time: Time, messages: Vec<String> },
}

#[derive(Clone, Copy, Debug)]
enum EventKind {
    Block { block: BlockId, tag: u32, item: Option<ItemId> },
    EndWarmup,
}

struct Scheduled {
    time: Time,
    seq: u64,
    kind: EventKind,
}

// Min-heap on (time, seq): simultaneous events run in the order they were scheduled.
impl Ord for Scheduled {
    fn cmp(&self, other: &Self) -> Ordering {
        other.time.total_cmp(&self.time).then_with(|| other.seq.cmp(&self.seq))
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
        self.staytime = Tally::default();
        self.states.reset(t);
        self.extra.values_mut().for_each(|tally| *tally = Tally::default());
    }
}

struct BlockRuntime {
    name: String,
    kind: &'static str,
    inputs: Vec<BlockId>,
    outputs: Vec<BlockId>,
    rule: OutputRule,
    contents: Vec<ItemId>,
    outbox: VecDeque<Pending>,
    stats: BlockStats,
    rng: SimRng,
    rr_next: usize,
    dirty: bool,
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
}

impl Core {
    fn schedule_at(&mut self, time: Time, kind: EventKind) {
        self.seq += 1;
        self.events.push(Scheduled { time, seq: self.seq, kind });
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
    pub fn schedule(&mut self, delay: Time, tag: u32, item: Option<ItemId>) {
        let time = self.core.now + delay.max(0.0);
        self.core.schedule_at(time, EventKind::Block { block: self.me, tag, item });
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

    pub fn set_state(&mut self, state: State) {
        let now = self.core.now;
        self.core.blocks[self.me.0].stats.states.set(now, state);
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
        });
        self.core.blocks[self.me.0].contents.push(id);
        self.core.update_content(self.me);
        id
    }

    /// Removes the item from the model once the current hook returns.
    pub fn destroy(&mut self, id: ItemId) {
        self.core.doomed.push(id);
    }

    /// Marks the item as ready to leave, behind items released earlier.
    pub fn release(&mut self, id: ItemId) {
        let pending = self.route(id);
        self.core.blocks[self.me.0].outbox.push_back(pending);
        self.core.mark_dirty(self.me);
    }

    /// Marks the item as ready to leave, ahead of items released earlier.
    pub fn release_front(&mut self, id: ItemId) {
        let pending = self.route(id);
        self.core.blocks[self.me.0].outbox.push_front(pending);
        self.core.mark_dirty(self.me);
    }

    /// Adds an observation to a named per-block tally (shown in the report).
    pub fn record(&mut self, name: &str, value: f64) {
        let extra = &mut self.core.blocks[self.me.0].stats.extra;
        extra.entry(name.to_string()).or_default().record(value);
    }

    /// Reports a modelling error; the run stops after the current event.
    pub fn error(&mut self, message: impl Into<String>) {
        let message = format!("{}: {}", self.name(), message.into());
        self.core.errors.push(message);
    }

    fn route(&mut self, id: ItemId) -> Pending {
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
            contents: Vec::new(),
            outbox: VecDeque::new(),
            stats: BlockStats::default(),
            rng: rng::stream(self.config.seed, self.config.replication, id.0 as u64),
            rr_next: 0,
            dirty: false,
        });
        self.behaviors.push(behavior);
        id
    }

    /// Connects an output port of `from` to an input port of `to`.
    /// Port numbers follow connection order, which is also the pull priority.
    pub fn connect(&mut self, from: BlockId, to: BlockId) {
        self.core.blocks[from.0].outputs.push(to);
        self.core.blocks[to.0].inputs.push(from);
    }

    pub fn block_id(&self, name: &str) -> Option<BlockId> {
        self.core.blocks.iter().position(|b| b.name == name).map(BlockId)
    }

    pub fn now(&self) -> Time {
        self.core.now
    }

    pub fn run(&mut self) -> Result<RunReport, SimError> {
        self.start()?;
        let stop = self.config.stop_time;
        while self.core.events.peek().is_some_and(|ev| ev.time <= stop) {
            let ev = self.core.events.pop().expect("peeked");
            self.core.now = ev.time;
            self.core.events_processed += 1;
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
            let Simulation { core, behaviors, .. } = self;
            behaviors[b].reset(&mut Ctx { core, me });
            self.flush_doomed();
        }
        if self.config.warmup > 0.0 {
            self.core.schedule_at(self.config.warmup, EventKind::EndWarmup);
        }
        self.resolve();
        self.check()
    }

    fn check(&mut self) -> Result<(), SimError> {
        if self.core.errors.is_empty() {
            Ok(())
        } else {
            Err(SimError::Model { time: self.core.now, messages: std::mem::take(&mut self.core.errors) })
        }
    }

    fn dispatch(&mut self, kind: EventKind) {
        match kind {
            EventKind::Block { block, tag, item } => {
                if item.is_some_and(|id| !self.core.items.contains_key(id)) {
                    return;
                }
                let Simulation { core, behaviors, .. } = self;
                behaviors[block.0].on_event(&mut Ctx { core, me: block }, tag, item);
                self.flush_doomed();
            }
            EventKind::EndWarmup => {
                let now = self.core.now;
                self.core.blocks.iter_mut().for_each(|rt| rt.stats.reset(now));
            }
        }
    }

    /// Moves released items until nothing else can move at the current instant.
    fn resolve(&mut self) {
        while let Some(b) = self.core.dirty.pop_front() {
            self.core.blocks[b.0].dirty = false;
            self.try_send(b);
        }
    }

    fn try_send(&mut self, from: BlockId) {
        let mut i = 0;
        while i < self.core.blocks[from.0].outbox.len() {
            let pending = self.core.blocks[from.0].outbox[i];
            match self.find_destination(from, pending) {
                Some(to) => {
                    self.core.blocks[from.0].outbox.remove(i);
                    self.transfer(pending.item, from, to);
                }
                None => i += 1,
            }
        }
    }

    fn find_destination(&self, from: BlockId, pending: Pending) -> Option<BlockId> {
        let outputs = &self.core.blocks[from.0].outputs;
        let item = &self.core.items[pending.item];
        let open = |to: &BlockId| self.behaviors[to.0].accepts(item, self.core.blocks[to.0].contents.len());
        match pending.port {
            Some(port) => outputs.get(port).copied().filter(open),
            None => outputs.iter().copied().find(open),
        }
    }

    fn transfer(&mut self, item: ItemId, from: BlockId, to: BlockId) {
        let now = self.core.now;

        let entered = self.core.items[item].entered_at;
        let rt = &mut self.core.blocks[from.0];
        if let Some(pos) = rt.contents.iter().position(|&x| x == item) {
            rt.contents.remove(pos);
        }
        rt.stats.output += 1;
        rt.stats.staytime.record(now - entered);
        self.core.update_content(from);
        {
            let Simulation { core, behaviors, .. } = self;
            behaviors[from.0].on_exit(&mut Ctx { core, me: from }, item);
        }

        let it = &mut self.core.items[item];
        it.location = Some(to);
        it.entered_at = now;
        let rt = &mut self.core.blocks[to.0];
        rt.contents.push(item);
        rt.stats.input += 1;
        self.core.update_content(to);
        {
            let Simulation { core, behaviors, .. } = self;
            behaviors[to.0].on_entry(&mut Ctx { core, me: to }, item);
        }
        self.flush_doomed();

        // `from` has room again: let its upstream blocks retry, in input-port order.
        for k in 0..self.core.blocks[from.0].inputs.len() {
            let up = self.core.blocks[from.0].inputs[k];
            self.core.mark_dirty(up);
        }
    }

    fn flush_doomed(&mut self) {
        while let Some(id) = self.core.doomed.pop() {
            let Some(item) = self.core.items.remove(id) else { continue };
            if let Some(b) = item.location {
                let rt = &mut self.core.blocks[b.0];
                rt.contents.retain(|&x| x != id);
                rt.outbox.retain(|p| p.item != id);
                self.core.update_content(b);
            }
        }
    }

    pub fn report(&self) -> RunReport {
        let now = self.core.now;
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
                staytime: TallySummary::from(&rt.stats.staytime),
                states: rt
                    .stats
                    .states
                    .fractions(now)
                    .into_iter()
                    .map(|(s, f)| (s.name().to_string(), f))
                    .collect(),
                extra: rt.stats.extra.iter().map(|(k, t)| (k.clone(), TallySummary::from(t))).collect(),
            })
            .collect();
        RunReport {
            replication: self.config.replication,
            seed: self.config.seed,
            stats_from: if now >= self.config.warmup { self.config.warmup } else { 0.0 },
            end_time: now,
            events: self.core.events_processed,
            blocks,
        }
    }
}
