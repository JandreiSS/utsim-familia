use std::collections::VecDeque;

use utsim_core::task::{MSG_ARRIVED, MSG_IDLE, MSG_RELEASE, path_length, travel_time};
use utsim_core::{Behavior, BlockId, Ctx, Dist, Item, ItemId, State, TaskKind, TaskRequest, Time};

const ARRIVED_PICKUP: u32 = 0;
const LOADED: u32 = 1;
const ARRIVED_DROP: u32 = 2;
const UNLOADED: u32 = 3;
const ARRIVED_STATION: u32 = 4;

/// Mobile resource: carries items between blocks (forklift, AGV, person) and works
/// at stations as an operator. Reference implementation of the phase 4 contract:
/// one task at a time, highest priority first (FIFO among equals), travel along
/// `Ctx::route` with a trapezoidal speed profile.
pub struct Transporter {
    speed: f64,
    acceleration: Option<f64>,
    deceleration: Option<f64>,
    load_time: Dist,
    unload_time: Dist,
    dispatcher: Option<BlockId>,
    queue: VecDeque<TaskRequest>,
    current: Option<TaskRequest>,
    position: [f64; 3],
}

impl Transporter {
    pub fn new(speed: f64) -> Self {
        Transporter {
            speed,
            acceleration: None,
            deceleration: None,
            load_time: Dist::Const(0.0),
            unload_time: Dist::Const(0.0),
            dispatcher: None,
            queue: VecDeque::new(),
            current: None,
            position: [0.0; 3],
        }
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

    /// The dispatcher this transporter reports to when it becomes free.
    pub fn dispatcher(mut self, dispatcher: Option<BlockId>) -> Self {
        self.dispatcher = dispatcher;
        self
    }

    /// Starts moving to `target`; `tag` fires on arrival.
    fn travel(&mut self, cx: &mut Ctx, target: [f64; 3], tag: u32, item: Option<ItemId>) {
        let path = cx.route(self.position, target);
        let t: Time = travel_time(
            path_length(&path),
            self.speed,
            self.acceleration,
            self.deceleration,
        );
        if t > 0.0 {
            cx.trace_travel(path, t);
        }
        self.position = target;
        cx.schedule(t, tag, item);
    }

    fn start_next(&mut self, cx: &mut Ctx) {
        let Some(request) = self.queue.pop_front() else {
            cx.set_state(State::Idle);
            if let Some(d) = self.dispatcher {
                cx.send(d, MSG_IDLE, 0);
            }
            return;
        };
        self.current = Some(request);
        match request.kind {
            TaskKind::Transport { item, from, .. } => {
                cx.set_state(State::TravelEmpty);
                let target = cx.position(from);
                self.travel(cx, target, ARRIVED_PICKUP, Some(item));
            }
            TaskKind::Operate { station, .. } => {
                cx.set_state(State::TravelEmpty);
                let target = cx.position(station);
                self.travel(cx, target, ARRIVED_STATION, None);
            }
        }
    }

    fn finish(&mut self, cx: &mut Ctx) {
        self.current = None;
        self.start_next(cx);
    }
}

impl Behavior for Transporter {
    fn kind(&self) -> &'static str {
        "transporter"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.queue.clear();
        self.current = None;
        self.position = cx.position(cx.me());
        cx.set_state(State::Idle);
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_task(&mut self, cx: &mut Ctx, request: TaskRequest) {
        let at = self
            .queue
            .iter()
            .position(|q| q.priority < request.priority)
            .unwrap_or(self.queue.len());
        self.queue.insert(at, request);
        if self.current.is_none() {
            self.start_next(cx);
        }
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, _item: Option<ItemId>) {
        let Some(request) = self.current else { return };
        match (tag, request.kind) {
            (ARRIVED_PICKUP, TaskKind::Transport { item, .. }) => {
                cx.set_state(State::Loading);
                let t = cx.sample_duration(&self.load_time);
                cx.schedule(t, LOADED, Some(item));
            }
            (LOADED, TaskKind::Transport { item, to, .. }) => {
                cx.pick_up(item);
                cx.set_state(State::TravelLoaded);
                let target = cx.position(to);
                self.travel(cx, target, ARRIVED_DROP, None);
            }
            (ARRIVED_DROP, TaskKind::Transport { .. }) => {
                cx.set_state(State::Unloading);
                let t = cx.sample_duration(&self.unload_time);
                cx.schedule(t, UNLOADED, None);
            }
            (UNLOADED, TaskKind::Transport { item, to, port, .. }) => {
                cx.drop_off(item, to, port);
                self.finish(cx);
            }
            (ARRIVED_STATION, TaskKind::Operate { station, .. }) => {
                cx.set_state(State::Utilize);
                cx.send(station, MSG_ARRIVED, request.id.0);
            }
            _ => {}
        }
    }

    fn on_message(&mut self, cx: &mut Ctx, _from: BlockId, tag: u32, payload: u64) {
        let releases_current = self
            .current
            .is_some_and(|r| r.id.0 == payload && matches!(r.kind, TaskKind::Operate { .. }));
        if tag == MSG_RELEASE && releases_current {
            self.finish(cx);
        }
    }
}
