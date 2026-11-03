//! Critical path observed in the simulation itself.
//!
//! Runs the whole model (its normal replications) with an observer on the trace
//! ([`TraceEvent`]) and, for every unit that leaves the selection through one of its final
//! objects, walks back along what really held it:
//!
//! - in each object, the time from entering to leaving, split into waiting, work and
//!   blocking with the object's states (for objects that hold one item at a time:
//!   processors with capacity 1, combiners and separators) or by the object's type
//!   (queues are waiting, conveyors work for `length / speed` and wait the rest);
//! - transporter trips and timed transfers are activities of their own, named like the
//!   network's link activities ([`transport_name`], [`transfer_name`]); the wait for the
//!   transporter (its empty trip and the loading included) stays in the origin object
//!   (state `waiting_transport`, counted as blocking), so the observed trip is the loaded
//!   trip plus the unloading;
//! - in a `Combiner`, the critical input is the item of the set that arrived last (the one
//!   that really held the assembly); the walk continues along that item;
//! - an item that came out of a `Separator` (unpacked or a `Split` copy) continues along
//!   the item that entered the separator.
//!
//! The walk stops when the previous stop of the item is outside the selection. The total
//! time of a unit goes from its entry into the first selected object of the chain to its
//! exit from the final object, so it includes the work of both ends (a `SpanDef` from `A`
//! to `B` measures from leaving `A` to entering `B` instead). Units that finish before
//! the warm-up are left out. Only the trace of the selected objects and of the mobile
//! resources is kept, in memory, one replication at a time per thread.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::Serialize;
use utsim_core::stats::{Histogram, Summary};
use utsim_core::{Observer, State, TraceEvent};
use utsim_model::{Model, ObjectKind};

use crate::PertError;
use crate::math::quantile_sorted;
use crate::monte_carlo::PERCENTILES;
use crate::network::{ActivityNetwork, transfer_name, transport_name};

#[derive(Clone, Debug)]
pub struct ObservedOptions {
    /// Replications to run (default: the model's `run.replications`).
    pub replications: Option<u32>,
    pub deadline: Option<f64>,
    /// Histogram bins of the total time.
    pub bins: usize,
    /// Unit paths kept as examples per replication.
    pub examples: usize,
}

impl Default for ObservedOptions {
    fn default() -> Self {
        ObservedOptions {
            replications: None,
            deadline: None,
            bins: 20,
            examples: 3,
        }
    }
}

/// One stop of a unit's critical chain.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObservedStep {
    pub activity: String,
    pub start: f64,
    pub end: f64,
    pub wait: f64,
    pub work: f64,
    pub blocked: f64,
}

/// Critical chain of one unit.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UnitPath {
    /// Trace id of the item that left the selection.
    pub item: u64,
    pub start: f64,
    pub end: f64,
    pub steps: Vec<ObservedStep>,
}

impl UnitPath {
    pub fn total(&self) -> f64 {
        self.end - self.start
    }
}

/// Sums over the units of one replication for one activity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Accum {
    /// Units whose critical chain passes through the activity.
    pub critical: usize,
    pub wait: f64,
    pub work: f64,
    pub blocked: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ObservedReplication {
    pub replication: u32,
    /// Total time of every unit, in finishing order.
    pub totals: Vec<f64>,
    pub activities: BTreeMap<String, Accum>,
    /// Critical chains (activity names) and how many units followed each.
    pub paths: BTreeMap<Vec<String>, usize>,
    pub examples: Vec<UnitPath>,
}

impl ObservedReplication {
    pub fn units(&self) -> usize {
        self.totals.len()
    }

    pub fn mean(&self) -> f64 {
        self.totals.iter().sum::<f64>() / self.totals.len() as f64
    }
}

/// Observed figures of one activity, across replications.
#[derive(Clone, Debug, Serialize)]
pub struct ObservedActivity {
    pub name: String,
    /// Fraction of the units whose chain passes through it: mean of the replications with
    /// the 95% interval.
    pub criticality: Summary,
    /// The same fraction over all units together.
    pub pooled_criticality: f64,
    /// Mean time per unit spent on the chain in this activity (zero when not on it).
    pub wait: f64,
    pub work: f64,
    pub blocked: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ObservedDeadline {
    pub deadline: f64,
    /// Fraction of units within the deadline, across replications.
    pub fraction: Summary,
    pub pooled: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ObservedResult {
    pub replications: Vec<ObservedReplication>,
    pub units: usize,
    /// Mean total time: one value per replication, with the 95% interval between them.
    pub total: Summary,
    /// Every unit together (spread of the total time).
    pub pooled: Summary,
    pub percentiles: Vec<(f64, f64)>,
    pub histogram: Option<Histogram>,
    pub deadline: Option<ObservedDeadline>,
    /// Network activities first (in topological order), then any other stop observed.
    pub activities: Vec<ObservedActivity>,
    /// Chains and the fraction of all units that followed each, most frequent first.
    pub paths: Vec<(Vec<String>, f64)>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Ignore,
    Source,
    Sink,
    Queue,
    /// Holds one item at a time: states tell waiting, work and blocking apart.
    Single,
    /// Minimum travel time of a conveyor.
    Conveyor(f64),
    /// Everything counts as work (racks, processors with capacity > 1).
    Work,
    /// Mobile resource carrying items.
    Carrier,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Block(usize),
    Transit(usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum End {
    Open,
    Left,
    Packed,
    Destroyed,
}

#[derive(Clone, Debug)]
struct Visit {
    place: Place,
    from: Option<usize>,
    enter: f64,
    exit: f64,
    end: End,
    /// The visit of the item it came from (separator copies and unpacked items).
    parent: Option<(u64, usize)>,
}

/// What every replication needs to know about the blocks.
struct Setup {
    names: Vec<String>,
    role: Vec<Role>,
    selected: Vec<bool>,
    combiner: Vec<bool>,
    separator: Vec<bool>,
    terminal: Vec<bool>,
}

impl Setup {
    fn new(model: &Model, net: &ActivityNetwork) -> Result<Self, PertError> {
        let n = model.objects.len();
        let names: Vec<String> = model.objects.iter().map(|o| o.name.clone()).collect();
        let index = |name: &str| {
            names
                .iter()
                .position(|x| x == name)
                .ok_or_else(|| PertError::UnknownObject(name.to_string()))
        };
        let mut selected = vec![false; n];
        for o in &net.objects {
            selected[index(o)?] = true;
        }
        let mut has_succ = vec![false; n];
        for (a, _) in &net.links {
            has_succ[index(a)?] = true;
        }
        let terminal: Vec<bool> = (0..n).map(|i| selected[i] && !has_succ[i]).collect();
        if !terminal.iter().any(|&t| t) {
            return Err(PertError::Invalid(
                "a seleção não tem objeto final (todos têm saída para dentro dela)".into(),
            ));
        }
        let mut role = vec![Role::Ignore; n];
        let mut combiner = vec![false; n];
        let mut separator = vec![false; n];
        for (i, o) in model.objects.iter().enumerate() {
            role[i] = match &o.kind {
                ObjectKind::Transporter { .. } => Role::Carrier,
                _ if !selected[i] => Role::Ignore,
                ObjectKind::Source { .. } => Role::Source,
                ObjectKind::Sink => Role::Sink,
                ObjectKind::Queue { .. } => Role::Queue,
                ObjectKind::Processor { capacity: 1, .. } => Role::Single,
                ObjectKind::Processor { .. } | ObjectKind::Rack { .. } => Role::Work,
                ObjectKind::Combiner { .. } => {
                    combiner[i] = true;
                    Role::Single
                }
                ObjectKind::Separator { .. } => {
                    separator[i] = true;
                    Role::Single
                }
                ObjectKind::Conveyor { length, speed, .. } => Role::Conveyor(length / speed),
                ObjectKind::Dispatcher { .. } => Role::Ignore,
            };
        }
        Ok(Setup {
            names,
            role,
            selected,
            combiner,
            separator,
            terminal,
        })
    }

    fn role(&self, b: usize) -> Role {
        self.role.get(b).copied().unwrap_or(Role::Ignore)
    }

    fn is(&self, flags: &[bool], b: usize) -> bool {
        flags.get(b).copied().unwrap_or(false)
    }
}

/// Trace of one replication, restricted to what the walk needs.
struct Collector {
    setup: Arc<Setup>,
    items: HashMap<u64, Vec<Visit>>,
    states: Vec<Vec<(f64, State)>>,
    /// Visits of each combiner, in entering order.
    at_combiner: Vec<Vec<(u64, usize)>>,
    /// Item being worked by each separator.
    current: Vec<Option<(u64, usize)>>,
}

impl Collector {
    fn new(setup: Arc<Setup>) -> Self {
        let n = setup.names.len();
        Collector {
            setup,
            items: HashMap::new(),
            states: vec![Vec::new(); n],
            at_combiner: vec![Vec::new(); n],
            current: vec![None; n],
        }
    }

    fn close(&mut self, item: u64, t: f64, end: End) {
        if let Some(v) = self.items.get_mut(&item).and_then(|v| v.last_mut()) {
            if v.end == End::Open {
                v.exit = t;
                v.end = end;
            }
        }
    }

    fn open(
        &mut self,
        item: u64,
        place: Place,
        from: Option<usize>,
        t: f64,
        parent: Option<(u64, usize)>,
    ) {
        let visits = self.items.entry(item).or_default();
        let k = visits.len();
        visits.push(Visit {
            place,
            from,
            enter: t,
            exit: f64::INFINITY,
            end: End::Open,
            parent: parent.filter(|p| p.0 != item),
        });
        if let Place::Block(b) = place {
            if self.setup.is(&self.setup.combiner, b) {
                self.at_combiner[b].push((item, k));
            }
            if self.setup.is(&self.setup.separator, b) {
                self.current[b] = Some((item, k));
            }
        }
    }

    fn relevant(&self, b: usize) -> bool {
        self.setup.role(b) != Role::Ignore
    }

    fn on(&mut self, t: f64, event: &TraceEvent) {
        match *event {
            TraceEvent::Created { item, block } if self.relevant(block) => {
                let parent = if self.setup.is(&self.setup.separator, block) {
                    self.current[block]
                } else {
                    None
                };
                self.open(item, Place::Block(block), None, t, parent);
            }
            TraceEvent::Moved { item, from, to } => {
                self.close(item, t, End::Left);
                if self.relevant(to) {
                    self.open(item, Place::Block(to), Some(from), t, None);
                }
            }
            TraceEvent::Transit { item, from, to, .. } => {
                self.close(item, t, End::Left);
                if self.relevant(from) || self.relevant(to) {
                    self.open(item, Place::Transit(from, to), Some(from), t, None);
                }
            }
            TraceEvent::Packed { item, .. } => self.close(item, t, End::Packed),
            TraceEvent::Unpacked { item, block } if self.relevant(block) => {
                let parent = self.current.get(block).copied().flatten();
                self.open(item, Place::Block(block), None, t, parent);
            }
            TraceEvent::Destroyed { item } => self.close(item, t, End::Destroyed),
            TraceEvent::State { block, state } if self.setup.role(block) == Role::Single => {
                self.states[block].push((t, state));
            }
            _ => {}
        }
    }

    fn visit(&self, item: u64, k: usize) -> &Visit {
        &self.items[&item][k]
    }

    /// Item of the combiner set of `(item, k)` that entered last.
    fn critical_member(&self, b: usize, item: u64, k: usize) -> (u64, usize) {
        let x = self.visit(item, k);
        let list = &self.at_combiner[b];
        let upto = list.partition_point(|&(i, j)| self.visit(i, j).enter <= x.exit);
        let mut best = (item, k);
        let mut best_enter = x.enter;
        let mut misses = 0;
        for &(i, j) in list[..upto].iter().rev() {
            let m = self.visit(i, j);
            if (i, j) == (item, k) {
                continue;
            }
            if m.enter < x.exit && m.exit > x.enter {
                misses = 0;
                if m.enter > best_enter {
                    best = (i, j);
                    best_enter = m.enter;
                }
            } else if m.exit <= x.enter {
                misses += 1;
                if misses > 64 {
                    break;
                }
            }
        }
        best
    }

    fn in_selection(&self, prev: &Visit, next: Option<usize>) -> bool {
        let s = &self.setup;
        match prev.place {
            Place::Block(b) if s.is(&s.selected, b) => true,
            Place::Block(b) if s.role(b) == Role::Carrier => {
                prev.from.is_some_and(|f| s.is(&s.selected, f))
                    && next.is_some_and(|n| s.is(&s.selected, n))
            }
            Place::Transit(f, t) => s.is(&s.selected, f) && s.is(&s.selected, t),
            _ => false,
        }
    }

    /// Waiting, work and blocking of a block holding one item at a time, from its states.
    fn split_by_state(&self, b: usize, start: f64, end: f64) -> (f64, f64, f64) {
        let states = &self.states[b];
        let i = states.partition_point(|s| s.0 <= start);
        let mut state = if i > 0 { states[i - 1].1 } else { State::Idle };
        let (mut wait, mut work, mut blocked) = (0.0, 0.0, 0.0);
        let mut t = start;
        let mut add = |state: State, dt: f64| match state {
            State::Setup | State::Processing => work += dt,
            State::Blocked | State::WaitingTransport => blocked += dt,
            _ => wait += dt,
        };
        for &(ts, s) in &states[i..] {
            if ts >= end {
                break;
            }
            add(state, ts - t);
            state = s;
            t = ts;
        }
        add(state, end - t);
        (wait, work, blocked)
    }

    fn step(&self, v: &Visit, start: f64, end: f64, next: Option<usize>) -> ObservedStep {
        let s = &self.setup;
        let name = |b: Option<usize>| {
            b.and_then(|b| s.names.get(b))
                .map_or("?", String::as_str)
                .to_string()
        };
        let len = (end - start).max(0.0);
        let (activity, (wait, work, blocked)) = match v.place {
            Place::Transit(f, t) => (
                transfer_name(&name(Some(f)), &name(Some(t))),
                (0.0, len, 0.0),
            ),
            Place::Block(b) => match s.role(b) {
                Role::Carrier => (transport_name(&name(v.from), &name(next)), (0.0, len, 0.0)),
                Role::Queue => (name(Some(b)), (len, 0.0, 0.0)),
                Role::Source => (name(Some(b)), (0.0, 0.0, len)),
                Role::Single => (name(Some(b)), self.split_by_state(b, start, end)),
                Role::Conveyor(min) => (name(Some(b)), (len - len.min(min), len.min(min), 0.0)),
                Role::Sink | Role::Work | Role::Ignore => (name(Some(b)), (0.0, len, 0.0)),
            },
        };
        ObservedStep {
            activity,
            start,
            end,
            wait,
            work,
            blocked,
        }
    }

    /// Critical chain of the unit that ends with visit `k` of `item`.
    fn walk(&self, item: u64, k: usize) -> UnitPath {
        let end = self.visit(item, k).exit;
        let mut cursor = end;
        let mut steps = Vec::new();
        let (mut it, mut idx) = (item, k);
        let mut next: Option<usize> = None;
        for _ in 0..1_000_000 {
            let (mut pi, mut pk) = (it, idx);
            while let Some(p) = self.visit(pi, pk).parent {
                (pi, pk) = p;
            }
            if let Place::Block(b) = self.visit(pi, pk).place {
                if self.setup.is(&self.setup.combiner, b) {
                    (pi, pk) = self.critical_member(b, pi, pk);
                }
            }
            let v = self.visit(pi, pk);
            let start = v.enter.min(cursor);
            steps.push(self.step(v, start, cursor, next));
            cursor = start;
            next = match v.place {
                Place::Block(b) => Some(b),
                Place::Transit(..) => None,
            };
            if pk == 0 || !self.in_selection(self.visit(pi, pk - 1), next) {
                break;
            }
            (it, idx) = (pi, pk - 1);
        }
        steps.reverse();
        UnitPath {
            item,
            start: cursor,
            end,
            steps,
        }
    }

    /// Every unit that left a final object at or after `warmup`, in finishing order.
    fn units(&self, warmup: f64) -> Vec<UnitPath> {
        let s = &self.setup;
        let mut ends: Vec<(f64, u64, usize)> = Vec::new();
        for (&item, visits) in &self.items {
            for (k, v) in visits.iter().enumerate() {
                let Place::Block(b) = v.place else { continue };
                let finished = match v.end {
                    End::Left => true,
                    End::Destroyed => !s.is(&s.combiner, b),
                    End::Open | End::Packed => false,
                };
                if s.is(&s.terminal, b) && finished && v.exit >= warmup {
                    ends.push((v.exit, item, k));
                }
            }
        }
        ends.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        ends.into_iter().map(|(_, i, k)| self.walk(i, k)).collect()
    }
}

struct Shared(Arc<Mutex<Collector>>);

impl Observer for Shared {
    fn on_event(&mut self, t: f64, event: &TraceEvent) {
        self.0.lock().expect("collector lock").on(t, event);
    }
}

fn replicate(
    model: &Model,
    setup: &Arc<Setup>,
    replication: u32,
    examples: usize,
) -> Result<ObservedReplication, PertError> {
    let shared = Arc::new(Mutex::new(Collector::new(setup.clone())));
    {
        let mut sim = model.build(replication);
        sim.set_observer(Box::new(Shared(shared.clone())));
        model.drive(&mut sim)?;
    }
    let collector = Arc::try_unwrap(shared)
        .ok()
        .expect("the simulation was dropped")
        .into_inner()
        .expect("collector lock");
    let units = collector.units(model.run.warmup);
    let mut rep = ObservedReplication {
        replication,
        totals: Vec::with_capacity(units.len()),
        activities: BTreeMap::new(),
        paths: BTreeMap::new(),
        examples: units.iter().take(examples).cloned().collect(),
    };
    for u in &units {
        rep.totals.push(u.total());
        let mut seen: Vec<&str> = Vec::new();
        for st in &u.steps {
            let a = rep.activities.entry(st.activity.clone()).or_default();
            a.wait += st.wait;
            a.work += st.work;
            a.blocked += st.blocked;
            if !seen.contains(&st.activity.as_str()) {
                seen.push(&st.activity);
                a.critical += 1;
            }
        }
        let chain: Vec<String> = u.steps.iter().map(|s| s.activity.clone()).collect();
        *rep.paths.entry(chain).or_default() += 1;
    }
    Ok(rep)
}

/// Runs the model and follows the critical chain of every unit of the selection.
pub fn run(
    model: &Model,
    net: &ActivityNetwork,
    options: &ObservedOptions,
) -> Result<ObservedResult, PertError> {
    model.validate()?;
    let setup = Arc::new(Setup::new(model, net)?);
    let reps = options
        .replications
        .unwrap_or(model.run.replications)
        .max(1);
    let one = |r: u32| replicate(model, &setup, r, options.examples);
    #[cfg(feature = "parallel")]
    let replications: Vec<ObservedReplication> = (0..reps)
        .into_par_iter()
        .map(one)
        .collect::<Result<_, _>>()?;
    #[cfg(not(feature = "parallel"))]
    let replications: Vec<ObservedReplication> = (0..reps).map(one).collect::<Result<_, _>>()?;
    Ok(aggregate(net, replications, options))
}

fn aggregate(
    net: &ActivityNetwork,
    replications: Vec<ObservedReplication>,
    options: &ObservedOptions,
) -> ObservedResult {
    let mut warnings = Vec::new();
    let empty: Vec<u32> = replications
        .iter()
        .filter(|r| r.units() == 0)
        .map(|r| r.replication)
        .collect();
    if !empty.is_empty() {
        warnings.push(format!(
            "nenhuma unidade completou o trecho depois do aquecimento nas replicações {:?}",
            empty
        ));
    }
    let used: Vec<&ObservedReplication> = replications.iter().filter(|r| r.units() > 0).collect();
    let units: usize = used.iter().map(|r| r.units()).sum();
    let total = Summary::from_samples(&used.iter().map(|r| r.mean()).collect::<Vec<_>>());
    let mut all: Vec<f64> = used.iter().flat_map(|r| r.totals.iter().copied()).collect();
    let pooled = Summary::from_samples(&all);
    let histogram = Histogram::build(&all, options.bins);
    all.sort_by(f64::total_cmp);
    let percentiles = PERCENTILES
        .iter()
        .map(|&p| (p, quantile_sorted(&all, p)))
        .collect();
    let deadline = options.deadline.map(|d| {
        let within = |xs: &[f64]| xs.iter().filter(|&&x| x <= d).count() as f64 / xs.len() as f64;
        ObservedDeadline {
            deadline: d,
            fraction: Summary::from_samples(
                &used.iter().map(|r| within(&r.totals)).collect::<Vec<_>>(),
            ),
            pooled: if all.is_empty() {
                f64::NAN
            } else {
                within(&all)
            },
        }
    });

    let mut names: Vec<String> = net
        .order()
        .into_iter()
        .map(|v| &net.activities[v])
        .filter(|a| !a.is_milestone())
        .map(|a| a.name.clone())
        .collect();
    let mut extra: Vec<String> = used
        .iter()
        .flat_map(|r| r.activities.keys().cloned())
        .filter(|k| !names.contains(k))
        .collect();
    extra.sort();
    extra.dedup();
    names.extend(extra);
    let n_units = units.max(1) as f64;
    let activities = names
        .into_iter()
        .map(|name| {
            let get =
                |r: &ObservedReplication| r.activities.get(&name).copied().unwrap_or_default();
            let per_rep: Vec<f64> = used
                .iter()
                .map(|r| get(r).critical as f64 / r.units() as f64)
                .collect();
            let sum = used.iter().fold(Accum::default(), |s, r| {
                let a = get(r);
                Accum {
                    critical: s.critical + a.critical,
                    wait: s.wait + a.wait,
                    work: s.work + a.work,
                    blocked: s.blocked + a.blocked,
                }
            });
            ObservedActivity {
                name,
                criticality: Summary::from_samples(&per_rep),
                pooled_criticality: sum.critical as f64 / n_units,
                wait: sum.wait / n_units,
                work: sum.work / n_units,
                blocked: sum.blocked / n_units,
            }
        })
        .collect();
    let mut chains: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    for r in &used {
        for (p, c) in &r.paths {
            *chains.entry(p.clone()).or_default() += c;
        }
    }
    let mut paths: Vec<(Vec<String>, f64)> = chains
        .into_iter()
        .map(|(p, c)| (p, c as f64 / n_units))
        .collect();
    paths.sort_by(|a, b| b.1.total_cmp(&a.1));
    ObservedResult {
        replications,
        units,
        total,
        pooled,
        percentiles,
        histogram,
        deadline,
        activities,
        paths,
        warnings,
    }
}
