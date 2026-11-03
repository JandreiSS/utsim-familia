use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use utsim_core::task::{MSG_IDLE, distance};
use utsim_core::{Behavior, BlockId, Ctx, Dist, Item, State, TaskKind, TaskRequest, Time};

use crate::transporter::{MSG_POSITION_X, MSG_POSITION_Y, MSG_POSITION_Z};

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
/// **Position protocol.** Members send `MSG_POSITION_X/Y/Z` (coordinate as
/// `f64::to_bits`) before `MSG_IDLE`; until the first report the dispatcher uses the
/// member's block position. A member that is broken or off shift still counts as
/// free: its task waits in its own queue until it is back.
pub struct Dispatcher {
    team: Vec<BlockId>,
    rule: DispatchRule,
    pick: TaskPick,
    free: Vec<bool>,
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
}

impl Dispatcher {
    pub fn new(team: Vec<BlockId>, rule: DispatchRule) -> Self {
        let n = team.len();
        Dispatcher {
            team,
            rule,
            pick: TaskPick::Priority,
            free: vec![true; n],
            busy: vec![0.0; n],
            assigned_at: vec![0.0; n],
            idle_since: vec![0.0; n],
            positions: vec![[0.0; 3]; n],
            capacity: vec![1; n],
            next: 0,
            queue: VecDeque::new(),
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

    fn assign(&mut self, cx: &mut Ctx, member: usize, request: TaskRequest) {
        if self.free[member] {
            self.assigned_at[member] = cx.now();
        }
        self.free[member] = false;
        cx.forward_task(self.team[member], request);
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
            TaskKind::Transport { from, .. } => cx.position(from),
            TaskKind::Operate { station, .. } => cx.position(station),
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
                Self::smallest(&open, |m| distance(self.positions[m], start))
            }
            DispatchRule::LeastBusy => Self::smallest(&open, |m| self.busy[m]),
            DispatchRule::LongestIdle => Self::smallest(&open, |m| self.idle_since[m]),
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
        cx.set_state(State::Idle);
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_task(&mut self, cx: &mut Ctx, request: TaskRequest) {
        match self.choose(cx, &request) {
            Some(member) => self.assign(cx, member, request),
            None => {
                let at = self
                    .queue
                    .iter()
                    .position(|q| q.priority < request.priority)
                    .unwrap_or(self.queue.len());
                self.queue.insert(at, request);
            }
        }
        self.update_state(cx);
    }

    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        let Some(member) = self.team.iter().position(|&m| m == from) else {
            return;
        };
        match tag {
            MSG_POSITION_X => self.positions[member][0] = f64::from_bits(payload),
            MSG_POSITION_Y => self.positions[member][1] = f64::from_bits(payload),
            MSG_POSITION_Z => self.positions[member][2] = f64::from_bits(payload),
            MSG_IDLE => {
                self.capacity[member] = (payload as usize).max(1);
                if !self.free[member] {
                    self.busy[member] += cx.now() - self.assigned_at[member];
                }
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
                                cx.forward_task(self.team[member], extra);
                                taken += 1;
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
            _ => return,
        }
        self.update_state(cx);
    }
}
