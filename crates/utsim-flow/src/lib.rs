//! Token flows (phase 6): logic drawn as a graph of activities. Tokens are not items: they are lightweight "threads of
//! logic" that wait, decide, hold resources and can carry an item reference. A flow
//! runs as one block of the simulation, listening to the physical objects through the
//! kernel (subscriptions) and acting on them (creating items, setting labels).
//!
//! The schema below is the contract; [`Flow`] is the engine that runs it.
//!
//! # Statistics recorded by a flow block (`cx.record`)
//! - `<atividade>.espera`: for `Delay`, `Acquire`, `WaitFor` and `Batch`, one
//!   observation per token with the time it spent in the activity. The tally count is
//!   the number of tokens that went through it.
//! - `<recurso>.ocupado`: one observation per released chunk of a resource, with the
//!   time those units were held. For unit holds, utilisation = `mean * count /
//!   (window * capacity)`.
//! - `<sink>.tempo_no_fluxo`: for each `Sink`, one observation per token with its
//!   lifetime in the flow. The count is the number of tokens that ended there.
//!
//! The kernel also receives `Ctx::trace_tokens` whenever an activity's token count
//! changes. The block state is `Processing` while any token is alive, `Idle` otherwise.
//! Breakdowns and shifts make no sense on a flow block: `on_stop`/`on_resume` are
//! deliberately ignored.
//!
//! # Execution model
//! A token moves through instantaneous activities (`Assign`, `Decide`, `Release`,
//! `CreateItem`, an `Acquire` with free units...) in the same instant, without
//! scheduling anything. It stops in `Delay` (a scheduled event), `Acquire` (FIFO queue
//! of the resource), `WaitFor` (FIFO queue of the activity) and `Batch`. Woken tokens
//! are queued in `ready` and run after the current token stops, still in the same
//! instant. A token that passes more than [`MAX_STEPS`] activities without ever
//! stopping is a zero-time loop and stops the run with an error.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use utsim_core::task::{MSG_DECISION, MSG_ITEM_ENTERED, MSG_ITEM_EXITED};
use utsim_core::trace::item_key;
use utsim_core::{Behavior, BlockId, Ctx, Dist, Item, ItemId, State, Time, Value};
use utsim_script::{Expr, ExprEnv, compile_expr};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowDef {
    pub name: String,
    /// Shared counting resources (e.g. "docas" with capacity 3).
    #[serde(default)]
    pub resources: Vec<ResourceDef>,
    pub activities: Vec<ActivityDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceDef {
    pub name: String,
    pub capacity: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActivityDef {
    pub name: String,
    pub kind: Activity,
    /// Where tokens go next. Most activities use one; `Decide` picks among several;
    /// `Sink` has none.
    #[serde(default)]
    pub next: Vec<String>,
}

/// Which object event a flow reacts to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockEvent {
    Entry,
    Exit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DecideRule {
    /// Random branch with relative weights, one per `next`.
    Probability(Vec<f64>),
    /// The branch number (1-based) is the value of this token label.
    ByLabel(String),
    /// One condition per `next`: the first true one wins; none true = last branch.
    Condition(Vec<String>),
    /// An external agent chooses the branch (reinforcement learning, a person):
    /// the simulation pauses with the values of `observation` (expressions).
    Agent { observation: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Activity {
    /// Creates tokens at random intervals.
    Source {
        interarrival: Dist,
        #[serde(default)]
        first_at_zero: bool,
        #[serde(default)]
        limit: Option<u64>,
    },
    /// Creates `count` tokens at each listed time.
    Schedule { times: Vec<(f64, u32)> },
    /// Creates a token, carrying the item, every time `event` happens at `block`.
    OnEvent { block: String, event: BlockEvent },
    /// Holds the token for a sampled time.
    Delay { time: Dist },
    /// Sends the token to one of `next`.
    Decide { rule: DecideRule },
    /// Sets labels from expressions: `name` sets a token label, `item.name` a label
    /// of the carried item.
    Assign { labels: BTreeMap<String, String> },
    /// Waits until `count` units of the resource are free, then holds them.
    Acquire {
        resource: String,
        #[serde(default = "one")]
        count: usize,
    },
    Release {
        resource: String,
        #[serde(default = "one")]
        count: usize,
    },
    /// Waits for the next `event` at `block`; the token then carries that item.
    WaitFor { block: String, event: BlockEvent },
    /// Creates an item inside object `into` (input `port`), carried by the token.
    CreateItem {
        into: String,
        #[serde(default)]
        port: usize,
        #[serde(default)]
        labels: BTreeMap<String, String>,
    },
    /// Collects `size` tokens and continues with one of them.
    Batch { size: usize },
    /// Ends the token.
    Sink,
}

fn one() -> usize {
    1
}

impl FlowDef {
    /// Structural checks; `objects` are the model's object names. Messages in Portuguese.
    pub fn validate(&self, objects: &HashSet<&str>) -> Vec<String> {
        let mut errors = Vec::new();
        let f = &self.name;
        let mut names = HashSet::new();
        for a in &self.activities {
            if !names.insert(a.name.as_str()) {
                errors.push(format!("{f}: atividade repetida '{}'", a.name));
            }
        }
        let mut resources: HashMap<&str, usize> = HashMap::new();
        for r in &self.resources {
            if resources.insert(r.name.as_str(), r.capacity).is_some() {
                errors.push(format!("{f}: recurso repetido '{}'", r.name));
            }
            if r.capacity == 0 {
                errors.push(format!("{f}: recurso '{}' com capacidade 0", r.name));
            }
        }
        for a in &self.activities {
            let at = format!("{f}.{}", a.name);
            for n in &a.next {
                if !names.contains(n.as_str()) {
                    errors.push(format!("{at}: próxima atividade '{n}' não existe"));
                }
            }
            let wants_next = !matches!(a.kind, Activity::Sink);
            if wants_next && a.next.is_empty() {
                errors.push(format!("{at}: precisa de uma próxima atividade (next)"));
            }
            match &a.kind {
                Activity::OnEvent { block, .. }
                | Activity::WaitFor { block, .. }
                | Activity::CreateItem { into: block, .. } => {
                    if !objects.contains(block.as_str()) {
                        errors.push(format!("{at}: objeto '{block}' não existe"));
                    }
                }
                Activity::Acquire { resource, count } | Activity::Release { resource, count } => {
                    match resources.get(resource.as_str()) {
                        None => errors.push(format!("{at}: recurso '{resource}' não existe")),
                        Some(cap) if count > cap => errors.push(format!(
                            "{at}: {count} unidades de '{resource}' é mais que a capacidade ({cap})"
                        )),
                        _ => {}
                    }
                    if *count == 0 {
                        errors.push(format!("{at}: count deve ser >= 1"));
                    }
                }
                Activity::Decide {
                    rule: DecideRule::Probability(w),
                } => {
                    if w.len() != a.next.len() {
                        errors.push(format!("{at}: um peso por próxima atividade"));
                    } else if w.iter().any(|x| x.is_nan() || *x < 0.0)
                        || w.iter().sum::<f64>() <= 0.0
                    {
                        errors.push(format!(
                            "{at}: os pesos devem ser não negativos e somar mais que zero"
                        ));
                    }
                }
                Activity::Decide {
                    rule: DecideRule::Condition(c),
                } if c.len() != a.next.len() => {
                    errors.push(format!("{at}: uma condição por próxima atividade"));
                }
                Activity::Schedule { times }
                    if times.iter().any(|(t, _)| t.is_nan() || *t < 0.0) =>
                {
                    errors.push(format!("{at}: os horários devem ser >= 0"));
                }
                Activity::Batch { size: 0 } => errors.push(format!("{at}: size deve ser >= 1")),
                Activity::Delay { time } => {
                    if let Err(e) = time.validate() {
                        errors.push(format!("{at}.time: {e}"));
                    }
                }
                Activity::Source {
                    interarrival,
                    limit,
                    ..
                } => match interarrival.validate() {
                    Err(e) => errors.push(format!("{at}.interarrival: {e}")),
                    // A zero (or vanishing) interval with no limit creates tokens forever
                    // at one instant, filling memory.
                    Ok(())
                        if limit.is_none()
                            && (interarrival.mean().is_nan() || interarrival.mean() <= 1e-9) =>
                    {
                        errors.push(format!(
                            "{at}: interarrival com média ({}) menor que 1e-9 exige `limit`",
                            interarrival.mean()
                        ));
                    }
                    Ok(()) => {}
                },
                _ => {}
            }
            errors.extend(expression_errors(a, &at));
        }
        errors
    }
}

/// Compile errors of the expressions in one activity (Portuguese messages).
fn expression_errors(a: &ActivityDef, at: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let check = |what: String, src: &str| -> Option<String> {
        compile_expr(src)
            .err()
            .map(|e| format!("{at}: expressão de {what} inválida ('{src}'): {e}"))
    };
    match &a.kind {
        Activity::Assign { labels } => {
            for (name, src) in labels {
                if name.is_empty() || name == "item." {
                    errors.push(format!("{at}: nome de rótulo vazio"));
                }
                errors.extend(check(format!("'{name}'"), src));
            }
        }
        Activity::CreateItem { labels, .. } => {
            for (name, src) in labels {
                errors.extend(check(format!("'{name}'"), src));
            }
        }
        Activity::Decide {
            rule: DecideRule::Condition(conds),
        } => {
            for (k, src) in conds.iter().enumerate() {
                errors.extend(check(format!("condição {}", k + 1), src));
            }
        }
        Activity::Decide {
            rule: DecideRule::Agent { observation },
        } => {
            for (k, src) in observation.iter().enumerate() {
                errors.extend(check(format!("observação {}", k + 1), src));
            }
            if a.next.len() < 2 {
                errors.push(format!("{at}: o agente precisa de ao menos 2 ramos (next)"));
            }
        }
        _ => {}
    }
    errors
}

// ---------------------------------------------------------------------------
// Compiled plan (immutable during a run)
// ---------------------------------------------------------------------------

/// Maximum activities one token may pass without stopping (zero-time loop guard).
pub const MAX_STEPS: usize = 10_000;

const TAG_SOURCE: u32 = 0;
const TAG_SCHEDULE: u32 = 1 << 30;
const TAG_DELAY: u32 = 2 << 30;
const TAG_KIND: u32 = 3 << 30;
const TAG_PAYLOAD: u32 = (1 << 30) - 1;

enum Target {
    Token(String),
    Item(String),
}

enum Rule {
    Probability(Vec<f64>),
    ByLabel(String),
    Condition(Vec<Expr>),
    /// Observation expressions; the token parks until the external agent answers.
    Agent(Vec<Expr>),
}

enum Op {
    Source {
        interarrival: Dist,
        first_at_zero: bool,
        limit: Option<u64>,
    },
    Schedule {
        times: Vec<(f64, u32)>,
    },
    OnEvent {
        block: BlockId,
        event: BlockEvent,
    },
    Delay(Dist),
    Decide(Rule),
    Assign(Vec<(Target, Expr)>),
    Acquire {
        res: usize,
        count: usize,
    },
    Release {
        res: usize,
        count: usize,
    },
    WaitFor {
        block: BlockId,
        event: BlockEvent,
    },
    CreateItem {
        into: BlockId,
        port: usize,
        labels: Vec<(String, Expr)>,
    },
    Batch(usize),
    Sink,
}

impl Op {
    /// Whether the time a token spends here is recorded as `<atividade>.espera`.
    fn waits(&self) -> bool {
        matches!(
            self,
            Op::Delay(_) | Op::Acquire { .. } | Op::WaitFor { .. } | Op::Batch(_)
        )
    }
}

struct Plan {
    ops: Vec<Op>,
    /// Indices of the `next` activities, per activity.
    next: Vec<Vec<usize>>,
    names: Vec<String>,
    /// Resource name and capacity.
    resources: Vec<(String, usize)>,
    /// Problems found while compiling (an unvalidated model): reported at reset.
    errors: Vec<String>,
}

impl Plan {
    fn build(def: &FlowDef, objects: &HashMap<String, BlockId>) -> Plan {
        let mut errors = Vec::new();
        let index: HashMap<&str, usize> = def
            .activities
            .iter()
            .enumerate()
            .map(|(i, a)| (a.name.as_str(), i))
            .collect();
        let resources: Vec<(String, usize)> = def
            .resources
            .iter()
            .map(|r| (r.name.clone(), r.capacity))
            .collect();
        let res_index = |name: &str| resources.iter().position(|(n, _)| n == name);
        let mut ops = Vec::new();
        let mut next = Vec::new();
        let mut names = Vec::new();
        for a in &def.activities {
            let at = format!("{}.{}", def.name, a.name);
            names.push(a.name.clone());
            next.push(
                a.next
                    .iter()
                    .filter_map(|n| {
                        let i = index.get(n.as_str()).copied();
                        if i.is_none() {
                            errors.push(format!("{at}: próxima atividade '{n}' não existe"));
                        }
                        i
                    })
                    .collect(),
            );
            let block = |name: &str, errors: &mut Vec<String>| {
                let b = objects.get(name).copied();
                if b.is_none() {
                    errors.push(format!("{at}: objeto '{name}' não existe"));
                }
                b.unwrap_or(BlockId(usize::MAX))
            };
            let compile = |src: &str, errors: &mut Vec<String>| match compile_expr(src) {
                Ok(e) => Some(e),
                Err(m) => {
                    errors.push(format!("{at}: expressão inválida ('{src}'): {m}"));
                    None
                }
            };
            let op = match &a.kind {
                Activity::Source {
                    interarrival,
                    first_at_zero,
                    limit,
                } => Op::Source {
                    interarrival: interarrival.clone(),
                    first_at_zero: *first_at_zero,
                    limit: *limit,
                },
                Activity::Schedule { times } => Op::Schedule {
                    times: times.clone(),
                },
                Activity::OnEvent { block: b, event } => Op::OnEvent {
                    block: block(b, &mut errors),
                    event: *event,
                },
                Activity::Delay { time } => Op::Delay(time.clone()),
                Activity::Decide { rule } => Op::Decide(match rule {
                    DecideRule::Probability(w) => Rule::Probability(w.clone()),
                    DecideRule::ByLabel(l) => Rule::ByLabel(l.clone()),
                    DecideRule::Condition(c) => Rule::Condition(
                        c.iter()
                            .filter_map(|s| compile(s, &mut errors))
                            .collect::<Vec<_>>(),
                    ),
                    DecideRule::Agent { observation } => Rule::Agent(
                        observation
                            .iter()
                            .filter_map(|s| compile(s, &mut errors))
                            .collect::<Vec<_>>(),
                    ),
                }),
                Activity::Assign { labels } => Op::Assign(
                    labels
                        .iter()
                        .filter_map(|(k, s)| {
                            let target = match k.strip_prefix("item.") {
                                Some(n) => Target::Item(n.to_string()),
                                None => Target::Token(k.clone()),
                            };
                            compile(s, &mut errors).map(|e| (target, e))
                        })
                        .collect(),
                ),
                Activity::Acquire { resource, count } | Activity::Release { resource, count } => {
                    let res = res_index(resource).unwrap_or_else(|| {
                        errors.push(format!("{at}: recurso '{resource}' não existe"));
                        0
                    });
                    if matches!(a.kind, Activity::Acquire { .. }) {
                        Op::Acquire { res, count: *count }
                    } else {
                        Op::Release { res, count: *count }
                    }
                }
                Activity::WaitFor { block: b, event } => Op::WaitFor {
                    block: block(b, &mut errors),
                    event: *event,
                },
                Activity::CreateItem { into, port, labels } => Op::CreateItem {
                    into: block(into, &mut errors),
                    port: *port,
                    labels: labels
                        .iter()
                        .filter_map(|(k, s)| compile(s, &mut errors).map(|e| (k.clone(), e)))
                        .collect(),
                },
                Activity::Batch { size } => Op::Batch((*size).max(1)),
                Activity::Sink => Op::Sink,
            };
            ops.push(op);
        }
        Plan {
            ops,
            next,
            names,
            resources,
            errors,
        }
    }

    fn first_next(&self, at: usize) -> Result<usize, String> {
        self.next[at]
            .first()
            .copied()
            .ok_or_else(|| "sem próxima atividade".to_string())
    }
}

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

struct Hold {
    res: usize,
    count: usize,
    since: Time,
}

struct Token {
    labels: BTreeMap<String, Value>,
    item: Option<ItemId>,
    holds: Vec<Hold>,
    /// Activity the token is in (or entering).
    activity: usize,
    /// When it entered `activity`.
    entered: Time,
    /// When the token was created.
    born: Time,
}

enum Step {
    /// Left the activity; continue at this one.
    Go(usize),
    /// Stopped (delay, queue); something will wake it.
    Wait,
    /// The token ended.
    End,
}

#[derive(Default)]
struct Rt {
    tokens: BTreeMap<u32, Token>,
    next_id: u32,
    /// Tokens now in each activity.
    counts: Vec<u32>,
    free: Vec<usize>,
    /// FIFO queues of tokens waiting for a resource: (token, units, activity).
    res_queue: Vec<VecDeque<(u32, usize, usize)>>,
    /// FIFO queues of tokens waiting in `WaitFor` and `Batch` activities.
    waiters: Vec<VecDeque<u32>>,
    /// Tokens parked at an `Agent` decision, oldest first: (token, activity). The
    /// kernel answers the block's requests in order, so one FIFO serves all of them.
    agent_wait: VecDeque<(u32, usize)>,
    /// Tokens created by each `Source`.
    generated: Vec<u64>,
    /// `(activity, count)` for each timer event of a `Schedule`.
    sched: Vec<(usize, u32)>,
    /// Tokens woken or created, to enter `.1`, run once the current token stops.
    ready: VecDeque<(u32, usize)>,
    busy: bool,
}

impl Rt {
    fn reset(&mut self, plan: &Plan) {
        *self = Rt {
            counts: vec![0; plan.ops.len()],
            free: plan.resources.iter().map(|(_, c)| *c).collect(),
            res_queue: vec![VecDeque::new(); plan.resources.len()],
            waiters: vec![VecDeque::new(); plan.ops.len()],
            generated: vec![0; plan.ops.len()],
            ..Rt::default()
        };
    }

    fn new_token(&mut self, now: Time, item: Option<ItemId>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.tokens.insert(
            id,
            Token {
                labels: BTreeMap::new(),
                item,
                holds: Vec::new(),
                activity: 0,
                entered: now,
                born: now,
            },
        );
        id
    }

    fn count_in(&mut self, cx: &mut Ctx, at: usize) {
        self.counts[at] += 1;
        cx.trace_tokens(at, self.counts[at]);
    }

    /// The token leaves `at`: updates the count and records the wait.
    fn leave(&mut self, plan: &Plan, cx: &mut Ctx, tid: u32, at: usize) {
        self.counts[at] -= 1;
        cx.trace_tokens(at, self.counts[at]);
        if plan.ops[at].waits() {
            if let Some(t) = self.tokens.get(&tid) {
                cx.record(&format!("{}.espera", plan.names[at]), cx.now() - t.entered);
            }
        }
    }

    /// A waiting token continues: it leaves its activity and goes on to the next one.
    fn resume(&mut self, plan: &Plan, cx: &mut Ctx, tid: u32, at: usize) {
        self.leave(plan, cx, tid, at);
        match plan.first_next(at) {
            Ok(n) => self.ready.push_back((tid, n)),
            Err(m) => {
                cx.error(format!("atividade '{}': {m}", plan.names[at]));
                self.tokens.remove(&tid);
            }
        }
    }

    /// Runs queued tokens until none is left, then refreshes the block state.
    fn drive(&mut self, plan: &Plan, cx: &mut Ctx) {
        while let Some((tid, at)) = self.ready.pop_front() {
            self.run(plan, cx, tid, at);
        }
        let want = if self.tokens.is_empty() {
            State::Idle
        } else {
            State::Processing
        };
        let busy = want == State::Processing;
        if busy != self.busy {
            self.busy = busy;
            cx.set_state(want);
        }
    }

    /// Moves a token through instantaneous activities until it stops.
    fn run(&mut self, plan: &Plan, cx: &mut Ctx, tid: u32, mut at: usize) {
        for _ in 0..MAX_STEPS {
            match self.enter(plan, cx, tid, at) {
                Ok(Step::Go(n)) => at = n,
                Ok(Step::Wait) | Ok(Step::End) => return,
                Err(m) => {
                    cx.error(format!("atividade '{}': {m}", plan.names[at]));
                    self.tokens.remove(&tid);
                    return;
                }
            }
        }
        cx.error(format!(
            "laço de tempo zero: um token passou por {MAX_STEPS} atividades sem parar (última: '{}')",
            plan.names[at]
        ));
        self.tokens.remove(&tid);
    }

    fn eval(&self, cx: &mut Ctx, tid: u32, e: &Expr) -> Result<Value, String> {
        let token = self.tokens.get(&tid).ok_or("token inexistente")?;
        let item_labels = token
            .item
            .and_then(|i| cx.item_by_key(item_key(i)))
            .map(|i| cx.item(i).labels.clone());
        let now = cx.now();
        let mut env = ExprEnv {
            now,
            rng: cx.rng(),
            token: &token.labels,
            item: item_labels.as_ref(),
        };
        e.eval(&mut env)
    }

    fn eval_bool(&self, cx: &mut Ctx, tid: u32, e: &Expr) -> Result<bool, String> {
        let token = self.tokens.get(&tid).ok_or("token inexistente")?;
        let item_labels = token
            .item
            .and_then(|i| cx.item_by_key(item_key(i)))
            .map(|i| cx.item(i).labels.clone());
        let now = cx.now();
        let mut env = ExprEnv {
            now,
            rng: cx.rng(),
            token: &token.labels,
            item: item_labels.as_ref(),
        };
        e.eval_bool(&mut env)
    }

    fn enter(&mut self, plan: &Plan, cx: &mut Ctx, tid: u32, at: usize) -> Result<Step, String> {
        let now = cx.now();
        self.count_in(cx, at);
        {
            let t = self.tokens.get_mut(&tid).ok_or("token inexistente")?;
            t.activity = at;
            t.entered = now;
        }
        match &plan.ops[at] {
            Op::Source { .. } | Op::Schedule { .. } | Op::OnEvent { .. } => {
                let n = plan.first_next(at)?;
                self.leave(plan, cx, tid, at);
                Ok(Step::Go(n))
            }
            Op::Delay(dist) => {
                if tid > TAG_PAYLOAD {
                    return Err("tokens demais para agendar".into());
                }
                let d = cx.sample_duration(dist);
                cx.schedule(d, TAG_DELAY | tid, None);
                Ok(Step::Wait)
            }
            Op::Decide(rule) => {
                let n = plan.next[at].len();
                let k = match rule {
                    Rule::Probability(w) => {
                        let total: f64 = w.iter().sum();
                        let u = cx.sample(&Dist::Uniform { min: 0.0, max: 1.0 }) * total;
                        let mut acc = 0.0;
                        let mut pick = n - 1;
                        for (i, x) in w.iter().enumerate() {
                            acc += x;
                            if u < acc {
                                pick = i;
                                break;
                            }
                        }
                        pick
                    }
                    Rule::ByLabel(label) => {
                        let v = self.tokens[&tid]
                            .labels
                            .get(label)
                            .ok_or_else(|| format!("o token não tem o rótulo '{label}'"))?
                            .as_num()
                            .ok_or_else(|| format!("o rótulo '{label}' não é numérico"))?;
                        if v.fract() != 0.0 || v < 1.0 || v > n as f64 {
                            return Err(format!(
                                "rótulo '{label}' = {v} não escolhe um dos {n} ramos (1 a {n})"
                            ));
                        }
                        v as usize - 1
                    }
                    Rule::Condition(conds) => {
                        let mut pick = n - 1;
                        for (i, c) in conds.iter().enumerate() {
                            if self.eval_bool(cx, tid, c)? {
                                pick = i;
                                break;
                            }
                        }
                        pick
                    }
                    Rule::Agent(obs) => {
                        let mut values = Vec::with_capacity(obs.len());
                        for e in obs {
                            let v = self.eval(cx, tid, e)?;
                            values.push(
                                v.as_num()
                                    .ok_or("a observação do agente deve ser numérica")?,
                            );
                        }
                        cx.request_decision(n, values);
                        self.agent_wait.push_back((tid, at));
                        return Ok(Step::Wait);
                    }
                };
                let next = *plan.next[at].get(k).ok_or("ramo de saída inexistente")?;
                self.leave(plan, cx, tid, at);
                Ok(Step::Go(next))
            }
            Op::Assign(assigns) => {
                let n = plan.first_next(at)?;
                let mut values = Vec::new();
                for (target, e) in assigns {
                    values.push((target, self.eval(cx, tid, e)?));
                }
                for (target, v) in values {
                    match target {
                        Target::Token(name) => {
                            self.tokens
                                .get_mut(&tid)
                                .ok_or("token inexistente")?
                                .labels
                                .insert(name.clone(), v);
                        }
                        Target::Item(name) => {
                            let item = self.carried(cx, tid)?;
                            cx.item_mut(item).set_label(name.clone(), v);
                        }
                    }
                }
                self.leave(plan, cx, tid, at);
                Ok(Step::Go(n))
            }
            Op::Acquire { res, count } => {
                let (res, count) = (*res, *count);
                if count > plan.resources[res].1 {
                    return Err(format!(
                        "pede {count} unidades de '{}', mais que a capacidade",
                        plan.resources[res].0
                    ));
                }
                if self.res_queue[res].is_empty() && self.free[res] >= count {
                    self.free[res] -= count;
                    self.hold(tid, res, count, now);
                    let n = plan.first_next(at)?;
                    self.leave(plan, cx, tid, at);
                    Ok(Step::Go(n))
                } else {
                    self.res_queue[res].push_back((tid, count, at));
                    Ok(Step::Wait)
                }
            }
            Op::Release { res, count } => {
                let (res, count) = (*res, *count);
                let n = plan.first_next(at)?;
                if self.free[res] + count > plan.resources[res].1 {
                    return Err(format!(
                        "liberou {count} unidades de '{}' que não estavam ocupadas",
                        plan.resources[res].0
                    ));
                }
                self.free[res] += count;
                self.unhold(plan, cx, tid, res, count, now);
                self.grant(plan, cx, res, now);
                self.leave(plan, cx, tid, at);
                Ok(Step::Go(n))
            }
            Op::WaitFor { .. } => {
                self.waiters[at].push_back(tid);
                Ok(Step::Wait)
            }
            Op::CreateItem { into, port, labels } => {
                let n = plan.first_next(at)?;
                let mut values = Vec::new();
                for (name, e) in labels {
                    values.push((name, self.eval(cx, tid, e)?));
                }
                let id = cx.inject_item(*into, *port);
                for (name, v) in values {
                    cx.item_mut(id).set_label(name.clone(), v);
                }
                self.tokens.get_mut(&tid).ok_or("token inexistente")?.item = Some(id);
                self.leave(plan, cx, tid, at);
                Ok(Step::Go(n))
            }
            Op::Batch(size) => {
                self.waiters[at].push_back(tid);
                if self.waiters[at].len() < *size {
                    return Ok(Step::Wait);
                }
                let n = plan.first_next(at)?;
                let members: Vec<u32> = self.waiters[at].drain(..*size).collect();
                let first = members[0];
                for &m in &members {
                    self.leave(plan, cx, m, at);
                }
                for &m in &members[1..] {
                    // The dropped tokens pass their held resources to the survivor.
                    if let Some(t) = self.tokens.remove(&m) {
                        if let Some(f) = self.tokens.get_mut(&first) {
                            f.holds.extend(t.holds);
                        }
                    }
                }
                if first == tid {
                    Ok(Step::Go(n))
                } else {
                    // `tid` was dropped (it is the last one); the first token continues.
                    self.ready.push_back((first, n));
                    Ok(Step::End)
                }
            }
            Op::Sink => {
                let t = self.tokens.remove(&tid).ok_or("token inexistente")?;
                if !t.holds.is_empty() {
                    return Err("o token terminou segurando recursos sem liberá-los".into());
                }
                self.counts[at] -= 1;
                cx.trace_tokens(at, self.counts[at]);
                cx.record(&format!("{}.tempo_no_fluxo", plan.names[at]), now - t.born);
                Ok(Step::End)
            }
        }
    }

    /// The live item carried by the token.
    fn carried(&self, cx: &Ctx, tid: u32) -> Result<ItemId, String> {
        self.tokens
            .get(&tid)
            .and_then(|t| t.item)
            .and_then(|i| cx.item_by_key(item_key(i)))
            .ok_or_else(|| "o token não carrega um item".to_string())
    }

    fn hold(&mut self, tid: u32, res: usize, count: usize, now: Time) {
        if let Some(t) = self.tokens.get_mut(&tid) {
            t.holds.push(Hold {
                res,
                count,
                since: now,
            });
        }
    }

    /// Removes `count` units of `res` from the token's holds (oldest first), recording
    /// the holding time of each chunk.
    fn unhold(&mut self, plan: &Plan, cx: &mut Ctx, tid: u32, res: usize, count: usize, now: Time) {
        let Some(t) = self.tokens.get_mut(&tid) else {
            return;
        };
        let mut left = count;
        for h in t.holds.iter_mut().filter(|h| h.res == res) {
            if left == 0 {
                break;
            }
            let take = left.min(h.count);
            h.count -= take;
            left -= take;
            cx.record(&format!("{}.ocupado", plan.resources[res].0), now - h.since);
        }
        t.holds.retain(|h| h.count > 0);
    }

    /// Hands free units to the waiting tokens of `res`, in FIFO order.
    fn grant(&mut self, plan: &Plan, cx: &mut Ctx, res: usize, now: Time) {
        while let Some(&(tid, count, at)) = self.res_queue[res].front() {
            if self.free[res] < count {
                break;
            }
            self.res_queue[res].pop_front();
            self.free[res] -= count;
            self.hold(tid, res, count, now);
            self.resume(plan, cx, tid, at);
        }
    }

    fn event_at(&mut self, plan: &Plan, cx: &mut Ctx, tag: u32) {
        let payload = tag & TAG_PAYLOAD;
        match tag & TAG_KIND {
            TAG_SOURCE => {
                let at = payload as usize;
                let now = cx.now();
                let id = self.new_token(now, None);
                self.ready.push_back((id, at));
                self.generated[at] += 1;
                self.schedule_source(plan, cx, at, false);
            }
            TAG_SCHEDULE => {
                if let Some(&(at, count)) = self.sched.get(payload as usize) {
                    let now = cx.now();
                    for _ in 0..count {
                        let id = self.new_token(now, None);
                        self.ready.push_back((id, at));
                    }
                }
            }
            TAG_DELAY => {
                if let Some(t) = self.tokens.get(&payload) {
                    let at = t.activity;
                    if matches!(plan.ops[at], Op::Delay(_)) {
                        self.resume(plan, cx, payload, at);
                    }
                }
            }
            _ => {}
        }
        self.drive(plan, cx);
    }

    /// Schedules the next arrival of a `Source` activity.
    fn schedule_source(&mut self, plan: &Plan, cx: &mut Ctx, at: usize, first: bool) {
        if let Op::Source {
            interarrival,
            first_at_zero,
            limit,
        } = &plan.ops[at]
        {
            if limit.is_some_and(|l| self.generated[at] >= l) {
                return;
            }
            let delay = if first && *first_at_zero {
                0.0
            } else {
                cx.sample_duration(interarrival)
            };
            cx.schedule(delay, TAG_SOURCE | at as u32, None);
        }
    }

    /// The agent chose branch `choice` for the oldest parked token.
    fn answer(&mut self, plan: &Plan, cx: &mut Ctx, choice: u64) {
        let Some((tid, at)) = self.agent_wait.pop_front() else {
            cx.error("resposta do agente sem token à espera".to_string());
            return;
        };
        match plan.next[at].get(choice as usize) {
            Some(&next) => {
                self.leave(plan, cx, tid, at);
                self.ready.push_back((tid, next));
            }
            None => {
                cx.error(format!(
                    "atividade '{}': a opção {choice} do agente não existe",
                    plan.names[at]
                ));
                self.tokens.remove(&tid);
            }
        }
    }

    fn message(&mut self, plan: &Plan, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        if tag == MSG_DECISION {
            self.answer(plan, cx, payload);
            self.drive(plan, cx);
            return;
        }
        let ev = match tag {
            MSG_ITEM_ENTERED => BlockEvent::Entry,
            MSG_ITEM_EXITED => BlockEvent::Exit,
            _ => return,
        };
        let item = cx.item_by_key(payload);
        let now = cx.now();
        for at in 0..plan.ops.len() {
            match &plan.ops[at] {
                Op::OnEvent { block, event } if *block == from && *event == ev => {
                    let id = self.new_token(now, item);
                    self.ready.push_back((id, at));
                }
                Op::WaitFor { block, event } if *block == from && *event == ev => {
                    if let Some(tid) = self.waiters[at].pop_front() {
                        if let Some(t) = self.tokens.get_mut(&tid) {
                            t.item = item;
                        }
                        self.resume(plan, cx, tid, at);
                    }
                }
                _ => {}
            }
        }
        self.drive(plan, cx);
    }
}

/// The flow engine: one simulation block per flow.
pub struct Flow {
    pub def: FlowDef,
    /// Object name → block id, for activities that reference objects.
    pub objects: HashMap<String, BlockId>,
    plan: Plan,
    rt: Rt,
}

impl Flow {
    /// Compiles the flow (expressions included). Problems of an unvalidated model are
    /// reported as modelling errors when the run starts.
    pub fn new(def: FlowDef, objects: HashMap<String, BlockId>) -> Self {
        let plan = Plan::build(&def, &objects);
        Flow {
            def,
            objects,
            plan,
            rt: Rt::default(),
        }
    }
}

impl Behavior for Flow {
    fn kind(&self) -> &'static str {
        "flow"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        cx.set_state(State::Idle);
        let plan = &self.plan;
        for e in &plan.errors {
            cx.error(e.clone());
        }
        if !plan.errors.is_empty() {
            return;
        }
        let rt = &mut self.rt;
        rt.reset(plan);
        // One subscription per watched block, with the union of the events.
        let mut watched: BTreeMap<usize, (BlockId, bool, bool)> = BTreeMap::new();
        for op in &plan.ops {
            if let Op::OnEvent { block, event } | Op::WaitFor { block, event } = op {
                let w = watched.entry(block.0).or_insert((*block, false, false));
                match event {
                    BlockEvent::Entry => w.1 = true,
                    BlockEvent::Exit => w.2 = true,
                }
            }
        }
        for (block, entry, exit) in watched.values() {
            cx.subscribe(*block, *entry, *exit);
        }
        for (at, op) in plan.ops.iter().enumerate() {
            match op {
                Op::Source { .. } => rt.schedule_source(plan, cx, at, true),
                Op::Schedule { times } => {
                    for &(time, count) in times {
                        if count > 0 {
                            let slot = rt.sched.len() as u32;
                            rt.sched.push((at, count));
                            cx.schedule(time, TAG_SCHEDULE | slot, None);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _item: Option<ItemId>) {
        self.rt.event_at(&self.plan, cx, tag);
    }

    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        self.rt.message(&self.plan, cx, from, tag, payload);
    }
}
