//! The fluid network block: event engine around the rate solver.
//!
//! Between events every link has a constant rate, so each tank level is piecewise
//! linear. The block keeps, for every node, the level at the instant `last`; to know
//! the level at any later instant it applies `net rate * elapsed` (no per-step
//! accumulation, so there is no drift). At every change (a scheduled boundary event, an
//! item entering a `FromItems` object, the start) it:
//!
//! 1. advances all levels / accumulators / volumes to `now` (snapping a tank that was
//!    heading to empty or full onto the exact boundary);
//! 2. completes converter batches (`ToItems` creates one item per `volume` collected);
//! 3. applies impulses (`FromItems` volumes delivered at once downstream);
//! 4. re-solves the rates ([`crate::solver`]), traces the nodes whose `(level, rate)`
//!    changed and schedules the earliest next boundary event: a tank reaching 0 or its
//!    capacity, or a converter completing a volume.
//!
//! A `ToItems` converter creates its item with `try_inject_item`. When the object
//! refuses (full or stopped) the kernel answers `MSG_INJECT_REFUSED`: the converter
//! keeps the full volume, becomes `blocked` (accepted rate 0, so upstream tanks fill)
//! and retries when an item leaves the object (it subscribes to the object's exits).

use std::collections::HashMap;

use utsim_core::task::{MSG_INJECT_REFUSED, MSG_ITEM_ENTERED, MSG_ITEM_EXITED};
use utsim_core::{Behavior, BlockId, BlockState, Ctx, EventId, Item, State, Time};

use crate::solver::{Graph, Role, SolveError, solve};
use crate::{FluidDef, FluidKind};

const TAG_UPDATE: u32 = 1;
/// Most items one `ToItems` converter may create in a single event.
const MAX_ITEMS_PER_EVENT: u32 = 100_000;
/// Rates below this are treated as zero when computing the next event.
const RATE_EPS: f64 = 1e-12;

/// Most boundary steps taken at one instant (see [`FluidNet::resolve`]) before the next
/// one is scheduled as usual.
const INSTANT_STEPS: usize = 1000;

/// Tolerance used to decide that a quantity reached a boundary (empty, full, volume).
fn eps(scale: f64) -> f64 {
    1e-9 * scale.max(1.0)
}

/// The fluid network block.
pub struct FluidNet {
    pub def: FluidDef,
    /// Discrete object name → block id (converters).
    pub objects: HashMap<String, BlockId>,
    graph: Graph,
    /// Per node: tank level.
    level: Vec<f64>,
    /// Per node: `ToItems` accumulation.
    acc: Vec<f64>,
    /// Per node: a `ToItems` converter whose object refused the last item.
    blocked: Vec<bool>,
    /// Per link: current rate.
    rate: Vec<f64>,
    /// Per node: total volume passed or received since the warm-up.
    vol: Vec<f64>,
    /// Per node: integral of the level since `start` (tanks).
    area: Vec<f64>,
    max_level: Vec<f64>,
    start: Time,
    last: Time,
    event: Option<EventId>,
    traced: Vec<Option<(f64, f64)>>,
}

impl FluidNet {
    pub fn new(def: FluidDef, objects: HashMap<String, BlockId>) -> Self {
        let n = def.nodes.len();
        let index: HashMap<&str, usize> = def
            .nodes
            .iter()
            .enumerate()
            .map(|(i, nd)| (nd.name.as_str(), i))
            .collect();
        let links: Vec<(usize, usize)> = def
            .links
            .iter()
            .filter_map(|(a, b)| Some((*index.get(a.as_str())?, *index.get(b.as_str())?)))
            .collect();
        // The model validates the definition (acyclic); fall back to no links otherwise.
        let graph = Graph::new(n, links).unwrap_or_else(|_| Graph::new(n, Vec::new()).unwrap());
        let nl = graph.links.len();
        let mut net = FluidNet {
            def,
            objects,
            graph,
            level: vec![0.0; n],
            acc: vec![0.0; n],
            blocked: vec![false; n],
            rate: vec![0.0; nl],
            vol: vec![0.0; n],
            area: vec![0.0; n],
            max_level: vec![0.0; n],
            start: 0.0,
            last: 0.0,
            event: None,
            traced: vec![None; n],
        };
        net.init();
        net
    }

    fn init(&mut self) {
        let n = self.def.nodes.len();
        for i in 0..n {
            self.level[i] = match &self.def.nodes[i].kind {
                FluidKind::Tank { initial, .. } => *initial,
                _ => 0.0,
            };
        }
        self.acc = vec![0.0; n];
        self.blocked = vec![false; n];
        self.rate = vec![0.0; self.graph.links.len()];
        self.vol = vec![0.0; n];
        self.area = vec![0.0; n];
        self.max_level = self.level.clone();
        self.start = 0.0;
        self.last = 0.0;
        self.event = None;
        self.traced = vec![None; n];
    }

    fn capacity(&self, i: usize) -> f64 {
        match &self.def.nodes[i].kind {
            FluidKind::Tank { capacity, .. } => *capacity,
            _ => 0.0,
        }
    }

    fn inflow(&self, i: usize) -> f64 {
        self.graph.inn[i].iter().map(|&l| self.rate[l]).sum()
    }

    fn outflow(&self, i: usize) -> f64 {
        self.graph.out[i].iter().map(|&l| self.rate[l]).sum()
    }

    /// Rate at which node `i` handles fluid (volume statistics): what a source emits,
    /// what everything else receives.
    fn through(&self, i: usize) -> f64 {
        match self.def.nodes[i].kind {
            FluidKind::Source { .. } => self.outflow(i),
            _ => self.inflow(i),
        }
    }

    /// Net accumulation rate of a tank.
    fn net(&self, i: usize) -> f64 {
        self.inflow(i) - self.outflow(i)
    }

    /// Level of tank `i` after `dt` more time units at the current rates.
    fn level_after(&self, i: usize, dt: f64) -> f64 {
        (self.level[i] + self.net(i) * dt).clamp(0.0, self.capacity(i))
    }

    fn role(&self, i: usize) -> Role {
        let inf = f64::INFINITY;
        match &self.def.nodes[i].kind {
            FluidKind::Source { rate } => Role::Source { rate: *rate },
            FluidKind::Tank {
                capacity,
                max_in,
                max_out,
                ..
            } => Role::Tank {
                empty: self.level[i] <= eps(*capacity),
                full: self.level[i] >= capacity - eps(*capacity),
                max_in: max_in.unwrap_or(inf),
                max_out: max_out.unwrap_or(inf),
            },
            FluidKind::Pipe { max_rate } => Role::Pipe {
                max_rate: *max_rate,
            },
            FluidKind::Sink { max_rate } => Role::Drain {
                max_rate: max_rate.unwrap_or(inf),
            },
            FluidKind::ToItems { max_rate, .. } => Role::Drain {
                max_rate: if self.blocked[i] {
                    0.0
                } else {
                    max_rate.unwrap_or(inf)
                },
            },
            FluidKind::FromItems { .. } => Role::Idle,
        }
    }

    /// Moves every level, accumulator and volume from `last` to `now`.
    fn advance(&mut self, now: Time) {
        let dt = now - self.last;
        if dt <= 0.0 {
            return;
        }
        for i in 0..self.def.nodes.len() {
            let through = self.through(i);
            self.vol[i] += through * dt;
            match &self.def.nodes[i].kind {
                FluidKind::Tank { capacity, .. } => {
                    let net = self.net(i);
                    let old = self.level[i];
                    let mut new = (old + net * dt).clamp(0.0, *capacity);
                    if net > 0.0 && new >= capacity - eps(*capacity) {
                        new = *capacity;
                    } else if net < 0.0 && new <= eps(*capacity) {
                        new = 0.0;
                    }
                    // Exact integral of a straight segment.
                    self.area[i] += (old + new) / 2.0 * dt;
                    self.level[i] = new;
                    self.max_level[i] = self.max_level[i].max(new);
                }
                FluidKind::ToItems { .. } => self.acc[i] += through * dt,
                _ => {}
            }
        }
        self.last = now;
    }

    /// Creates one item per completed `volume` in every `ToItems` converter.
    fn complete_batches(&mut self, cx: &mut Ctx) {
        for i in 0..self.def.nodes.len() {
            let FluidKind::ToItems { object, volume, .. } = &self.def.nodes[i].kind else {
                continue;
            };
            // The tolerance must stay below the volume itself: with `eps` alone a
            // volume under 1e-9 made `acc >= volume - e` always true (endless loop).
            let (volume, e) = (*volume, eps(*volume).min(*volume * 1e-6));
            let target = self.objects.get(object).copied();
            // A tiny `volume` against a large accumulation would create billions of
            // items in one event; stop with a clear error instead of exhausting memory.
            let pending = (self.acc[i] / volume).floor();
            if pending > MAX_ITEMS_PER_EVENT as f64 {
                cx.error(format!(
                    "conversor '{}' criaria {pending:.0} itens de uma vez (volume por item {volume}); \
                     aumente o volume por item ou reduza a vazão (limite {MAX_ITEMS_PER_EVENT} por evento)",
                    self.def.nodes[i].name
                ));
                self.acc[i] = 0.0;
                continue;
            }
            while !self.blocked[i] && self.acc[i] >= volume - e {
                self.acc[i] = (self.acc[i] - volume).max(0.0);
                if self.acc[i] < e {
                    self.acc[i] = 0.0;
                }
                // A refusal comes back as MSG_INJECT_REFUSED, which returns the volume.
                if let Some(b) = target {
                    cx.try_inject_item(b, 0);
                }
            }
        }
    }

    /// Delivers `v` at once to node `i` and, for pass-through nodes, downstream.
    fn deliver(&mut self, cx: &mut Ctx, i: usize, v: f64) {
        self.vol[i] += v;
        match &self.def.nodes[i].kind {
            FluidKind::Tank { capacity, .. } => {
                let cap = *capacity;
                let new = self.level[i] + v;
                if new > cap + eps(cap) {
                    cx.error(format!(
                        "tanque '{}' transbordou: nível {new:.6} acima da capacidade {cap} \
                         ao receber {v} de uma vez (itens que viram fluido)",
                        self.def.nodes[i].name
                    ));
                }
                self.level[i] = new.min(cap);
                self.max_level[i] = self.max_level[i].max(self.level[i]);
            }
            FluidKind::ToItems { .. } => self.acc[i] += v,
            FluidKind::Pipe { .. } | FluidKind::FromItems { .. } => {
                let heads: Vec<usize> = self.graph.out[i]
                    .iter()
                    .map(|&l| self.graph.links[l].1)
                    .collect();
                if heads.is_empty() {
                    cx.error(format!(
                        "fluidos: o nó '{}' recebeu volume de uma vez mas não tem saída",
                        self.def.nodes[i].name
                    ));
                }
                let share = v / heads.len().max(1) as f64;
                for h in heads {
                    self.deliver(cx, h, share);
                }
            }
            FluidKind::Source { .. } | FluidKind::Sink { .. } => {}
        }
    }

    fn display(&self, i: usize) -> (f64, f64) {
        match &self.def.nodes[i].kind {
            FluidKind::Tank { .. } => (self.level[i], self.net(i)),
            FluidKind::ToItems { .. } => (self.acc[i], self.inflow(i)),
            _ => (0.0, self.through(i)),
        }
    }

    /// Re-solves the rates, traces what changed and schedules the next boundary event.
    ///
    /// With rates near the largest `f64` the next boundary can be closer than the clock can
    /// tell apart from now (`now + dt == now`): scheduled, it would fire at this same instant,
    /// move nothing (no time passed) and schedule itself again for ever. Such a step is
    /// taken at once instead ([`FluidNet::step_at_once`]) and the rates solved again, up to
    /// [`INSTANT_STEPS`] times.
    fn resolve(&mut self, cx: &mut Ctx) {
        for _ in 0..INSTANT_STEPS {
            self.solve_rates(cx);
            match self.next_boundary() {
                Some(dt) if cx.now() + dt == cx.now() => {
                    self.step_at_once(dt);
                    self.complete_batches(cx);
                }
                _ => break,
            }
        }
        self.trace_and_schedule(cx);
    }

    /// Solves the rates for the current levels.
    fn solve_rates(&mut self, cx: &mut Ctx) {
        let n = self.def.nodes.len();
        let roles: Vec<Role> = (0..n).map(|i| self.role(i)).collect();
        match solve(&self.graph, &roles) {
            Ok(r) => self.rate = r,
            Err(e) => {
                let message = match e {
                    SolveError::Unbounded(l) => {
                        let (a, b) = self.graph.links[l];
                        format!(
                            "fluidos: vazão ilimitada entre '{}' e '{}': defina max_out, \
                             max_rate ou a taxa da fonte",
                            self.def.nodes[a].name, self.def.nodes[b].name
                        )
                    }
                    SolveError::NoConvergence => {
                        "fluidos: o cálculo das vazões não convergiu".to_string()
                    }
                };
                cx.error(message);
                self.rate = vec![0.0; self.graph.links.len()];
            }
        }
    }

    /// Time to the earliest boundary at the current rates: a tank reaching 0 or its
    /// capacity, or a converter completing a volume (`None`: nothing changes).
    fn next_boundary(&self) -> Option<f64> {
        let mut next: Option<f64> = None;
        for i in 0..self.def.nodes.len() {
            if let Some(dt) = self.boundary_of(i) {
                next = Some(next.map_or(dt, |m: f64| m.min(dt)));
            }
        }
        next
    }

    /// Time until node `i` reaches its own boundary at the current rates.
    fn boundary_of(&self, i: usize) -> Option<f64> {
        match &self.def.nodes[i].kind {
            FluidKind::Tank { capacity, .. } => {
                let net = self.net(i);
                if net > RATE_EPS && self.level[i] < capacity - eps(*capacity) {
                    Some(((capacity - self.level[i]) / net).max(0.0))
                } else if net < -RATE_EPS && self.level[i] > eps(*capacity) {
                    Some((self.level[i] / -net).max(0.0))
                } else {
                    None
                }
            }
            FluidKind::ToItems { volume, .. } => {
                let r = self.inflow(i);
                (r > RATE_EPS).then(|| ((volume - self.acc[i]) / r).max(0.0))
            }
            _ => None,
        }
    }

    /// Takes a boundary step `dt` too short for the clock without moving `last`: the nodes
    /// whose boundary it is land exactly on it (a tank empty or full, a converter with its
    /// volume); the others move by rate × `dt` when that is a finite amount.
    fn step_at_once(&mut self, dt: f64) {
        let ends: Vec<Option<f64>> = (0..self.def.nodes.len())
            .map(|i| self.boundary_of(i))
            .collect();
        let reaches = |e: Option<f64>| e.is_some_and(|e| e <= dt * (1.0 + 1e-9));
        let moved = |rate: f64| {
            let v = rate * dt;
            if v.is_finite() { v } else { 0.0 }
        };
        for (i, end) in ends.into_iter().enumerate() {
            let through = self.through(i);
            self.vol[i] += moved(through);
            match &self.def.nodes[i].kind {
                FluidKind::Tank { capacity, .. } => {
                    let net = self.net(i);
                    self.level[i] = if reaches(end) {
                        if net > 0.0 { *capacity } else { 0.0 }
                    } else {
                        (self.level[i] + moved(net)).clamp(0.0, *capacity)
                    };
                    self.max_level[i] = self.max_level[i].max(self.level[i]);
                }
                FluidKind::ToItems { volume, .. } => {
                    self.acc[i] = if reaches(end) {
                        self.acc[i].max(*volume)
                    } else {
                        self.acc[i] + moved(through)
                    };
                }
                _ => {}
            }
        }
    }

    /// Traces the nodes whose `(level, rate)` changed and schedules the next boundary event.
    fn trace_and_schedule(&mut self, cx: &mut Ctx) {
        let n = self.def.nodes.len();
        for i in 0..n {
            let shown = self.display(i);
            if self.traced[i] != Some(shown) {
                cx.trace_fluid(i, shown.0, shown.1);
                self.traced[i] = Some(shown);
            }
        }
        let busy = self.rate.iter().any(|&r| r > RATE_EPS);
        cx.set_state(if busy { State::Processing } else { State::Idle });

        if let Some(id) = self.event.take() {
            cx.cancel(id);
        }
        if let Some(dt) = self.next_boundary() {
            self.event = Some(cx.schedule(dt, TAG_UPDATE, None));
        }
    }
}

impl Behavior for FluidNet {
    fn kind(&self) -> &'static str {
        "fluid"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.init();
        for i in 0..self.def.nodes.len() {
            match &self.def.nodes[i].kind {
                FluidKind::FromItems { object, .. } => match self.objects.get(object) {
                    Some(&b) => cx.subscribe(b, true, false),
                    None => cx.error(format!("fluidos: objeto '{object}' não existe")),
                },
                FluidKind::ToItems { object, .. } => match self.objects.get(object) {
                    Some(&b) => cx.subscribe(b, false, true),
                    None => cx.error(format!("fluidos: objeto '{object}' não existe")),
                },
                _ => {}
            }
            if matches!(self.def.nodes[i].kind, FluidKind::FromItems { .. })
                && self.graph.out[i].is_empty()
            {
                cx.error(format!(
                    "fluidos: o nó '{}' (FromItems) não tem ligação de saída",
                    self.def.nodes[i].name
                ));
            }
        }
        self.resolve(cx);
    }

    /// Tank levels now. Restoring them is done by the model, which sets each tank's
    /// `initial` level (the default `restore` refuses levels).
    fn save_state(&self, cx: &Ctx, state: &mut BlockState) -> Option<String> {
        let dt = (cx.now() - self.last).max(0.0);
        let mut partial = false;
        for (i, node) in self.def.nodes.iter().enumerate() {
            match node.kind {
                FluidKind::Tank { .. } => {
                    state
                        .levels
                        .insert(node.name.clone(), self.level_after(i, dt));
                }
                FluidKind::ToItems { .. } => partial |= self.acc[i] > 0.0,
                _ => {}
            }
        }
        partial.then(|| {
            "o volume parcial acumulado nos conversores ToItems não entra no estado gravado".into()
        })
    }

    fn accepts_is_item_independent(&self) -> bool {
        true
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _item: Option<utsim_core::ItemId>) {
        if tag != TAG_UPDATE {
            return;
        }
        self.event = None;
        self.advance(cx.now());
        self.complete_batches(cx);
        self.resolve(cx);
    }

    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        if tag == MSG_INJECT_REFUSED {
            self.advance(cx.now());
            for i in 0..self.def.nodes.len() {
                if let FluidKind::ToItems { object, volume, .. } = &self.def.nodes[i].kind
                    && self.objects.get(object).map(|b| b.0 as u64) == Some(payload)
                {
                    // One message per refused item: every one returns its volume.
                    self.acc[i] += *volume;
                    self.blocked[i] = true;
                }
            }
            self.resolve(cx);
            return;
        }
        if tag == MSG_ITEM_EXITED {
            self.advance(cx.now());
            for i in 0..self.def.nodes.len() {
                if let FluidKind::ToItems { object, .. } = &self.def.nodes[i].kind
                    && self.objects.get(object) == Some(&from)
                {
                    self.blocked[i] = false;
                }
            }
            self.complete_batches(cx);
            self.resolve(cx);
            return;
        }
        if tag != MSG_ITEM_ENTERED {
            return;
        }
        self.advance(cx.now());
        for i in 0..self.def.nodes.len() {
            if let FluidKind::FromItems { object, volume } = &self.def.nodes[i].kind
                && self.objects.get(object) == Some(&from)
            {
                let v = *volume;
                self.deliver(cx, i, v);
            }
        }
        self.complete_batches(cx);
        self.resolve(cx);
    }

    fn on_warmup(&mut self, cx: &mut Ctx) {
        self.advance(cx.now());
        self.start = cx.now();
        self.area = vec![0.0; self.def.nodes.len()];
        self.max_level = self.level.clone();
        self.vol = vec![0.0; self.def.nodes.len()];
    }

    fn custom_metrics(&self, now: Time) -> Vec<(String, f64)> {
        let dt = (now - self.last).max(0.0);
        let span = now - self.start;
        let mut out = Vec::new();
        for (i, node) in self.def.nodes.iter().enumerate() {
            if let FluidKind::Tank { .. } = node.kind {
                let end = self.level_after(i, dt);
                let area = self.area[i] + (self.level[i] + end) / 2.0 * dt;
                let mean = if span > 0.0 { area / span } else { end };
                out.push((format!("{}.nivel_medio", node.name), mean));
                out.push((
                    format!("{}.nivel_max", node.name),
                    self.max_level[i].max(end),
                ));
                out.push((format!("{}.nivel_final", node.name), end));
            }
            out.push((
                format!("{}.volume", node.name),
                self.vol[i] + self.through(i) * dt,
            ));
        }
        out
    }
}
