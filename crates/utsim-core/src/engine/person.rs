//! Kernel side of [`crate::person`]: fatigue level updated at state changes, pace factor
//! of operation times and walking speed, learning by repetition, pauses, skills of
//! requests, saved states and metrics. Only blocks with a [`Person`] profile come here;
//! every other block takes none of these paths and keeps its results.

use std::collections::BTreeMap;

use super::{Core, Simulation};
use crate::Time;
use crate::block::{BlockId, State};
use crate::engine::Ctx;
use crate::init::BlockState;
use crate::item::{ItemId, Value};
use crate::person::{Fatigue, Person, fatigue_rest, fatigue_work, rest_duration};
use crate::rng::SimRng;
use crate::task::{RequestId, TaskKind, TaskRequest};

/// How the fatigue level moves in the current state.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rate {
    Work(f64),
    Rest(f64),
}

/// Run-time state of one person.
pub(super) struct PersonState {
    cfg: Person,
    rng: SimRng,
    /// Individual pace of this replication (1 without `variation`).
    pace: f64,
    /// Fatigue level at `since`, and how it moves from there.
    level: f64,
    since: Time,
    rate: Rate,
    /// Weight carried (kg), set when a travel starts.
    load: f64,
    /// Repetitions done of each task (learning curve).
    experience: BTreeMap<String, u64>,
    // Statistics of the window (cleared at the warm-up).
    from: Time,
    area: f64,
    max: f64,
    above: f64,
    reps: u64,
    learn_sum: f64,
    rests: u64,
    /// Operations drawn from the person's own times.
    own: u64,
}

impl PersonState {
    pub(super) fn new(cfg: Person, rng: SimRng) -> Self {
        PersonState {
            cfg,
            rng,
            pace: 1.0,
            level: 0.0,
            since: 0.0,
            rate: Rate::Rest(0.0),
            load: 0.0,
            experience: BTreeMap::new(),
            from: 0.0,
            area: 0.0,
            max: 0.0,
            above: 0.0,
            reps: 0,
            learn_sum: 0.0,
            rests: 0,
            own: 0,
        }
    }

    fn dynamic(&self) -> bool {
        matches!(self.cfg.fatigue, Some(Fatigue::Dynamic { .. }))
    }

    fn limit(&self) -> Option<f64> {
        match self.cfg.fatigue {
            Some(Fatigue::Dynamic { limit, .. }) => limit,
            _ => None,
        }
    }

    /// Rate of the fatigue level in `state`.
    fn rate_for(&self, state: Option<State>) -> Rate {
        let Some(Fatigue::Dynamic {
            work,
            walk,
            carry,
            recovery,
            ..
        }) = self.cfg.fatigue
        else {
            return Rate::Rest(0.0);
        };
        match state {
            Some(
                State::Utilize
                | State::Loading
                | State::Unloading
                | State::Setup
                | State::Processing,
            ) => Rate::Work(work),
            Some(State::TravelEmpty) => Rate::Work(walk),
            Some(State::TravelLoaded) => Rate::Work(walk + carry * self.load),
            _ => Rate::Rest(recovery),
        }
    }

    /// Level after `dt` under the current rate, and its integral.
    fn step(&self, dt: f64) -> (f64, f64) {
        match self.rate {
            Rate::Work(l) => fatigue_work(self.level, l, dt),
            Rate::Rest(m) => fatigue_rest(self.level, m, dt),
        }
    }

    fn level_at(&self, now: Time) -> f64 {
        if !self.dynamic() {
            return 0.0;
        }
        self.step(now - self.since).0
    }

    /// Time above the limit during the next `dt` (the level is monotone in it).
    fn above_in(&self, dt: f64, end: f64) -> f64 {
        let Some(l) = self.limit() else { return 0.0 };
        let f0 = self.level;
        match self.rate {
            Rate::Work(lambda) => {
                if end <= l {
                    0.0
                } else if f0 >= l {
                    dt
                } else {
                    (dt - ((1.0 - f0) / (1.0 - l)).ln() / lambda).max(0.0)
                }
            }
            Rate::Rest(mu) => {
                if f0 <= l {
                    0.0
                } else if end >= l {
                    dt
                } else {
                    ((f0 / l).ln() / mu).min(dt)
                }
            }
        }
    }

    /// Brings the level and the statistics up to `now`.
    fn advance(&mut self, now: Time) {
        if !self.dynamic() {
            self.since = now;
            return;
        }
        let dt = now - self.since;
        if dt > 0.0 {
            let (end, area) = self.step(dt);
            self.above += self.above_in(dt, end);
            self.area += area;
            self.level = end;
            self.max = self.max.max(end);
        }
        self.since = now;
    }

    pub(super) fn reset_stats(&mut self, now: Time) {
        self.advance(now);
        self.from = now;
        self.area = 0.0;
        self.max = self.level;
        self.above = 0.0;
        self.reps = 0;
        self.learn_sum = 0.0;
        self.rests = 0;
        self.own = 0;
    }

    /// Operation-time multiplier from fatigue at `now` (pace and learning apart).
    fn fatigue_factor(&self, now: Time) -> f64 {
        match self.cfg.fatigue {
            None => 1.0,
            Some(Fatigue::Allowance(p)) => 1.0 + p,
            Some(Fatigue::Dynamic { time_factor, .. }) => 1.0 + time_factor * self.level_at(now),
        }
    }

    /// Rested time of `task` drawn from this person's own times (with its own stream),
    /// divided by `gain`; `None` when the person has no own time for the task.
    fn own_time(&mut self, task: &str, gain: f64) -> Option<Time> {
        let t = self.cfg.times.get(task)?;
        let x = t.dist.sample(&mut self.rng).max(0.0);
        self.own += 1;
        Some(x / gain)
    }

    /// Counts one more repetition of `task` and returns its learning multiplier.
    fn learn(&mut self, task: &str) -> f64 {
        let Some(curve) = self.cfg.learning else {
            return 1.0;
        };
        let n = self.experience.entry(task.to_string()).or_insert(0);
        *n += 1;
        let factor = curve.factor(*n);
        self.reps += 1;
        self.learn_sum += factor;
        factor
    }

    fn metrics(&self, now: Time, rest_time: f64, break_time: f64) -> Vec<(String, f64)> {
        let mut out = vec![("person.pace".to_string(), self.pace)];
        if self.dynamic() {
            let dt = (now - self.since).max(0.0);
            let (end, area) = self.step(dt);
            let window = now - self.from;
            let mean = if window > 0.0 {
                (self.area + area) / window
            } else {
                end
            };
            out.push(("person.fatigue_mean".into(), mean));
            out.push(("person.fatigue_max".into(), self.max.max(end)));
            out.push(("person.fatigue_end".into(), end));
            if self.limit().is_some() {
                out.push((
                    "person.time_above_limit".into(),
                    self.above + self.above_in(dt, end),
                ));
                out.push(("person.rests".into(), self.rests as f64));
            }
        }
        out.push(("person.rest_time".into(), rest_time));
        out.push(("person.break_time".into(), break_time));
        if let Some(curve) = self.cfg.learning {
            let factor = if self.reps == 0 {
                curve.factor(1)
            } else {
                self.learn_sum / self.reps as f64
            };
            out.push(("person.repetitions".into(), self.reps as f64));
            out.push(("person.learning_factor".into(), factor));
            out.push(("person.learning_gain".into(), 1.0 - factor / curve.first));
        }
        if !self.cfg.times.is_empty() {
            out.push(("person.own_times".into(), self.own as f64));
        }
        out
    }
}

impl Core {
    /// The state of person `b` changed: the level runs under the new rate from now on.
    pub(super) fn person_refresh(&mut self, b: BlockId) {
        let now = self.now;
        let rt = &mut self.blocks[b.0];
        let state = rt.stats.states.current();
        let Some(p) = rt.person.as_deref_mut() else {
            return;
        };
        p.advance(now);
        p.rate = p.rate_for(state);
    }

    /// The statistics window starts now (warm-up).
    pub(super) fn person_warmup(&mut self) {
        let now = self.now;
        for rt in &mut self.blocks {
            if let Some(p) = rt.person.as_deref_mut() {
                p.reset_stats(now);
            }
        }
    }
}

impl Ctx<'_> {
    /// Walking speed of this block from its empty speed `base`, carrying `load_kg`: the
    /// loaded walk, fatigue and pace of its [`Person`] profile. Call it right before the
    /// travel state is set (the load also sets the fatigue rate of a loaded walk). Without
    /// a profile it is `base`.
    pub fn walk_speed(&mut self, base: f64, load_kg: f64) -> f64 {
        let now = self.core.now;
        let Some(p) = self.core.blocks[self.me.0].person.as_deref_mut() else {
            return base;
        };
        p.load = load_kg.max(0.0);
        let mut speed = base;
        if let Some(w) = &p.cfg.walk_speed_loaded {
            if load_kg > 0.0 {
                speed = w.speed(speed, load_kg);
            }
        }
        if let Some(Fatigue::Dynamic { speed_factor, .. }) = p.cfg.fatigue {
            speed *= 1.0 - speed_factor * p.level_at(now);
        }
        speed * p.pace
    }

    /// Duration of an operation of standard time `t` done by `persons` (the operators of
    /// a station, or the carrier itself) as repetition of `task`: the slowest person sets
    /// the pace (fatigue or allowance, learning, individual pace); each person with a
    /// learning curve counts one repetition. A person with an own time for `task` draws
    /// its rested time from it in place of `t` (and its individual pace is left out).
    /// People without a profile take `t`.
    pub fn person_time(&mut self, persons: &[BlockId], task: &str, t: Time) -> Time {
        self.pace_of(persons, task, t, Some(1.0))
    }

    /// [`Ctx::person_time`] for a crew whose work goes `gain` times faster than one
    /// person's: own times are divided by `gain` (`t` already is).
    pub fn person_time_crew(
        &mut self,
        persons: &[BlockId],
        task: &str,
        t: Time,
        gain: f64,
    ) -> Time {
        self.pace_of(persons, task, t, Some(gain))
    }

    /// [`Ctx::person_time`] for the rest of an operation that was already under way (an
    /// initial state): `t` is the remaining standard time and own times are not drawn.
    pub fn person_time_resumed(&mut self, persons: &[BlockId], task: &str, t: Time) -> Time {
        self.pace_of(persons, task, t, None)
    }

    fn pace_of(&mut self, persons: &[BlockId], task: &str, t: Time, own: Option<f64>) -> Time {
        let now = self.core.now;
        let mut out: Option<Time> = None;
        let mut slowest = |x: Time| out = Some(out.map_or(x, |y: Time| y.max(x)));
        for &b in persons {
            let Some(p) = self.core.blocks[b.0].person.as_deref_mut() else {
                slowest(t);
                continue;
            };
            // Same arithmetic as before own times existed, so standard times stay exact.
            let (base, f) = match own.and_then(|gain| p.own_time(task, gain)) {
                Some(x) => (x, p.fatigue_factor(now) * p.learn(task)),
                None => (t, p.fatigue_factor(now) / p.pace * p.learn(task)),
            };
            slowest(if f != 1.0 { base * f } else { base });
        }
        out.unwrap_or(t)
    }

    /// Whether this person must pause now (its fatigue reached the limit of its profile):
    /// the pause that brings it back to the target level. Counts the pause.
    pub fn take_rest(&mut self) -> Option<Time> {
        let now = self.core.now;
        let p = self.core.blocks[self.me.0].person.as_deref_mut()?;
        let Some(Fatigue::Dynamic {
            recovery,
            limit: Some(limit),
            target,
            ..
        }) = p.cfg.fatigue
        else {
            return None;
        };
        let f = p.level_at(now);
        if f < limit {
            return None;
        }
        p.rests += 1;
        Some(rest_duration(f, target, recovery))
    }

    /// Weight (kg) of `item` for this person: its weight label (0 when absent or not a
    /// number, or without a profile).
    pub fn person_weight(&self, item: ItemId) -> f64 {
        let Some(p) = self.core.blocks[self.me.0].person.as_deref() else {
            return 0.0;
        };
        match self
            .core
            .items
            .get(item)
            .and_then(|it| it.label(&p.cfg.weight_label))
        {
            Some(Value::Num(x)) if x.is_finite() && *x > 0.0 => *x,
            _ => 0.0,
        }
    }

    /// Whether this block has a person profile.
    pub fn is_person(&self) -> bool {
        self.core.blocks[self.me.0].person.is_some()
    }

    /// Like [`Ctx::request_task`], for work only some people may do (`need`): dispatchers
    /// give it only to qualified members ([`Ctx::qualified`]). An empty `need` = anyone.
    pub fn request_task_requiring(
        &mut self,
        to: BlockId,
        kind: TaskKind,
        priority: i32,
        need: &crate::task::Requirement,
    ) -> RequestId {
        let id = self.request_task(to, kind, priority);
        if !need.is_empty() {
            self.core.required.insert(id.0, need.clone());
        }
        id
    }

    /// Whether `member` may take `request`: the request needs nothing, or the member's
    /// person profile lists every skill and the member wears every protective equipment
    /// asked.
    pub fn qualified(&self, member: BlockId, request: &TaskRequest) -> bool {
        let Some(need) = self.core.required.get(&request.id.0) else {
            return true;
        };
        let block = &self.core.blocks[member.0];
        let skills = need.skills.is_empty()
            || block
                .person
                .as_deref()
                .is_some_and(|p| need.skills.iter().all(|s| p.cfg.has_skill(s)));
        skills && need.ppe.iter().all(|e| block.ppe.contains(e))
    }
}

impl Simulation {
    /// Protective equipment the resource `block` wears (checked by [`Ctx::qualified`]).
    pub fn set_ppe(&mut self, block: BlockId, ppe: Vec<String>) {
        self.core.blocks[block.0].ppe = ppe;
    }

    /// Gives block `block` (a mobile resource used as a person) a [`Person`] profile.
    pub fn set_person(&mut self, block: BlockId, person: Person) {
        let rng = crate::rng::stream(
            self.config.seed,
            self.config.replication,
            super::PERSON_STREAMS + block.0 as u64,
        );
        self.core.blocks[block.0].person = Some(Box::new(PersonState::new(person, rng)));
    }

    /// Run start: pace of the replication, initial fatigue and experience.
    pub(super) fn person_start(&mut self, b: BlockId, state: Option<&BlockState>) {
        let now = self.core.now;
        let rt = &mut self.core.blocks[b.0];
        let current = rt.stats.states.current();
        let Some(p) = rt.person.as_deref_mut() else {
            return;
        };
        if let Some(v) = p.cfg.variation.clone() {
            p.pace = v.draw(&mut p.rng);
        }
        p.level = state.and_then(|s| s.fatigue).unwrap_or(0.0);
        p.experience = state.map(|s| s.experience.clone()).unwrap_or_default();
        p.since = now;
        p.rate = p.rate_for(current);
        p.from = now;
        p.area = 0.0;
        p.max = p.level;
        p.above = 0.0;
    }

    /// The person part of a saved state; returns a note about what is lost.
    pub(super) fn person_save(&self, b: BlockId, state: &mut BlockState) -> Option<String> {
        let p = self.core.blocks[b.0].person.as_deref()?;
        if p.dynamic() {
            state.fatigue = Some(p.level_at(self.core.now));
        }
        state.experience = p.experience.clone();
        p.cfg
            .variation
            .as_ref()
            .map(|_| "o ritmo individual é sorteado de novo na próxima rodada".to_string())
    }

    /// Checks of the person part of an initial state.
    pub(super) fn person_check_state(&self, b: BlockId, state: &BlockState) -> Option<String> {
        let p = self.core.blocks[b.0].person.as_deref();
        let uses = state.fatigue.is_some() || !state.experience.is_empty();
        match (p, state.fatigue) {
            (None, _) if uses => {
                Some("'fatigue' e 'experience' exigem o perfil de pessoa (person) no objeto".into())
            }
            (Some(p), Some(_)) if !p.dynamic() => {
                Some("'fatigue' exige fadiga dinâmica (fatigue: Dynamic) no perfil".into())
            }
            (_, Some(f)) if !(0.0..1.0).contains(&f) => {
                Some(format!("'fatigue' deve ficar em [0, 1) (está {f})"))
            }
            _ => None,
        }
    }

    /// Person metrics of block `b`, if it has a profile.
    pub(super) fn person_metrics(&self, b: BlockId, now: Time) -> Vec<(String, f64)> {
        let rt = &self.core.blocks[b.0];
        let Some(p) = rt.person.as_deref() else {
            return Vec::new();
        };
        let window = (now - p.from).max(0.0);
        let fractions = rt.stats.states.fractions(now);
        let time_in = |s: State| fractions.get(&s).copied().unwrap_or(0.0) * window;
        p.metrics(now, time_in(State::Resting), time_in(State::OffShift))
    }
}
