use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use utsim_core::task::MSG_IDLE;
use utsim_core::{Behavior, BlockId, Ctx, Item, State, TaskRequest};

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
/// member is busy, highest priority first. Reference implementation: the rule is
/// stored, `FirstAvailable` is applied.
pub struct Dispatcher {
    team: Vec<BlockId>,
    rule: DispatchRule,
    free: Vec<bool>,
    queue: VecDeque<TaskRequest>,
}

impl Dispatcher {
    pub fn new(team: Vec<BlockId>, rule: DispatchRule) -> Self {
        let free = vec![true; team.len()];
        Dispatcher {
            team,
            rule,
            free,
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
        self.queue.clear();
        cx.set_state(State::Idle);
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_task(&mut self, cx: &mut Ctx, request: TaskRequest) {
        match self.free.iter().position(|&f| f) {
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

    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, _payload: u64) {
        if tag != MSG_IDLE {
            return;
        }
        if let Some(member) = self.team.iter().position(|&m| m == from) {
            match self.queue.pop_front() {
                Some(request) => self.assign(cx, member, request),
                None => self.free[member] = true,
            }
        }
        self.update_state(cx);
    }
}
