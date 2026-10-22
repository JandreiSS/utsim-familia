use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::traffic::{Traffic, segment_durations};
use utsim_core::task::{
    MSG_ARRIVED, MSG_IDLE, MSG_RELEASE, MSG_USER, distance, path_length, travel_time,
};
use utsim_core::{
    Behavior, BlockId, Ctx, Dist, EventId, Item, ItemId, State, TaskKind, TaskRequest, Time,
};

/// Sent to the dispatcher right before [`MSG_IDLE`]: one coordinate of the member's
/// position (payload: `f64::to_bits`). X, Y and Z are separate messages because a
/// message carries a single `u64`; they are delivered in order, so the dispatcher
/// always has a complete position when the idle message arrives.
pub const MSG_POSITION_X: u32 = MSG_USER + 1;
pub const MSG_POSITION_Y: u32 = MSG_USER + 2;
pub const MSG_POSITION_Z: u32 = MSG_USER + 3;

/// Sent by an AGV that releases an edge to the vehicle that was next in that edge's
/// queue (payload: edge id). The receiver already holds the edge when it arrives.
pub const MSG_TRAFFIC_GRANT: u32 = MSG_USER + 10;

const START: u32 = 10;
const ARRIVED_PICKUP: u32 = 0;
const LOADED: u32 = 1;
const ARRIVED_DROP: u32 = 2;
const UNLOADED: u32 = 3;
const ARRIVED_STATION: u32 = 4;
const ARRIVED_HOME: u32 = 5;
/// An AGV finished one piece of its route and starts the next.
const AGV_STEP: u32 = 6;

/// One piece of an AGV route: a controlled/free network edge or a connector between
/// a point off the network and its node.
#[derive(Clone, Copy)]
struct Seg {
    a: [f64; 3],
    b: [f64; 3],
    /// Controlled edge to be held while travelling this piece.
    edge: Option<usize>,
    dur: Time,
}

/// An AGV route in progress.
struct Route {
    segs: Vec<Seg>,
    idx: usize,
    /// Timer tag fired when the last piece ends.
    tag: u32,
    /// Travel state to restore after waiting.
    state: State,
    /// Edge being waited for (state `Blocked`).
    wait: Option<usize>,
}

/// A running travel segment, kept so it can be cut short by a stop or a new task.
struct Leg {
    path: Vec<[f64; 3]>,
    start: Time,
    duration: Time,
}

/// The single pending timer of the transporter (start, travel, load, unload).
/// While the block is down the timer is paused (`event == None`) with the time left.
struct Timer {
    tag: u32,
    event: Option<EventId>,
    due: Time,
    remaining: Time,
    leg: Option<Leg>,
}

/// Point reached after `fraction` of the path length, and the rest of the path
/// starting at that point.
fn point_along(path: &[[f64; 3]], fraction: f64) -> ([f64; 3], Vec<[f64; 3]>) {
    let last = *path.last().expect("a path has points");
    let total = path_length(path);
    if fraction >= 1.0 || total <= 0.0 {
        return (last, vec![last]);
    }
    let target = fraction.max(0.0) * total;
    let mut walked = 0.0;
    for (i, w) in path.windows(2).enumerate() {
        let seg = distance(w[0], w[1]);
        if seg > 0.0 && walked + seg >= target {
            let f = (target - walked) / seg;
            let p = [
                w[0][0] + (w[1][0] - w[0][0]) * f,
                w[0][1] + (w[1][1] - w[0][1]) * f,
                w[0][2] + (w[1][2] - w[0][2]) * f,
            ];
            let mut rest = vec![p];
            rest.extend_from_slice(&path[i + 1..]);
            return (p, rest);
        }
        walked += seg;
    }
    (last, vec![last])
}

/// Mobile resource: carries items between blocks (forklift, AGV, person) and works
/// at stations as an operator.
///
/// **Policy.** One trip at a time. The next task is the highest priority one (FIFO
/// among equals). When it is a `Transport` and `capacity > 1`, the transporter also
/// takes, in queue order, up to `capacity - 1` other queued `Transport` tasks that
/// start at the same block, loads them one by one (`load_time` each), then delivers
/// them in the same order, travelling between destinations (`unload_time` each).
/// Extra tasks ride along even if their priority is lower than other queued tasks
/// that start elsewhere: the head of the queue still decides where the trip goes.
/// It never waits for more items; `Operate` tasks always run alone.
///
/// A task that arrives while the transporter is idle starts after a zero-delay event,
/// so several tasks requested in the same instant (e.g. a released batch) can share
/// a trip.
///
/// **Breakdowns and shifts.** `on_stop` pauses the pending timer keeping the time
/// left; `on_resume` continues it. For a travel the trace gets a `Travel` with a
/// zero-length path at the stopping point, and a new `Travel` for the remaining
/// path on resume. The stopping point assumes constant progress along the path in
/// time (exact for constant speed, approximate with acceleration), which only
/// affects the 3D view, never the timing.
///
/// **Home.** With `home`, an idle transporter with nothing queued drives back to its
/// own block position. A new task interrupts that travel; the transporter departs
/// from the point it reached.
///
/// **AGV traffic (phase 7).** With [`Transporter::agv`] the vehicle follows the node
/// path of the network piece by piece: connector from its position to the nearest
/// node (private spur, not controlled), one piece per edge, connector to the target.
/// Before entering a controlled edge (one with `capacity`) it asks the shared
/// [`Traffic`]; if full it stops in state `Blocked`, queued FIFO, still holding the
/// edge it was on, and resumes when the releasing vehicle's message arrives. It frees
/// the edge it leaves at the moment the next one is granted (or when it starts a free
/// edge or connector). Speed/acceleration follow the profile of the *whole* route:
/// piece `k` takes the slice of that profile (see [`segment_durations`]), and waiting
/// freezes the profile (no extra stop/start penalty). Without waiting the total equals
/// an ordinary transporter's. A breakdown pauses the current piece and keeps the edge;
/// interrupting a homing trip frees the edge at once. A deadlock is reported with
/// `cx.error("impasse: ...")`.
///
/// **Dispatcher protocol.** When free, it sends its position (`MSG_POSITION_X/Y/Z`)
/// and then `MSG_IDLE` with its capacity as payload. After arriving home it sends
/// the position again so `Nearest` stays accurate.
pub struct Transporter {
    speed: f64,
    acceleration: Option<f64>,
    deceleration: Option<f64>,
    load_time: Dist,
    unload_time: Dist,
    capacity: usize,
    home: bool,
    dispatcher: Option<BlockId>,
    queue: VecDeque<TaskRequest>,
    /// Tasks of the current trip (several only for batched transports).
    trip: Vec<TaskRequest>,
    /// Index into `trip` of the item being loaded or delivered.
    k: usize,
    timer: Option<Timer>,
    position: [f64; 3],
    /// Shared traffic state; `Some` only for AGVs.
    traffic: Option<Arc<Mutex<Traffic>>>,
    route: Option<Route>,
    /// Controlled edge currently occupied.
    held: Option<usize>,
    /// An edge was granted while the block was down: start moving on resume.
    grant_pending: bool,
}

impl Transporter {
    pub fn new(speed: f64) -> Self {
        Transporter {
            speed,
            acceleration: None,
            deceleration: None,
            load_time: Dist::Const(0.0),
            unload_time: Dist::Const(0.0),
            capacity: 1,
            home: false,
            dispatcher: None,
            queue: VecDeque::new(),
            trip: Vec::new(),
            k: 0,
            timer: None,
            position: [0.0; 3],
            traffic: None,
            route: None,
            held: None,
            grant_pending: false,
        }
    }

    /// Makes this transporter an AGV: it follows the network edge by edge under the
    /// given traffic control (see [`crate::traffic`]). `None` = ordinary transporter.
    pub fn agv(mut self, traffic: Option<Arc<Mutex<Traffic>>>) -> Self {
        self.traffic = traffic;
        self
    }

    pub fn acceleration(mut self, accel: Option<f64>, decel: Option<f64>) -> Self {
        self.acceleration = accel;
        self.deceleration = decel;
        self
    }

    pub fn handling(mut self, load_time: Dist, unload_time: Dist) -> Self {
        self.load_time = load_time;
        self.unload_time = unload_time;
        self
    }

    /// Items carried per trip (at least 1).
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    /// Whether to return to the block position when idle.
    pub fn home(mut self, home: bool) -> Self {
        self.home = home;
        self
    }

    /// The dispatcher this transporter reports to when it becomes free.
    pub fn dispatcher(mut self, dispatcher: Option<BlockId>) -> Self {
        self.dispatcher = dispatcher;
        self
    }

    fn arm(&mut self, cx: &mut Ctx, delay: Time, tag: u32, leg: Option<Leg>) {
        let event = cx.schedule(delay, tag, None);
        self.timer = Some(Timer {
            tag,
            event: Some(event),
            due: cx.now() + delay,
            remaining: delay,
            leg,
        });
    }

    fn lock(traffic: &Mutex<Traffic>) -> MutexGuard<'_, Traffic> {
        traffic.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Splits the way to `target` into pieces: connector from the current point to
    /// its node, one piece per network edge, connector to `target`. Connectors are
    /// private spurs and not controlled. `None` = no network route (plain travel).
    fn plan_route(&self, traffic: &Traffic, target: [f64; 3]) -> Option<Vec<Seg>> {
        if self.position == target {
            return None;
        }
        let router = traffic.router();
        let nr = router.node_route(self.position, target)?;
        let mut points = vec![self.position];
        let mut edges = vec![None];
        points.push(router.node_position(nr.nodes[0]));
        for w in nr.nodes.windows(2) {
            points.push(router.node_position(w[1]));
            edges.push(traffic.edge_between(w[0], w[1]));
        }
        points.push(target);
        edges.push(None);
        let lengths: Vec<f64> = points.windows(2).map(|w| distance(w[0], w[1])).collect();
        let durs = segment_durations(&lengths, self.speed, self.acceleration, self.deceleration);
        let mut segs = Vec::new();
        for (k, w) in points.windows(2).enumerate() {
            // Zero-length connectors (point exactly on a node) are dropped.
            if edges[k].is_none() && lengths[k] == 0.0 {
                continue;
            }
            segs.push(Seg {
                a: w[0],
                b: w[1],
                edge: edges[k],
                dur: durs[k],
            });
        }
        Some(segs)
    }

    /// Starts moving to `target` in travel state `state`; `tag` fires on arrival.
    fn travel(&mut self, cx: &mut Ctx, target: [f64; 3], tag: u32, state: State) {
        cx.set_state(state);
        if let Some(traffic) = self.traffic.clone() {
            let plan = self.plan_route(&Self::lock(&traffic), target);
            if let Some(segs) = plan.filter(|s| !s.is_empty()) {
                self.route = Some(Route {
                    segs,
                    idx: 0,
                    tag,
                    state,
                    wait: None,
                });
                self.begin_segment(cx);
                return;
            }
        }
        let path = cx.route(self.position, target);
        let t = travel_time(
            path_length(&path),
            self.speed,
            self.acceleration,
            self.deceleration,
        );
        if t > 0.0 {
            cx.trace_travel(path.clone(), t);
        }
        let leg = Leg {
            path,
            start: cx.now(),
            duration: t,
        };
        self.arm(cx, t, tag, Some(leg));
    }

    /// Starts piece `route.idx`: asks for its edge (waiting if it is full) and, once
    /// the edge is held, runs the piece.
    fn begin_segment(&mut self, cx: &mut Ctx) {
        let Some(traffic) = self.traffic.clone() else {
            return;
        };
        let Some(route) = &mut self.route else { return };
        match route.segs[route.idx].edge {
            Some(e) if self.held != Some(e) => {
                let me = cx.me();
                let mut t = Self::lock(&traffic);
                if !t.request(e, me) {
                    route.wait = Some(e);
                    cx.set_state(State::Blocked);
                    if let Some(cycle) = t.deadlock(me) {
                        let parts: Vec<String> = cycle
                            .iter()
                            .map(|w| {
                                let holders: Vec<&str> =
                                    w.holders.iter().map(|&h| cx.block_name(h)).collect();
                                format!(
                                    "{} espera a aresta {} (ocupada por {})",
                                    cx.block_name(w.vehicle),
                                    t.edge_label(w.edge),
                                    holders.join(", ")
                                )
                            })
                            .collect();
                        cx.error(format!("impasse: {}", parts.join("; ")));
                    }
                    return;
                }
                drop(t);
                self.release_held(cx);
                self.held = Some(e);
            }
            Some(_) => {}
            None => self.release_held(cx),
        }
        self.run_segment(cx);
    }

    /// Moves along the current piece (its edge, if any, is already held).
    fn run_segment(&mut self, cx: &mut Ctx) {
        let Some(route) = &self.route else { return };
        let seg = route.segs[route.idx];
        let tag = if route.idx + 1 == route.segs.len() {
            route.tag
        } else {
            AGV_STEP
        };
        if seg.dur > 0.0 {
            cx.trace_travel(vec![seg.a, seg.b], seg.dur);
        }
        let leg = Leg {
            path: vec![seg.a, seg.b],
            start: cx.now(),
            duration: seg.dur,
        };
        self.arm(cx, seg.dur, tag, Some(leg));
    }

    /// Frees the occupied edge and wakes the vehicle that was next in its queue.
    fn release_held(&mut self, cx: &mut Ctx) {
        let (Some(e), Some(traffic)) = (self.held.take(), self.traffic.clone()) else {
            return;
        };
        let next = Self::lock(&traffic).release(e, cx.me());
        if let Some(w) = next {
            cx.send(w, MSG_TRAFFIC_GRANT, e as u64);
        }
    }

    /// Drops the route: leaves the wait queue, frees the held edge.
    fn abandon_route(&mut self, cx: &mut Ctx) {
        self.route = None;
        self.grant_pending = false;
        if let Some(traffic) = self.traffic.clone() {
            let granted = Self::lock(&traffic).forget(cx.me());
            self.held = None;
            for (w, e) in granted {
                cx.send(w, MSG_TRAFFIC_GRANT, e as u64);
            }
        }
    }

    /// Where the running travel has got to, and the rest of its path.
    fn progress(leg: &Leg, now: Time) -> ([f64; 3], Vec<[f64; 3]>) {
        let f = if leg.duration > 0.0 {
            (now - leg.start) / leg.duration
        } else {
            1.0
        };
        point_along(&leg.path, f)
    }

    fn queue_task(&mut self, request: TaskRequest) {
        let at = self
            .queue
            .iter()
            .position(|q| q.priority < request.priority)
            .unwrap_or(self.queue.len());
        self.queue.insert(at, request);
    }

    /// Takes the next trip from the queue and starts driving to its first stop.
    fn begin_trip(&mut self, cx: &mut Ctx) {
        let Some(head) = self.queue.pop_front() else {
            return;
        };
        self.trip = vec![head];
        self.k = 0;
        match head.kind {
            TaskKind::Transport { from, .. } => {
                let mut i = 0;
                while self.trip.len() < self.capacity && i < self.queue.len() {
                    let same = matches!(
                        self.queue[i].kind,
                        TaskKind::Transport { from: f, .. } if f == from
                    );
                    if same {
                        self.trip.extend(self.queue.remove(i));
                    } else {
                        i += 1;
                    }
                }
                let target = cx.position(from);
                self.travel(cx, target, ARRIVED_PICKUP, State::TravelEmpty);
            }
            TaskKind::Operate { station, .. } => {
                let target = cx.position(station);
                self.travel(cx, target, ARRIVED_STATION, State::TravelEmpty);
            }
        }
    }

    fn report_position(&self, cx: &mut Ctx) {
        if let Some(d) = self.dispatcher {
            for (tag, v) in [
                (MSG_POSITION_X, self.position[0]),
                (MSG_POSITION_Y, self.position[1]),
                (MSG_POSITION_Z, self.position[2]),
            ] {
                cx.send(d, tag, v.to_bits());
            }
        }
    }

    /// Nothing to do: wait (or drive home) and tell the dispatcher.
    fn go_idle(&mut self, cx: &mut Ctx, announce: bool) {
        cx.set_state(State::Idle);
        if announce {
            if let Some(d) = self.dispatcher {
                self.report_position(cx);
                cx.send(d, MSG_IDLE, self.capacity as u64);
            }
        }
        if self.home && !cx.is_down() {
            let home = cx.position(cx.me());
            if distance(self.position, home) > 0.0 {
                self.travel(cx, home, ARRIVED_HOME, State::TravelEmpty);
            }
        }
    }

    fn finish(&mut self, cx: &mut Ctx) {
        self.trip.clear();
        self.k = 0;
        if self.queue.is_empty() {
            self.go_idle(cx, true);
        } else if !cx.is_down() {
            self.begin_trip(cx);
        }
    }

    /// A task arrived: if idle, start after the other same-instant tasks arrived.
    fn kick(&mut self, cx: &mut Ctx) {
        if self.trip.is_empty() && self.timer.is_none() && !self.queue.is_empty() && !cx.is_down() {
            self.arm(cx, 0.0, START, None);
        }
    }

    /// Gives up driving home because work arrived.
    fn abandon_homing(&mut self, cx: &mut Ctx) {
        if self.route.as_ref().is_some_and(|r| r.tag == ARRIVED_HOME) {
            // An AGV leaves wherever it is: the edge it occupies is freed at once.
            if let Some(timer) = self.timer.take() {
                if let (Some(event), Some(leg)) = (timer.event, &timer.leg) {
                    cx.cancel(event);
                    self.position = Self::progress(leg, cx.now()).0;
                }
            }
            self.abandon_route(cx);
            return;
        }
        let Some(timer) = &self.timer else { return };
        if timer.tag != ARRIVED_HOME {
            return;
        }
        if let (Some(event), Some(leg)) = (timer.event, &timer.leg) {
            cx.cancel(event);
            self.position = Self::progress(leg, cx.now()).0;
        }
        self.timer = None;
    }

    /// Drives to the destination of the item at `self.k`.
    fn deliver_leg(&mut self, cx: &mut Ctx) {
        if let TaskKind::Transport { to, .. } = self.trip[self.k].kind {
            let target = cx.position(to);
            self.travel(cx, target, ARRIVED_DROP, State::TravelLoaded);
        }
    }
}

impl Behavior for Transporter {
    fn kind(&self) -> &'static str {
        "transporter"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.queue.clear();
        self.trip.clear();
        self.k = 0;
        self.timer = None;
        self.abandon_route(cx);
        self.position = cx.position(cx.me());
        cx.set_state(State::Idle);
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_task(&mut self, cx: &mut Ctx, request: TaskRequest) {
        self.queue_task(request);
        self.abandon_homing(cx);
        self.kick(cx);
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _item: Option<ItemId>) {
        if let Some(Timer { leg: Some(leg), .. }) = self.timer.take() {
            self.position = *leg.path.last().expect("a path has points");
        }
        if tag == AGV_STEP {
            if let Some(route) = &mut self.route {
                route.idx += 1;
            }
            self.begin_segment(cx);
            return;
        }
        if self.route.take().is_some() {
            // Last piece done: the final edge is free once the vehicle reaches its node.
            self.release_held(cx);
        }
        if tag == START {
            self.begin_trip(cx);
            return;
        }
        if tag == ARRIVED_HOME {
            self.go_idle(cx, false);
            self.report_position(cx);
            return;
        }
        let Some(&task) = self.trip.get(self.k) else {
            return;
        };
        match (tag, task.kind) {
            (ARRIVED_PICKUP, TaskKind::Transport { .. }) => {
                cx.set_state(State::Loading);
                let t = cx.sample_duration(&self.load_time);
                self.arm(cx, t, LOADED, None);
            }
            (LOADED, TaskKind::Transport { item, .. }) => {
                cx.pick_up(item);
                if self.k + 1 < self.trip.len() {
                    self.k += 1;
                    let t = cx.sample_duration(&self.load_time);
                    self.arm(cx, t, LOADED, None);
                } else {
                    self.k = 0;
                    self.deliver_leg(cx);
                }
            }
            (ARRIVED_DROP, TaskKind::Transport { .. }) => {
                cx.set_state(State::Unloading);
                let t = cx.sample_duration(&self.unload_time);
                self.arm(cx, t, UNLOADED, None);
            }
            (UNLOADED, TaskKind::Transport { item, to, port, .. }) => {
                cx.drop_off(item, to, port);
                if self.k + 1 < self.trip.len() {
                    self.k += 1;
                    self.deliver_leg(cx);
                } else {
                    self.finish(cx);
                }
            }
            (ARRIVED_STATION, TaskKind::Operate { station, .. }) => {
                cx.set_state(State::Utilize);
                cx.send(station, MSG_ARRIVED, task.id.0);
            }
            _ => {}
        }
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        let now = cx.now();
        let Some(timer) = &mut self.timer else { return };
        let Some(event) = timer.event.take() else {
            return;
        };
        cx.cancel(event);
        timer.remaining = (timer.due - now).max(0.0);
        if let Some(leg) = &mut timer.leg {
            let (p, rest) = Self::progress(leg, now);
            self.position = p;
            leg.path = rest;
            leg.duration = timer.remaining;
            cx.trace_travel(vec![p, p], 0.0);
        }
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        let now = cx.now();
        if self.grant_pending {
            self.grant_pending = false;
            self.run_segment(cx);
            return;
        }
        if self.route.as_ref().is_some_and(|r| r.wait.is_some()) {
            return; // still waiting for an edge
        }
        if let Some(timer) = &mut self.timer {
            if timer.event.is_none() {
                timer.event = Some(cx.schedule(timer.remaining, timer.tag, None));
                timer.due = now + timer.remaining;
                if let Some(leg) = &mut timer.leg {
                    leg.start = now;
                    if timer.remaining > 0.0 {
                        cx.trace_travel(leg.path.clone(), timer.remaining);
                    }
                }
            }
            return;
        }
        if !self.trip.is_empty() {
            return; // operating a station: nothing is pending
        }
        if self.queue.is_empty() {
            self.go_idle(cx, false);
        } else {
            self.begin_trip(cx);
        }
    }

    fn on_message(&mut self, cx: &mut Ctx, _from: BlockId, tag: u32, payload: u64) {
        if tag == MSG_TRAFFIC_GRANT {
            let edge = payload as usize;
            let Some(route) = &mut self.route else { return };
            if route.wait != Some(edge) {
                return;
            }
            route.wait = None;
            let state = route.state;
            self.release_held(cx);
            self.held = Some(edge);
            cx.set_state(state);
            if cx.is_down() {
                self.grant_pending = true;
            } else {
                self.run_segment(cx);
            }
            return;
        }
        let releases_current = self
            .trip
            .first()
            .is_some_and(|r| r.id.0 == payload && matches!(r.kind, TaskKind::Operate { .. }));
        if tag == MSG_RELEASE && releases_current {
            self.finish(cx);
        }
    }
}
