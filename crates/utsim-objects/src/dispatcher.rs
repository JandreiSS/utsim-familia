use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use utsim_core::task::{MSG_IDLE, distance, is_case_tag};
use utsim_core::{Behavior, BlockId, Ctx, Dist, Item, State, TaskKind, TaskRequest, Time};

use crate::transporter::{
    MSG_CASE_DONE, MSG_POSITION_X, MSG_POSITION_Y, MSG_POSITION_Z, MSG_PREEMPT, MSG_PREEMPT_FULL,
    MSG_PREEMPT_REFUSED, MSG_PREEMPTED,
};

/// How a dispatcher chooses among free team members.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchRule {
    /// The first free member in team order.
    #[default]
    FirstAvailable,
    /// Free members in turn.
    RoundRobin,
    /// The free member closest to where the work starts.
    Nearest,
    /// The free member with the least busy time so far (from assignment to its next
    /// idle report).
    LeastBusy,
    /// The free member that has been free the longest.
    LongestIdle,
    /// A free member at random (the dispatcher's own random stream).
    Random,
}

/// Which waiting task a member takes when it becomes free.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPick {
    /// The head of the queue: highest priority, then first come.
    #[default]
    Priority,
    /// Among the tasks of the highest waiting priority, the one that starts closest to
    /// the member (ties: first come).
    Nearest,
    /// The task that starts closest to the member, whatever its priority (ties: first
    /// come). Far or low-priority tasks may wait a long time.
    NearestAny,
}

/// When a dispatcher takes work away from a busy member for a more urgent request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preempt {
    /// Never: the request waits for a member to become free.
    #[default]
    Never,
    /// A member travelling empty to work of lower priority (nothing loaded, not yet at
    /// the station) gives it back and takes the new request; the work given back goes to
    /// the front of its priority in the queue. A loaded member is never interrupted.
    Empty,
    /// As `Empty`, and a member working at stations on jobs of lower priority also leaves
    /// them for the new request (an interruption): each station keeps the time left and
    /// waits for an operator again, and the jobs go back to the front of their priority in
    /// the queue. A loaded transport or a maintenance job is still never interrupted.
    Full,
}

impl Preempt {
    pub fn is_never(&self) -> bool {
        *self == Preempt::Never
    }
}

impl TaskPick {
    pub fn is_priority(&self) -> bool {
        *self == TaskPick::Priority
    }
}

/// Hands tasks to a team of transporters/operators. Tasks wait here while every
/// member is busy, highest priority first (FIFO among equals).
///
/// **Rules** (applied when a task arrives and at least one member is free):
/// - `FirstAvailable`: the first free member in team order.
/// - `RoundRobin`: free members in turn, starting after the last one assigned.
/// - `Nearest`: the free member closest (straight-line distance, ignoring any path
///   network) to where the task starts: the `from` block of a transport, or the
///   station of an operation. Ties go to the earlier team member.
/// - `LeastBusy`: the free member with the least accumulated busy time (assignment to
///   idle report, as seen by the dispatcher). Ties go to the earlier team member.
/// - `LongestIdle`: the free member whose last idle report is the oldest (at the start
///   every member is free since 0, so ties go to the earlier team member).
/// - `Random`: a uniformly random free member, from the dispatcher's own random stream
///   (one draw per choice with two or more candidates; same seed, same choices).
///
/// When a member reports `MSG_IDLE` and tasks are waiting, it receives a task whatever
/// the rule (it is the only free member): the head of the queue with `pick: Priority`,
/// or the nearest one with [`TaskPick::Nearest`] / [`TaskPick::NearestAny`]. If it can carry more
/// than one item (the idle payload is its capacity) it also receives, in queue
/// order, further waiting `Transport` tasks that start at the same block, up to its
/// capacity, so it can serve them in one trip.
///
/// **Skills.** A request that needs a skill ([`Ctx::qualified`]) goes only to members
/// whose person profile lists it: the rules choose among the qualified free members, and
/// a member that reports idle takes the first waiting task it is qualified for. Without
/// skills this is exactly the behaviour above.
///
/// **Preemption** ([`Preempt::Empty`]). When a request waits and no member is free, the
/// dispatcher asks one busy member whose work all has a lower priority than the head of
/// the queue (the one with the lowest such priority; ties: team order) to give it back.
/// A member travelling empty to its first stop does so ([`MSG_PREEMPTED`] for each task,
/// then its idle report from the point reached); otherwise it refuses and the next
/// candidate is asked. One question at a time; a member that refused is not asked again
/// before its next idle report. Work given back goes to the front of its priority.
///
/// **Position protocol.** Members send `MSG_POSITION_X/Y/Z` (coordinate as
/// `f64::to_bits`) before `MSG_IDLE`; until the first report the dispatcher uses the
/// member's block position. A member that is broken or off shift still counts as
/// free: its task waits in its own queue until it is back.
///
/// **Multitasking members** ([`Dispatcher::open_limits`]). A member that may hold
/// several cases also takes an operator job (not maintenance) while busy, as long as all
/// its work is operator jobs and it holds fewer than its limit: when no member is free,
/// the rule chooses among such members. On its idle report it takes the first waiting
/// task and then, if that is an operator job, further waiting operator jobs up to its
/// limit; when it reports a case done ([`MSG_CASE_DONE`]) it takes one more.
pub struct Dispatcher {
    team: Vec<BlockId>,
    rule: DispatchRule,
    pick: TaskPick,
    preempt: Preempt,
    free: Vec<bool>,
    /// Tasks each member is working on (forwarded and not yet reported done).
    assigned: Vec<Vec<TaskRequest>>,
    /// Member asked to give its work back, waiting for the answer.
    asking: Option<usize>,
    /// Members that refused since their last idle report.
    refused: Vec<bool>,
    /// Preemptions since the warm-up, and whether the member asked gave work back.
    preemptions: u64,
    gave_back: bool,
    /// Team metrics on (see [`Dispatcher::metrics`]).
    metrics: bool,
    /// Member names, for the metric names.
    names: Vec<String>,
    /// When each waiting request reached the dispatcher (until its first assignment).
    since: HashMap<u64, Time>,
    /// Requests received and assigned per member, since the warm-up.
    requests: u64,
    served: Vec<u64>,
    /// Start of the statistics window and the busy time of each member at that moment.
    window_from: Time,
    busy_at_window: Vec<Time>,
    /// Busy time of each member so far, and when its current work was assigned.
    busy: Vec<Time>,
    assigned_at: Vec<Time>,
    /// When each member last became free.
    idle_since: Vec<Time>,
    /// Last known position of each member.
    positions: Vec<[f64; 3]>,
    /// Items each member can carry per trip, learned from its idle reports.
    capacity: Vec<usize>,
    /// Where RoundRobin starts looking.
    next: usize,
    queue: VecDeque<TaskRequest>,
    /// Cases each member may hold at once (1: one task at a time).
    open_limit: Vec<usize>,
}

impl Dispatcher {
    pub fn new(team: Vec<BlockId>, rule: DispatchRule) -> Self {
        let n = team.len();
        Dispatcher {
            team,
            rule,
            pick: TaskPick::Priority,
            preempt: Preempt::Never,
            free: vec![true; n],
            assigned: vec![Vec::new(); n],
            asking: None,
            refused: vec![false; n],
            preemptions: 0,
            gave_back: false,
            metrics: false,
            names: Vec::new(),
            since: HashMap::new(),
            requests: 0,
            served: vec![0; n],
            window_from: 0.0,
            busy_at_window: vec![0.0; n],
            busy: vec![0.0; n],
            assigned_at: vec![0.0; n],
            idle_since: vec![0.0; n],
            positions: vec![[0.0; 3]; n],
            capacity: vec![1; n],
            next: 0,
            queue: VecDeque::new(),
            open_limit: vec![1; n],
        }
    }

    /// Cases each member may hold at once, in team order (missing or 0: 1). See the
    /// multitasking paragraph of [`Dispatcher`].
    pub fn open_limits(mut self, limits: Vec<usize>) -> Self {
        for (m, l) in limits.into_iter().enumerate().take(self.team.len()) {
            self.open_limit[m] = l.max(1);
        }
        self
    }

    /// Whether `request` is an operator job a multitasking member may interleave.
    fn is_case(request: &TaskRequest) -> bool {
        matches!(request.kind, TaskKind::Operate { tag, .. } if is_case_tag(tag))
    }

    /// Busy member `m` has room for one more case.
    fn has_room(&self, m: usize) -> bool {
        !self.free[m]
            && self.assigned[m].len() < self.open_limit[m]
            && self.assigned[m].iter().all(Self::is_case)
    }

    /// Whether `request` may join the work of member `m` as one more open case: only work
    /// of the same priority is interleaved. A more urgent request is not sliced with the
    /// cases in hand (it waits for a free member or takes one by preemption), and urgent
    /// work taken that way is done alone.
    fn joins(&self, m: usize, request: &TaskRequest) -> bool {
        self.assigned[m]
            .iter()
            .all(|r| request.priority == r.priority)
    }

    /// Hands member `m` waiting operator jobs while it has room for them.
    fn fill_room(&mut self, cx: &mut Ctx, m: usize) {
        while self.has_room(m) && !cx.away_on_finish(self.team[m]) {
            let me = self.team[m];
            let Some(i) = (0..self.queue.len()).find(|&i| {
                Self::is_case(&self.queue[i])
                    && cx.qualified(me, &self.queue[i])
                    && self.joins(m, &self.queue[i])
            }) else {
                return;
            };
            let request = self.queue.remove(i).expect("index in range");
            self.assign(cx, m, request);
        }
    }

    pub fn rule(&self) -> DispatchRule {
        self.rule
    }

    /// Which waiting task a member that becomes free takes (see [`TaskPick`]).
    pub fn pick(mut self, pick: TaskPick) -> Self {
        self.pick = pick;
        self
    }

    /// Team metrics in the report: the tally `request_wait` (from the request reaching
    /// the dispatcher to its first assignment to a member), `requests.count` and, per
    /// member, `member.<name>.requests` (tasks handed to it) and `member.<name>.busy`
    /// (fraction of the window between assignment and idle report).
    pub fn metrics(mut self, on: bool) -> Self {
        self.metrics = on;
        self
    }

    /// Busy time of member `m` up to `now`, the work in progress included.
    fn busy_until(&self, m: usize, now: Time) -> Time {
        self.busy[m]
            + if self.free[m] {
                0.0
            } else {
                now - self.assigned_at[m]
            }
    }

    /// When to take work away from a busy member (see [`Preempt`]).
    pub fn preempt(mut self, preempt: Preempt) -> Self {
        self.preempt = preempt;
        self
    }

    fn assign(&mut self, cx: &mut Ctx, member: usize, request: TaskRequest) {
        if self.free[member] {
            self.assigned_at[member] = cx.now();
        }
        self.free[member] = false;
        self.assigned[member].push(request);
        if self.metrics {
            self.served[member] += 1;
            if let Some(t) = self.since.remove(&request.id.0) {
                cx.record("request_wait", cx.now() - t);
            }
        }
        cx.forward_task(self.team[member], request);
    }

    /// Puts `request` in the queue after the tasks of higher or equal priority (`front`:
    /// before the equal ones).
    fn enqueue(&mut self, request: TaskRequest, front: bool) {
        let at = self
            .queue
            .iter()
            .position(|q| {
                q.priority < request.priority || (front && q.priority == request.priority)
            })
            .unwrap_or(self.queue.len());
        self.queue.insert(at, request);
    }

    /// With [`Preempt::Empty`]: asks a busy member with lower-priority work to give it
    /// back for the head of the queue.
    fn try_preempt(&mut self, cx: &mut Ctx) {
        if self.preempt == Preempt::Never || self.asking.is_some() {
            return;
        }
        let Some(head) = self.queue.front().copied() else {
            return;
        };
        let mut best: Option<(usize, i32)> = None;
        for m in 0..self.team.len() {
            if self.free[m] || self.refused[m] || self.assigned[m].is_empty() {
                continue;
            }
            let top = self.assigned[m].iter().map(|r| r.priority).max();
            let Some(top) = top.filter(|p| *p < head.priority) else {
                continue;
            };
            if cx.away_on_finish(self.team[m]) || !cx.qualified(self.team[m], &head) {
                continue;
            }
            if best.is_none_or(|(_, bp)| top < bp) {
                best = Some((m, top));
            }
        }
        if let Some((m, _)) = best {
            self.asking = Some(m);
            let tag = if self.preempt == Preempt::Full {
                MSG_PREEMPT_FULL
            } else {
                MSG_PREEMPT
            };
            cx.send(self.team[m], tag, head.priority as i64 as u64);
        }
    }

    /// Picks the first free member among `open` with the smallest `key` (ties: team order).
    fn smallest(open: &[bool], key: impl Fn(usize) -> f64) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for m in (0..open.len()).filter(|&m| open[m]) {
            let k = key(m);
            if best.is_none_or(|(_, bk)| k < bk) {
                best = Some((m, k));
            }
        }
        best.map(|(m, _)| m)
    }

    /// Index in the queue of the task member `member` takes now (see [`TaskPick`]).
    fn pick_for(&self, cx: &Ctx, member: usize) -> Option<usize> {
        let me = self.team[member];
        let qualified = |i: usize| cx.qualified(me, &self.queue[i]);
        let first = (0..self.queue.len()).find(|&i| qualified(i))?;
        let top = self.queue[first].priority;
        let at = self.positions[member];
        let mut best: Option<(usize, f64)> = None;
        for i in (first..self.queue.len()).filter(|&i| qualified(i)) {
            let q = &self.queue[i];
            match self.pick {
                TaskPick::Priority => return Some(i),
                TaskPick::Nearest if q.priority < top => break,
                _ => {}
            }
            let d = distance(at, Self::start_of(cx, q));
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Where the work of `request` starts.
    fn start_of(cx: &Ctx, request: &TaskRequest) -> [f64; 3] {
        match request.kind {
            // Where the item is picked up: its own place in a real waiting area, a belt's
            // output end, else the centre.
            TaskKind::Transport { from, item, .. } => cx
                .pickup_point(from, item)
                .unwrap_or_else(|| cx.output_end(from)),
            TaskKind::Operate { station, .. } => cx.meeting_point(station, request.id),
        }
    }

    fn choose(&mut self, cx: &mut Ctx, request: &TaskRequest) -> Option<usize> {
        let n = self.team.len();
        // A member that ends its shift with `Finish` takes no new work until it is back.
        let open: Vec<bool> = (0..n)
            .map(|m| {
                self.free[m]
                    && !cx.away_on_finish(self.team[m])
                    && cx.qualified(self.team[m], request)
            })
            .collect();
        if let Some(m) = self.choose_among(cx, request, &open) {
            return Some(m);
        }
        if !Self::is_case(request) {
            return None;
        }
        // Nobody free: a multitasking member with room for one more case.
        let room: Vec<bool> = (0..n)
            .map(|m| {
                self.has_room(m)
                    && self.joins(m, request)
                    && !cx.away_on_finish(self.team[m])
                    && cx.qualified(self.team[m], request)
            })
            .collect();
        self.choose_among(cx, request, &room)
    }

    /// Applies the rule to the members marked in `open`.
    fn choose_among(
        &mut self,
        cx: &mut Ctx,
        request: &TaskRequest,
        open: &[bool],
    ) -> Option<usize> {
        let n = self.team.len();
        match self.rule {
            DispatchRule::FirstAvailable => open.iter().position(|&f| f),
            DispatchRule::RoundRobin => {
                let member = (0..n).map(|k| (self.next + k) % n).find(|&m| open[m]);
                if let Some(m) = member {
                    self.next = (m + 1) % n;
                }
                member
            }
            DispatchRule::Nearest => {
                let start = Self::start_of(cx, request);
                Self::smallest(open, |m| distance(self.positions[m], start))
            }
            DispatchRule::LeastBusy => Self::smallest(open, |m| self.busy[m]),
            DispatchRule::LongestIdle => Self::smallest(open, |m| self.idle_since[m]),
            DispatchRule::Random => {
                let candidates: Vec<usize> = (0..n).filter(|&m| open[m]).collect();
                match candidates.len() {
                    0 => None,
                    1 => Some(candidates[0]),
                    k => {
                        let u = cx.sample(&Dist::Uniform {
                            min: 0.0,
                            max: k as f64,
                        });
                        Some(candidates[(u.floor() as usize).min(k - 1)])
                    }
                }
            }
        }
    }

    fn update_state(&self, cx: &mut Ctx) {
        cx.set_state(if self.queue.is_empty() {
            State::Idle
        } else {
            State::Blocked
        });
    }
}

impl Behavior for Dispatcher {
    fn kind(&self) -> &'static str {
        "dispatcher"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.free = vec![true; self.team.len()];
        self.busy = vec![0.0; self.team.len()];
        self.assigned_at = vec![0.0; self.team.len()];
        self.idle_since = vec![cx.now(); self.team.len()];
        self.positions = self.team.iter().map(|&m| cx.position(m)).collect();
        self.capacity = vec![1; self.team.len()];
        self.next = 0;
        self.queue.clear();
        self.assigned = vec![Vec::new(); self.team.len()];
        self.asking = None;
        self.refused = vec![false; self.team.len()];
        self.preemptions = 0;
        self.gave_back = false;
        self.names = self
            .team
            .iter()
            .map(|&m| cx.block_name(m).to_string())
            .collect();
        self.since.clear();
        self.requests = 0;
        self.served = vec![0; self.team.len()];
        self.window_from = cx.now();
        self.busy_at_window = vec![0.0; self.team.len()];
        cx.set_state(State::Idle);
    }

    fn accepts_is_item_independent(&self) -> bool {
        true
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_task(&mut self, cx: &mut Ctx, request: TaskRequest) {
        if self.metrics {
            self.requests += 1;
            self.since.insert(request.id.0, cx.now());
        }
        match self.choose(cx, &request) {
            Some(member) => self.assign(cx, member, request),
            None => {
                self.enqueue(request, false);
                self.try_preempt(cx);
            }
        }
        self.update_state(cx);
    }

    fn on_warmup(&mut self, cx: &mut Ctx) {
        self.preemptions = 0;
        self.requests = 0;
        self.served = vec![0; self.team.len()];
        self.window_from = cx.now();
        self.busy_at_window = (0..self.team.len())
            .map(|m| self.busy_until(m, cx.now()))
            .collect();
    }

    /// With preemption: `preemptions.count` (members that gave their work back). With
    /// [`Dispatcher::metrics`]: the team metrics.
    fn custom_metrics(&self, now: Time) -> Vec<(String, f64)> {
        let mut out = Vec::new();
        if self.preempt != Preempt::Never {
            out.push(("preemptions.count".to_string(), self.preemptions as f64));
        }
        if !self.metrics {
            return out;
        }
        out.push(("requests.count".to_string(), self.requests as f64));
        let window = now - self.window_from;
        for (m, name) in self.names.iter().enumerate() {
            out.push((format!("member.{name}.requests"), self.served[m] as f64));
            let busy = self.busy_until(m, now) - self.busy_at_window[m];
            out.push((
                format!("member.{name}.busy"),
                if window > 0.0 {
                    (busy / window).clamp(0.0, 1.0)
                } else {
                    0.0
                },
            ));
        }
        out
    }

    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        let Some(member) = self.team.iter().position(|&m| m == from) else {
            return;
        };
        match tag {
            MSG_POSITION_X => self.positions[member][0] = f64::from_bits(payload),
            MSG_POSITION_Y => self.positions[member][1] = f64::from_bits(payload),
            MSG_POSITION_Z => self.positions[member][2] = f64::from_bits(payload),
            MSG_PREEMPTED => {
                if let Some(i) = self.assigned[member].iter().position(|r| r.id.0 == payload) {
                    let request = self.assigned[member].remove(i);
                    self.enqueue(request, true);
                    if self.asking == Some(member) {
                        self.gave_back = true;
                    }
                }
            }
            MSG_CASE_DONE => {
                if let Some(i) = self.assigned[member].iter().position(|r| r.id.0 == payload) {
                    self.assigned[member].remove(i);
                    self.fill_room(cx, member);
                }
            }
            MSG_PREEMPT_REFUSED => {
                if self.asking == Some(member) {
                    self.asking = None;
                    self.refused[member] = true;
                    self.try_preempt(cx);
                }
            }
            MSG_IDLE => {
                self.capacity[member] = (payload as usize).max(1);
                if !self.free[member] {
                    self.busy[member] += cx.now() - self.assigned_at[member];
                }
                if self.asking == Some(member) {
                    self.asking = None;
                    if std::mem::take(&mut self.gave_back) {
                        self.preemptions += 1;
                    }
                }
                self.assigned[member].clear();
                self.refused[member] = false;
                self.free[member] = true;
                self.idle_since[member] = cx.now();
                let me = self.team[member];
                let first = self.pick_for(cx, member);
                if let Some(head) = first.and_then(|i| self.queue.remove(i)) {
                    self.assign(cx, member, head);
                    if let TaskKind::Transport { from: origin, .. } = head.kind {
                        let mut taken = 1;
                        let mut i = 0;
                        while taken < self.capacity[member] && i < self.queue.len() {
                            let same = matches!(
                                self.queue[i].kind,
                                TaskKind::Transport { from: f, .. } if f == origin
                            ) && cx.qualified(me, &self.queue[i]);
                            if same {
                                let extra = self.queue.remove(i).expect("index in range");
                                self.assigned[member].push(extra);
                                if self.metrics {
                                    self.served[member] += 1;
                                    if let Some(t) = self.since.remove(&extra.id.0) {
                                        cx.record("request_wait", cx.now() - t);
                                    }
                                }
                                cx.forward_task(self.team[member], extra);
                                taken += 1;
                            } else {
                                i += 1;
                            }
                        }
                    }
                    self.fill_room(cx, member);
                }
                self.try_preempt(cx);
            }
            _ => return,
        }
        self.update_state(cx);
    }
}
