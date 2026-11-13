use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::traffic::{Crossings, Traffic, crossing_plan, segment_durations};
use serde::{Deserialize, Serialize};
use utsim_core::init::Accepts;
use utsim_core::person::{TASK_LOAD, TASK_UNLOAD};
use utsim_core::stats::TimeWeighted;
use utsim_core::task::{
    MAINTENANCE_TAG, MSG_ARRIVED, MSG_IDLE, MSG_RELEASE, MSG_RESUME_WORK, MSG_SUSPEND, MSG_USER,
    MSG_WITHDRAW, distance, path_length, travel_time,
};
use utsim_core::{
    Behavior, BlockId, BlockState, Ctx, Dist, EventId, Item, ItemId, RequestId, Router, State,
    TaskKind, TaskRequest, Time,
};

/// A person that works on several cases at once (`docs/plano_linhas_especializadas.md`,
/// section 2.2): up to `max_open` operator jobs open together, worked in turn for `slice`
/// each, losing `switch` at every turn from one case to another. With `max_open: 1` the
/// person works exactly as without multitasking.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Multitask {
    /// Cases (operator jobs) open at the same time; at least 1.
    pub max_open: usize,
    /// Time worked on one case before turning to the next open one.
    pub slice: Dist,
    /// Time lost at each turn from one case to another (absent: none).
    #[serde(default = "no_time")]
    pub switch: Dist,
}

fn no_time() -> Dist {
    Dist::Const(0.0)
}

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

/// Dispatcher to member: give up the work you are travelling empty to, for a request of
/// higher priority ([`crate::Preempt::Empty`]); payload: that priority (`i32` as `i64`
/// bits), so work of this priority or more is never given back. The member answers
/// [`MSG_PREEMPTED`]
/// once per task it gives back (payload: request id) followed by its idle report, or
/// [`MSG_PREEMPT_REFUSED`] when it is no longer travelling empty.
pub const MSG_PREEMPT: u32 = MSG_USER + 20;
/// Member to dispatcher: the preemption was refused (loading, loaded, at the station...).
pub const MSG_PREEMPT_REFUSED: u32 = MSG_USER + 21;
/// Member to dispatcher: this task (payload: request id) is given back, not started.
pub const MSG_PREEMPTED: u32 = MSG_USER + 22;
/// Multitasking member to dispatcher: this case (payload: request id) is done and the
/// member has room for another one while it goes on with its other open cases.
pub const MSG_CASE_DONE: u32 = MSG_USER + 23;
/// Dispatcher to member, with [`crate::Preempt::Full`]: like [`MSG_PREEMPT`], and the member
/// may also give back the station jobs it is working on (payload: the priority of the
/// urgent request). For each job already started it sends [`MSG_WITHDRAW`] to the station
/// (which keeps the time left), then [`MSG_PREEMPTED`] for every task it gives back and its
/// idle report. A loaded transport or a maintenance job is never interrupted: the member
/// answers [`MSG_PREEMPT_REFUSED`].
pub const MSG_PREEMPT_FULL: u32 = MSG_USER + 24;

const START: u32 = 10;
const ARRIVED_PICKUP: u32 = 0;
const LOADED: u32 = 1;
const ARRIVED_DROP: u32 = 2;
const UNLOADED: u32 = 3;
const ARRIVED_STATION: u32 = 4;
const ARRIVED_HOME: u32 = 5;
/// An AGV finished one piece of its route and starts the next.
const AGV_STEP: u32 = 6;
/// A person finished a pause to recover from fatigue.
const RESTED: u32 = 7;
/// Multitasking: the time slice on the current case is over.
const SLICE: u32 = 8;
/// Multitasking: the turn to the next case is done.
const SWITCHED: u32 = 9;

/// One piece of an AGV route: a controlled/free network edge or a connector between
/// a point off the network and its node.
#[derive(Clone, Copy)]
struct Seg {
    a: [f64; 3],
    b: [f64; 3],
    /// Controlled edge to be held while travelling this piece.
    edge: Option<usize>,
    dur: Time,
    /// Crossing zone starting here (crossing, time to cross it): ask the board first.
    cross: Option<(usize, f64)>,
    /// Standing still (stop or wait before a crossing).
    stop: bool,
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
///
/// **People** ([`utsim_core::person`]). With a person profile the speed of each travel
/// comes from [`Ctx::walk_speed`] (load carried, fatigue, individual pace) and the load
/// and unload times from [`Ctx::person_time`]; when its fatigue reached the limit, the
/// person pauses (state `Resting`) after finishing a task and only then takes the next
/// one or reports idle. Without a profile nothing of this happens.
///
/// **Multitasking** ([`Multitask`]). While operating a station, the member also takes
/// other `Operate` jobs (not maintenance) up to `max_open` open cases; the dispatcher
/// offers it such jobs when no member is free. It works the cases in turn: after `slice`
/// on the current one, if another case is open, it sends [`MSG_SUSPEND`] to that station,
/// spends `switch` in state `Setup`, walks to the next case's station and sends
/// [`MSG_ARRIVED`] (first time) or [`MSG_RESUME_WORK`] there. When a case is released and
/// others remain, it tells the dispatcher ([`MSG_CASE_DONE`]) and turns to the next one
/// (paying `switch`); the idle report comes only when the last case is released.
pub struct Transporter {
    speed: f64,
    /// Speed of the travel in progress (the person's walking speed, or `speed`).
    cur_speed: f64,
    acceleration: Option<f64>,
    deceleration: Option<f64>,
    load_time: Dist,
    unload_time: Dist,
    capacity: usize,
    home: bool,
    /// Where home is (absent: the block's own position).
    home_point: Option<[f64; 3]>,
    /// Where the member walks when its shift pauses (absent: it stays where it is).
    break_point: Option<[f64; 3]>,
    /// Goes to the ends of a block (a belt's) instead of its centre.
    use_ends: bool,
    /// On a break at `break_point`: where to walk back to when the shift resumes.
    back_from_break: Option<[f64; 3]>,
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
    /// Finished its last task while closing (`ShiftEnd::Finish`) without telling the
    /// dispatcher: it reports `MSG_IDLE` when the shift comes back.
    announce_on_resume: bool,
    /// Own router (movement mode); `None` = the simulation's router.
    router: Option<Arc<dyn Router>>,
    /// Pedestrian × vehicle crossings (movement `Aisles`) and whether this is a vehicle.
    crossings: Option<(Arc<Mutex<Crossings>>, bool)>,
    /// Ramps of the aisle network (slower on them); `None`: none.
    ramps: Option<Arc<crate::ramps::Ramps>>,
    multitask: Option<Multitask>,
    /// Open cases other than the current one, in turn order; `true` once its station
    /// got `MSG_ARRIVED`.
    others: VecDeque<(TaskRequest, bool)>,
    /// Cases whose station already got `MSG_ARRIVED` (the current one included).
    started: Vec<RequestId>,
    /// Turns from one case to another since the warm-up, time spent turning and open
    /// cases over time.
    switches: u64,
    switching: TimeWeighted,
    open: TimeWeighted,
}

impl Transporter {
    pub fn new(speed: f64) -> Self {
        Transporter {
            speed,
            cur_speed: speed,
            acceleration: None,
            deceleration: None,
            load_time: Dist::Const(0.0),
            unload_time: Dist::Const(0.0),
            capacity: 1,
            home: false,
            home_point: None,
            break_point: None,
            use_ends: false,
            back_from_break: None,
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
            announce_on_resume: false,
            router: None,
            crossings: None,
            ramps: None,
            multitask: None,
            others: VecDeque::new(),
            started: Vec::new(),
            switches: 0,
            switching: TimeWeighted::default(),
            open: TimeWeighted::default(),
        }
    }

    /// Works on several cases at once (see [`Multitask`]); `None` = one at a time.
    pub fn multitask(mut self, multitask: Option<Multitask>) -> Self {
        self.multitask = multitask.filter(|m| m.max_open > 1);
        self
    }

    /// Whether the current trip is an operator job that may be interleaved.
    fn on_case(&self) -> bool {
        matches!(
            self.trip.first().map(|r| r.kind),
            Some(TaskKind::Operate { tag, .. }) if tag != MAINTENANCE_TAG
        )
    }

    fn open_cases(&self) -> usize {
        self.others.len() + usize::from(self.on_case())
    }

    /// Whether `request` joins the open cases instead of the queue.
    fn takes_case(&self, request: &TaskRequest) -> bool {
        let Some(m) = &self.multitask else {
            return false;
        };
        matches!(request.kind, TaskKind::Operate { tag, .. } if tag != MAINTENANCE_TAG)
            && (self.on_case() || !self.others.is_empty())
            && self.open_cases() < m.max_open
    }

    /// Arms `tag` after `delay`, paused (to be resumed by `on_resume`) while stopped.
    fn arm_or_hold(&mut self, cx: &mut Ctx, delay: Time, tag: u32) {
        if cx.is_down() {
            self.timer = Some(Timer {
                tag,
                event: None,
                due: cx.now() + delay,
                remaining: delay,
                leg: None,
            });
        } else {
            self.arm(cx, delay, tag, None);
        }
    }

    /// Working at the station of a started case with others open: time the slice.
    fn arm_slice(&mut self, cx: &mut Ctx) {
        let at_station = self.timer.is_none()
            && self.route.is_none()
            && self
                .trip
                .first()
                .is_some_and(|r| self.started.contains(&r.id));
        if !at_station || self.others.is_empty() {
            return;
        }
        let Some(m) = &self.multitask else { return };
        let slice = m.slice.clone();
        let t = cx.sample_duration(&slice);
        self.arm_or_hold(cx, t, SLICE);
    }

    /// Leaves the current case (already set aside) and turns to the next open one.
    fn begin_switch(&mut self, cx: &mut Ctx) {
        let Some(m) = &self.multitask else { return };
        let switch = m.switch.clone();
        let t = cx.sample_duration(&switch);
        self.switches += 1;
        self.switching.set(cx.now(), 1.0);
        self.open.set(cx.now(), self.open_cases() as f64);
        cx.set_state(State::Setup);
        self.arm_or_hold(cx, t, SWITCHED);
    }

    /// Cancels a pending time slice (the current case ended).
    fn drop_slice(&mut self, cx: &mut Ctx) {
        if self.timer.as_ref().is_some_and(|t| t.tag == SLICE) {
            if let Some(event) = self.timer.take().and_then(|t| t.event) {
                cx.cancel(event);
            }
        }
    }

    /// Takes part in the crossing reservations (movement `Aisles`): `(board, vehicle)`.
    /// A vehicle stops before each crossing and yields to pedestrians; a pedestrian
    /// waits only for a vehicle already inside (see [`Crossings`]). `None` = no crossings.
    pub fn crossings(mut self, crossings: Option<(Arc<Mutex<Crossings>>, bool)>) -> Self {
        self.crossings = crossings;
        self
    }

    /// The ramps of the aisle network: on them it goes slower (`ramps.rs`).
    pub fn ramps(mut self, ramps: Option<Arc<crate::ramps::Ramps>>) -> Self {
        self.ramps = ramps.filter(|r| !r.is_empty());
        self
    }

    /// Speed on `path` at `speed`: slower when it goes up or down a ramp.
    fn speed_on(&self, path: &[[f64; 3]], speed: f64) -> f64 {
        match &self.ramps {
            Some(r) => r.speed_on(path, speed),
            None => speed,
        }
    }

    /// Routes this transporter's travels with `router` instead of the simulation's one
    /// (movement modes: free path, network with access points). `None` = the simulation's.
    /// AGVs under traffic control still follow the network edge by edge.
    pub fn router(mut self, router: Option<Arc<dyn Router>>) -> Self {
        self.router = router;
        self
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

    /// Home somewhere else than the block's own position (an own point, an object, an
    /// area). The transporter returns there when idle if `home` is on.
    pub fn home_point(mut self, point: Option<[f64; 3]>) -> Self {
        self.home_point = point;
        self
    }

    /// Break place: when the shift pauses with nothing to do, the member walks there
    /// (inside the break, so it takes no work time) and walks back (home, or where it
    /// was) when the shift resumes; a task that arrives meanwhile interrupts that walk.
    pub fn break_point(mut self, point: Option<[f64; 3]>) -> Self {
        self.break_point = point;
        self
    }

    /// Picks items up at the output end of a block and drops them at its input end (a
    /// belt's ends, [`utsim_core::Sim::set_ends`]) instead of going to its centre.
    pub fn use_ends(mut self, on: bool) -> Self {
        self.use_ends = on;
        self
    }

    /// Where home is.
    fn home_of(&self, cx: &Ctx) -> [f64; 3] {
        self.home_point.unwrap_or_else(|| cx.position(cx.me()))
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
        let durs = segment_durations(
            &lengths,
            self.cur_speed,
            self.acceleration,
            self.deceleration,
        );
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
                cross: None,
                stop: false,
            });
        }
        Some(segs)
    }

    /// Starts moving to `target` in travel state `state`; `tag` fires on arrival.
    fn travel(&mut self, cx: &mut Ctx, target: [f64; 3], tag: u32, state: State) {
        let load = if state == State::TravelLoaded {
            self.load_kg(cx)
        } else {
            0.0
        };
        self.cur_speed = cx.walk_speed(self.speed, load);
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
        let path = match &self.router {
            Some(r) => r.route(self.position, target),
            None => cx.route(self.position, target),
        };
        let speed = self.speed_on(&path, self.cur_speed);
        if let Some((board, vehicle)) = self.crossings.clone() {
            let plan = crossing_plan(
                &path,
                &board.lock().unwrap_or_else(|e| e.into_inner()),
                vehicle,
                speed,
                self.acceleration,
                self.deceleration,
            );
            if let Some(pieces) = plan.filter(|p| !p.is_empty()) {
                let segs = pieces
                    .into_iter()
                    .map(|p| Seg {
                        a: p.a,
                        b: p.b,
                        edge: None,
                        dur: p.dur,
                        cross: p.cross,
                        stop: p.stop,
                    })
                    .collect();
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
        let t = travel_time(
            path_length(&path),
            speed,
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
        if self.cross_wait(cx) {
            return;
        }
        let Some(traffic) = self.traffic.clone() else {
            self.run_segment(cx);
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

    /// At the start of a crossing zone: asks the board. When it must wait, a standing
    /// piece is inserted before the zone and started (`true`); a vehicle asks again after
    /// it, a pedestrian is already booked.
    fn cross_wait(&mut self, cx: &mut Ctx) -> bool {
        let (Some((board, vehicle)), Some(route)) = (self.crossings.clone(), &mut self.route)
        else {
            return false;
        };
        let seg = route.segs[route.idx];
        let Some((k, zone)) = seg.cross else {
            return false;
        };
        let (now, me) = (cx.now(), cx.me());
        let mut b = board.lock().unwrap_or_else(|e| e.into_inner());
        let wait = if vehicle {
            match b.vehicle_enter(k, me, now, zone) {
                Ok(()) => 0.0,
                Err(t) => {
                    b.record_wait(k, t - now);
                    t - now
                }
            }
        } else {
            b.pedestrian_enter(k, me, now, zone) - now
        };
        drop(b);
        if !vehicle || wait <= 0.0 {
            route.segs[route.idx].cross = None;
        }
        if wait <= 0.0 {
            return false;
        }
        route.segs.insert(
            route.idx,
            Seg {
                a: seg.a,
                b: seg.a,
                edge: None,
                dur: wait,
                cross: None,
                stop: true,
            },
        );
        self.run_segment(cx);
        true
    }

    /// Moves along the current piece (its edge, if any, is already held).
    fn run_segment(&mut self, cx: &mut Ctx) {
        let Some(route) = &self.route else { return };
        let seg = route.segs[route.idx];
        if seg.stop {
            cx.set_state(State::Blocked);
        } else if route.idx > 0 && route.segs[route.idx - 1].stop {
            cx.set_state(route.state);
        }
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
        if let Some((board, _)) = &self.crossings {
            board
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .forget(cx.me(), cx.now());
        }
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
                let target = if self.use_ends {
                    cx.output_end(from)
                } else {
                    cx.position(from)
                };
                self.travel(cx, target, ARRIVED_PICKUP, State::TravelEmpty);
            }
            TaskKind::Operate { station, .. } => {
                // Cases handed over together (an idle report) open together.
                let limit = self.multitask.as_ref().map_or(1, |m| m.max_open);
                while self.on_case() && self.open_cases() < limit {
                    let Some(i) = self.queue.iter().position(|r| {
                        matches!(r.kind, TaskKind::Operate { tag, .. } if tag != MAINTENANCE_TAG)
                    }) else {
                        break;
                    };
                    let case = self.queue.remove(i).expect("index in range");
                    self.others.push_back((case, false));
                }
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
            let home = self.home_of(cx);
            if distance(self.position, home) > 0.0 {
                self.travel(cx, home, ARRIVED_HOME, State::TravelEmpty);
            }
        }
    }

    fn finish(&mut self, cx: &mut Ctx) {
        self.trip.clear();
        self.k = 0;
        if cx.is_closing() {
            // End of shift with `Finish`: no new work and no idle report, so the
            // dispatcher gives the next task to someone on shift. The kernel takes the
            // transporter off shift right after this.
            self.announce_on_resume = true;
            cx.set_state(State::Idle);
            return;
        }
        if let Some(rest) = cx.take_rest() {
            // A tired person pauses before the next task (the dispatcher waits for it).
            cx.set_state(State::Resting);
            self.arm(cx, rest, RESTED, None);
            return;
        }
        self.next_task(cx);
    }

    /// Free after a task (or a pause): the next trip, or idle.
    fn next_task(&mut self, cx: &mut Ctx) {
        if cx.is_closing() {
            self.announce_on_resume = true;
            cx.set_state(State::Idle);
            return;
        }
        if self.queue.is_empty() {
            self.go_idle(cx, true);
        } else if !cx.is_down() {
            self.begin_trip(cx);
        }
    }

    /// A task arrived: if idle, start after the other same-instant tasks arrived.
    fn kick(&mut self, cx: &mut Ctx) {
        if self.trip.is_empty()
            && self.timer.is_none()
            && !self.queue.is_empty()
            && !cx.is_down()
            && !cx.is_closing()
        {
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

    /// Whether the current work can be given back: travelling empty to the first stop
    /// of the trip (nothing loaded yet), or about to start it.
    fn travelling_empty(&self, cx: &Ctx, below: i32) -> bool {
        if cx.is_down() || self.k != 0 || !self.others.is_empty() {
            return false;
        }
        if self
            .trip
            .iter()
            .chain(&self.queue)
            .any(|r| r.priority >= below)
        {
            return false;
        }
        let tag = match (&self.route, &self.timer) {
            (Some(route), _) => Some(route.tag),
            (None, Some(timer)) => Some(timer.tag),
            (None, None) => None,
        };
        match tag {
            Some(ARRIVED_PICKUP | ARRIVED_STATION) => !self.trip.is_empty(),
            Some(START) => self.trip.is_empty() && !self.queue.is_empty(),
            _ => false,
        }
    }

    /// Whether the member works only on station jobs (not maintenance) of priority below
    /// `below`, which it may give back mid-work ([`MSG_PREEMPT_FULL`]): at a station, walking
    /// to one or turning from one case to another, never loaded nor resting.
    fn on_lower_cases(&self, cx: &Ctx, below: i32) -> bool {
        if cx.is_down() || self.k != 0 || self.route.is_some() {
            return false;
        }
        if self
            .timer
            .as_ref()
            .is_some_and(|t| t.tag == RESTED || t.tag == START)
        {
            return false;
        }
        let cases: Vec<&TaskRequest> = self
            .trip
            .iter()
            .chain(self.others.iter().map(|(r, _)| r))
            .collect();
        !cases.is_empty()
            && cases
                .iter()
                .all(|r| matches!(r.kind, TaskKind::Operate { tag, .. } if tag != MAINTENANCE_TAG))
            && cases
                .iter()
                .copied()
                .chain(self.queue.iter())
                .all(|r| r.priority < below)
    }

    /// Gives back the station jobs in progress (and the queued tasks) for a more urgent
    /// request: the stations keep the time left of the jobs already started (see
    /// [`MSG_PREEMPT_FULL`]); then the member reports idle from where it is.
    fn give_back_cases(&mut self, cx: &mut Ctx, dispatcher: BlockId) {
        if let Some(timer) = self.timer.take() {
            if let Some(event) = timer.event {
                cx.cancel(event);
            }
            if let Some(leg) = &timer.leg {
                self.position = Self::progress(leg, cx.now()).0;
                if leg.duration > 0.0 {
                    cx.trace_travel(vec![self.position, self.position], 0.0);
                }
            }
        }
        let cases: Vec<TaskRequest> = self
            .trip
            .drain(..)
            .chain(self.others.drain(..).map(|(r, _)| r))
            .collect();
        for c in &cases {
            if let TaskKind::Operate { station, .. } = c.kind
                && self.started.contains(&c.id)
            {
                cx.send(station, MSG_WITHDRAW, c.id.0);
            }
        }
        self.started.clear();
        self.k = 0;
        let now = cx.now();
        self.switching.set(now, 0.0);
        self.open.set(now, 0.0);
        let queued: Vec<TaskRequest> = self.queue.drain(..).collect();
        for t in cases.iter().chain(&queued) {
            cx.send(dispatcher, MSG_PREEMPTED, t.id.0);
        }
        self.go_idle(cx, true);
    }

    /// Gives the current trip (and queued tasks) back to the dispatcher and reports idle
    /// from the point reached (see [`MSG_PREEMPT`]).
    fn give_back(&mut self, cx: &mut Ctx, dispatcher: BlockId) {
        if let Some(timer) = self.timer.take() {
            if let Some(event) = timer.event {
                cx.cancel(event);
            }
            if let Some(leg) = &timer.leg {
                self.position = Self::progress(leg, cx.now()).0;
                if leg.duration > 0.0 {
                    cx.trace_travel(vec![self.position, self.position], 0.0);
                }
            }
        }
        if self.route.is_some() {
            self.abandon_route(cx);
        }
        let tasks: Vec<TaskRequest> = self.trip.drain(..).chain(self.queue.drain(..)).collect();
        self.k = 0;
        for t in tasks {
            cx.send(dispatcher, MSG_PREEMPTED, t.id.0);
        }
        self.go_idle(cx, true);
    }

    /// Off shift with nothing to do: walk to the break place (the walk is drawn in the
    /// 3D view; the member is stopped, so it takes no time of the run).
    fn walk_to_break(&mut self, cx: &mut Ctx) {
        let Some(spot) = self.break_point else { return };
        let back = if self.home {
            self.home_of(cx)
        } else {
            self.position
        };
        if distance(self.position, spot) > 0.0 {
            let path = match &self.router {
                Some(r) => r.route(self.position, spot),
                None => cx.route(self.position, spot),
            };
            let t = travel_time(
                path_length(&path),
                self.speed_on(&path, self.speed),
                self.acceleration,
                self.deceleration,
            );
            cx.trace_travel(path, t);
            self.position = spot;
        }
        self.back_from_break = Some(back);
    }

    /// Weight (kg) of the items still carried (from `self.k` on), for a person.
    fn load_kg(&self, cx: &Ctx) -> f64 {
        if !cx.is_person() {
            return 0.0;
        }
        self.trip[self.k.min(self.trip.len())..]
            .iter()
            .filter_map(|r| match r.kind {
                TaskKind::Transport { item, .. } => Some(cx.person_weight(item)),
                TaskKind::Operate { .. } => None,
            })
            .sum()
    }

    /// Duration of a load or unload of standard time `t` (pace of a person).
    fn handling_time(cx: &mut Ctx, task: &str, t: Time) -> Time {
        let me = cx.me();
        cx.person_time(&[me], task, t)
    }

    /// Drives to the destination of the item at `self.k`.
    fn deliver_leg(&mut self, cx: &mut Ctx) {
        if let TaskKind::Transport { to, .. } = self.trip[self.k].kind {
            let target = if self.use_ends {
                cx.input_end(to)
            } else {
                cx.position(to)
            };
            self.travel(cx, target, ARRIVED_DROP, State::TravelLoaded);
        }
    }
}

impl Behavior for Transporter {
    fn kind(&self) -> &'static str {
        "transporter"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.back_from_break = None;
        self.queue.clear();
        self.trip.clear();
        self.k = 0;
        self.timer = None;
        self.others.clear();
        self.started.clear();
        self.switches = 0;
        self.switching = TimeWeighted::default();
        self.open = TimeWeighted::default();
        self.abandon_route(cx);
        self.position = cx.position(cx.me());
        cx.set_state(State::Idle);
    }

    /// `position`: idle there at `t = 0` (with `home`, it drives home at once). Tasks
    /// are not part of the state: the objects that need it ask again when they restart.
    fn restore(&mut self, cx: &mut Ctx, state: &BlockState) -> Result<(), String> {
        state.check(
            "Transporter",
            Accepts {
                position: true,
                ..Accepts::default()
            },
        )?;
        let Some(p) = state.position else {
            return Ok(());
        };
        if !p.iter().all(|v| v.is_finite()) {
            return Err(format!("position {p:?} precisa ter coordenadas finitas"));
        }
        self.position = p;
        self.report_position(cx);
        let home = self.home_of(cx);
        if self.home && distance(p, home) > 0.0 {
            self.travel(cx, home, ARRIVED_HOME, State::TravelEmpty);
        }
        Ok(())
    }

    fn save_state(&self, cx: &Ctx, state: &mut BlockState) -> Option<String> {
        let position = match &self.timer {
            Some(Timer {
                event: Some(_),
                leg: Some(leg),
                ..
            }) => Self::progress(leg, cx.now()).0,
            _ => self.position,
        };
        state.position = Some(position);
        let carried = state.items.len();
        state.items.clear();
        let busy = !self.trip.is_empty() || !self.queue.is_empty() || !self.others.is_empty();
        match (carried, busy) {
            (0, false) => None,
            (0, true) => Some(
                "as tarefas em curso não entram no estado gravado (os objetos que as pediram pedem de novo)"
                    .into(),
            ),
            (n, _) => Some(format!(
                "{n} item(ns) carregados pelo transportador não entram no estado gravado; as tarefas recomeçam"
            )),
        }
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_task(&mut self, cx: &mut Ctx, request: TaskRequest) {
        if self.takes_case(&request) {
            self.others.push_back((request, false));
            self.open.set(cx.now(), self.open_cases() as f64);
            self.arm_slice(cx);
            return;
        }
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
            if cx.is_closing() {
                // The shift ended (`Finish`) before the trip started: it waits.
                cx.set_state(State::Idle);
            } else {
                self.begin_trip(cx);
            }
            return;
        }
        if tag == ARRIVED_HOME {
            // Home is wherever the trip home ends, which need not be the block position:
            // a router may stop elsewhere (the access point of an object placed on the
            // same point). Driving "home" again from there would be a zero-length trip
            // repeated forever at the same instant.
            cx.set_state(State::Idle);
            self.report_position(cx);
            return;
        }
        if tag == RESTED {
            self.next_task(cx);
            return;
        }
        if tag == SLICE {
            let Some(current) = self.trip.first().copied() else {
                return;
            };
            if self.others.is_empty() {
                return;
            }
            if let TaskKind::Operate { station, .. } = current.kind {
                cx.send(station, MSG_SUSPEND, current.id.0);
            }
            self.others.push_back((current, true));
            self.trip.clear();
            self.begin_switch(cx);
            return;
        }
        if tag == SWITCHED {
            self.switching.set(cx.now(), 0.0);
            let Some((next, _)) = self.others.pop_front() else {
                self.finish(cx);
                return;
            };
            self.trip = vec![next];
            self.k = 0;
            if let TaskKind::Operate { station, .. } = next.kind {
                let target = cx.position(station);
                self.travel(cx, target, ARRIVED_STATION, State::TravelEmpty);
            }
            return;
        }
        let Some(&task) = self.trip.get(self.k) else {
            return;
        };
        match (tag, task.kind) {
            (ARRIVED_PICKUP, TaskKind::Transport { .. }) => {
                cx.set_state(State::Loading);
                let t = cx.sample_duration(&self.load_time);
                let t = Self::handling_time(cx, TASK_LOAD, t);
                self.arm(cx, t, LOADED, None);
            }
            (LOADED, TaskKind::Transport { item, .. }) => {
                cx.pick_up(item);
                if self.k + 1 < self.trip.len() {
                    self.k += 1;
                    let t = cx.sample_duration(&self.load_time);
                    let t = Self::handling_time(cx, TASK_LOAD, t);
                    self.arm(cx, t, LOADED, None);
                } else {
                    self.k = 0;
                    self.deliver_leg(cx);
                }
            }
            (ARRIVED_DROP, TaskKind::Transport { .. }) => {
                cx.set_state(State::Unloading);
                let t = cx.sample_duration(&self.unload_time);
                let t = Self::handling_time(cx, TASK_UNLOAD, t);
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
                if self.started.contains(&task.id) {
                    cx.send(station, MSG_RESUME_WORK, task.id.0);
                } else {
                    cx.send(station, MSG_ARRIVED, task.id.0);
                    // Remembered also without multitasking: a preemption tells the
                    // station the job was left (`MSG_WITHDRAW`).
                    self.started.push(task.id);
                    if self.multitask.is_some() {
                        self.open.set(cx.now(), self.open_cases() as f64);
                    }
                }
                self.arm_slice(cx);
            }
            _ => {}
        }
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        if cx.away_on_finish(cx.me()) {
            // The dispatcher skips it while away; report back when the shift returns.
            self.announce_on_resume = true;
        }
        let now = cx.now();
        if self.timer.is_none() && self.trip.is_empty() && cx.is_off_shift() {
            self.walk_to_break(cx);
            return;
        }
        if self
            .timer
            .as_ref()
            .is_some_and(|t| t.tag == ARRIVED_HOME && t.event.is_some())
            && self.trip.is_empty()
            && cx.is_off_shift()
            && self.break_point.is_some()
        {
            // Driving home when the break starts: go to the break place instead.
            self.abandon_homing(cx);
            self.walk_to_break(cx);
            return;
        }
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
        let resting = self.timer.as_ref().is_some_and(|t| t.tag == RESTED);
        if std::mem::take(&mut self.announce_on_resume)
            && self.trip.is_empty()
            && self.queue.is_empty()
            && !resting
            && let Some(d) = self.dispatcher
        {
            // Back from a shift that ended with `Finish`: free for the dispatcher again.
            self.report_position(cx);
            cx.send(d, MSG_IDLE, self.capacity as u64);
        }
        if let Some(back) = self.back_from_break.take() {
            // Back from the break place: walk home (or to where it was) unless work waits,
            // which then starts from here. A new task interrupts the walk.
            if self.queue.is_empty()
                && self.trip.is_empty()
                && self.timer.is_none()
                && distance(self.position, back) > 0.0
            {
                self.report_position(cx);
                self.travel(cx, back, ARRIVED_HOME, State::TravelEmpty);
                return;
            }
        }
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

    /// The current trip (or station job) is work to finish; queued tasks and the drive
    /// home are not.
    fn work_in_progress(&self, _cx: &Ctx) -> bool {
        !self.trip.is_empty() || !self.others.is_empty()
    }

    fn on_warmup(&mut self, cx: &mut Ctx) {
        self.switches = 0;
        self.switching.reset(cx.now());
        self.open.reset(cx.now());
    }

    /// With multitasking: `multitask.switches` (turns from one case to another),
    /// `multitask.switching` (fraction of the window spent turning) and `multitask.open`
    /// (mean open cases).
    fn custom_metrics(&self, now: Time) -> Vec<(String, f64)> {
        if self.multitask.is_none() {
            return Vec::new();
        }
        vec![
            ("multitask.switches".to_string(), self.switches as f64),
            ("multitask.switching".to_string(), self.switching.mean(now)),
            ("multitask.open".to_string(), self.open.mean(now)),
        ]
    }

    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
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
        if tag == MSG_PREEMPT || tag == MSG_PREEMPT_FULL {
            let Some(d) = self.dispatcher.filter(|d| *d == from) else {
                return;
            };
            let below = payload as i64 as i32;
            if self.travelling_empty(cx, below) {
                self.give_back(cx, d);
            } else if tag == MSG_PREEMPT_FULL && self.on_lower_cases(cx, below) {
                self.give_back_cases(cx, d);
            } else {
                cx.send(d, MSG_PREEMPT_REFUSED, payload);
            }
            return;
        }
        if tag != MSG_RELEASE {
            return;
        }
        self.started.retain(|r| r.0 != payload);
        if let Some(at) = self.others.iter().position(|(r, _)| r.id.0 == payload) {
            // A case set aside was released (its other operators finished it).
            self.others.remove(at);
            self.open.set(cx.now(), self.open_cases() as f64);
            if let Some(d) = self.dispatcher {
                cx.send(d, MSG_CASE_DONE, payload);
            }
            return;
        }
        let releases_current = self
            .trip
            .first()
            .is_some_and(|r| r.id.0 == payload && matches!(r.kind, TaskKind::Operate { .. }));
        if !releases_current {
            return;
        }
        self.drop_slice(cx);
        if self.others.is_empty() {
            self.open.set(cx.now(), 0.0);
            self.finish(cx);
            return;
        }
        self.trip.clear();
        self.k = 0;
        if let Some(d) = self.dispatcher {
            cx.send(d, MSG_CASE_DONE, payload);
        }
        self.begin_switch(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_core::{OutputRule, RunConfig, Simulation, StartState};

    /// Straight lines, except that going to the origin stops at `(1, 0, 0)`, like an
    /// access point of an object placed at the origin.
    struct StopsBeside;

    impl Router for StopsBeside {
        fn route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]> {
            let to = if to == [0.0; 3] { [1.0, 0.0, 0.0] } else { to };
            if from == to {
                vec![from]
            } else {
                vec![from, to]
            }
        }
    }

    #[test]
    fn homing_ends_where_the_router_stops() {
        let mut sim = Simulation::new(RunConfig {
            stop_time: 10.0,
            warmup: 0.0,
            seed: 1,
            replication: 0,
            sample_interval: None,
            histogram_bins: None,
        });
        let t = Transporter::new(1.0)
            .home(true)
            .router(Some(Arc::new(StopsBeside)));
        let f = sim.add_block("f", OutputRule::default(), Box::new(t));
        sim.set_position(f, [0.0; 3]);
        // Starts at (5, 0, 0): drives "home", which ends at (1, 0, 0) after 4.
        sim.set_start_state(StartState {
            clock: 0.0,
            blocks: vec![(
                f,
                BlockState {
                    position: Some([5.0, 0.0, 0.0]),
                    ..BlockState::default()
                },
            )],
        });
        let r = sim
            .run()
            .expect("no same-instant loop at the stop beside home");
        let states = &r.block("f").unwrap().states;
        assert!((states["travel_empty"] - 0.4).abs() < 1e-12, "{states:?}");
        assert!((states["idle"] - 0.6).abs() < 1e-12, "{states:?}");
    }
}
