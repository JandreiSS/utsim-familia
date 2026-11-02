use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use utsim_core::task::{MSG_IDLE, distance};
use utsim_core::{Behavior, BlockId, Ctx, Item, State, TaskKind, TaskRequest};

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
///
/// When a member reports `MSG_IDLE` and tasks are waiting, it receives the head of
/// the queue whatever the rule (it is the only free member). If it can carry more
/// than one item (the idle payload is its capacity) it also receives, in queue
/// order, further waiting `Transport` tasks that start at the same block, up to its
/// capacity, so it can serve them in one trip.
///
/// **Position protocol.** Members send `MSG_POSITION_X/Y/Z` (coordinate as
/// `f64::to_bits`) before `MSG_IDLE`; until the first report the dispatcher uses the
/// member's block position. A member that is broken or off shift still counts as
/// free: its task waits in its own queue until it is back.
pub struct Dispatcher {
    team: Vec<BlockId>,
    rule: DispatchRule,
    free: Vec<bool>,
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
            free: vec![true; n],
            positions: vec![[0.0; 3]; n],
            capacity: vec![1; n],
            next: 0,
            queue: VecDeque::new(),
        }
    }

    pub fn rule(&self) -> DispatchRule {
        self.rule
    }

    fn assign(&mut self, cx: &mut Ctx, member: usize, request: TaskRequest) {
        self.free[member] = false;
        cx.forward_task(self.team[member], request);
    }

    /// Where the work of `request` starts.
    fn start_of(cx: &Ctx, request: &TaskRequest) -> [f64; 3] {
        match request.kind {
            TaskKind::Transport { from, .. } => cx.position(from),
            TaskKind::Operate { station, .. } => cx.position(station),
        }
    }

    fn choose(&mut self, cx: &Ctx, request: &TaskRequest) -> Option<usize> {
        let n = self.team.len();
        // A member that ends its shift with `Finish` takes no new work until it is back.
        let open: Vec<bool> = (0..n)
            .map(|m| self.free[m] && !cx.away_on_finish(self.team[m]))
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
                let mut best: Option<(usize, f64)> = None;
                for m in (0..n).filter(|&m| open[m]) {
                    let d = distance(self.positions[m], start);
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((m, d));
                    }
                }
                best.map(|(m, _)| m)
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
                match self.queue.pop_front() {
                    Some(head) => {
                        self.assign(cx, member, head);
                        if let TaskKind::Transport { from: origin, .. } = head.kind {
                            let mut taken = 1;
                            let mut i = 0;
                            while taken < self.capacity[member] && i < self.queue.len() {
                                let same = matches!(
                                    self.queue[i].kind,
                                    TaskKind::Transport { from: f, .. } if f == origin
                                );
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
                    None => self.free[member] = true,
                }
            }
            _ => return,
        }
        self.update_state(cx);
    }
}
