use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Ctx, Dist, EventId, Item, ItemId, State, Time};

const ARRIVAL: u32 = 0;
const LOST_CHECK: u32 = 1;
const BOUNDARY: u32 = 2;

/// What the source does with an item that cannot leave at the instant it is created.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnBlocked {
    /// Keep the item and stop generating until it leaves.
    #[default]
    Wait,
    /// Destroy the item (counted as `lost`) and keep the arrival clock running.
    Drop,
}

/// Arrivals that change with the time of day (or week): a repeating cycle of periods.
/// Outside every period the source creates nothing (state `off_shift`, "fora do horário").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArrivalSchedule {
    /// Length of the cycle (e.g. 1440 for a day in minutes, 10080 for a week).
    pub cycle: f64,
    /// Periods `[start, end)` inside the cycle, sorted and without overlap.
    pub periods: Vec<ArrivalPeriod>,
}

/// One period of an [`ArrivalSchedule`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArrivalPeriod {
    pub start: f64,
    pub end: f64,
    /// How items arrive during the period (the source's own `interarrival` by default).
    #[serde(default, skip_serializing_if = "PeriodArrivals::is_default")]
    pub arrivals: PeriodArrivals,
    /// Labels that replace the source's labels of the same name during the period
    /// (labels the source does not have are added).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, Dist>,
}

/// Arrival process of one period.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PeriodArrivals {
    /// The source's `interarrival` ("igual ao padrão").
    #[default]
    Default,
    /// Time between arrivals with its own distribution.
    Interarrival(Dist),
    /// Poisson arrivals with this many arrivals per time unit.
    Rate(f64),
    /// No arrivals during the period.
    Closed,
}

impl PeriodArrivals {
    pub fn is_default(&self) -> bool {
        *self == PeriodArrivals::Default
    }
}

impl ArrivalSchedule {
    /// Checks the cycle and the periods. Messages are user-facing (Portuguese); periods
    /// are numbered from 1, as in the editor and in experiment keys.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if !(self.cycle.is_finite() && self.cycle > 0.0) {
            errors.push(format!(
                "schedule.cycle deve ser finito e > 0 (está {})",
                self.cycle
            ));
            return errors;
        }
        if self.periods.is_empty() {
            errors.push(
                "schedule precisa de ao menos um período (sem períodos a fonte nunca cria itens)"
                    .into(),
            );
        }
        let mut end_prev = 0.0;
        for (k, p) in self.periods.iter().enumerate() {
            let n = k + 1;
            if !(p.start.is_finite() && p.end.is_finite()) {
                errors.push(format!(
                    "schedule[{n}]: início e fim devem ser números finitos"
                ));
                continue;
            }
            if p.start < 0.0 || p.end > self.cycle {
                errors.push(format!(
                    "schedule[{n}]: o período [{}, {}) precisa estar dentro do ciclo [0, {}]",
                    p.start, p.end, self.cycle
                ));
            }
            if p.end <= p.start {
                errors.push(format!(
                    "schedule[{n}]: o fim ({}) precisa ser maior que o início ({})",
                    p.end, p.start
                ));
            }
            if k > 0 && p.start < end_prev {
                errors.push(format!(
                    "schedule[{n}]: começa em {} antes do fim do período anterior ({end_prev}); \
                     os períodos precisam estar em ordem e sem sobreposição",
                    p.start
                ));
            }
            end_prev = p.end;
            match &p.arrivals {
                PeriodArrivals::Rate(r) if !(r.is_finite() && *r > 0.0) => errors.push(format!(
                    "schedule[{n}]: a taxa deve ser finita e > 0 (está {r}); \
                     para um período sem chegadas use Closed"
                )),
                PeriodArrivals::Interarrival(d) => {
                    if let Err(e) = d.validate() {
                        errors.push(format!("schedule[{n}].interarrival: {e}"));
                    }
                }
                _ => {}
            }
            for (name, d) in &p.labels {
                if let Err(e) = d.validate() {
                    errors.push(format!("schedule[{n}].labels.{name}: {e}"));
                }
            }
        }
        errors
    }

    /// Expected number of arrivals in period `k` (0-based), given the source's default
    /// interarrival time: `rate * duration` for rates (exact), `duration / mean` for
    /// distributions (exact for `Exp`, the long-run value for the others).
    pub fn expected_in_period(&self, k: usize, default: &Dist) -> f64 {
        let p = &self.periods[k];
        let len = (p.end - p.start).max(0.0);
        match &p.arrivals {
            PeriodArrivals::Default => len / default.mean(),
            PeriodArrivals::Interarrival(d) => len / d.mean(),
            PeriodArrivals::Rate(r) => len * r,
            PeriodArrivals::Closed => 0.0,
        }
    }

    /// Expected arrivals per cycle (sum over the periods).
    pub fn expected_per_cycle(&self, default: &Dist) -> f64 {
        (0..self.periods.len())
            .map(|k| self.expected_in_period(k, default))
            .sum()
    }

    /// Offsets in the cycle where the active period may change, sorted, each with the
    /// period that starts there (an end equal to the cycle wraps to 0).
    fn marks(&self) -> Vec<(f64, Option<usize>)> {
        let mut offsets: Vec<f64> = Vec::new();
        for p in &self.periods {
            offsets.push(p.start);
            offsets.push(if p.end >= self.cycle { 0.0 } else { p.end });
        }
        offsets.sort_by(f64::total_cmp);
        offsets.dedup();
        offsets
            .into_iter()
            .map(|m| (m, self.periods.iter().position(|p| p.start == m)))
            .collect()
    }
}

/// The arrival process in force at a moment, used to compare two periods.
#[derive(PartialEq)]
enum Process<'a> {
    Dist(&'a Dist),
    Rate(f64),
    Closed,
}

/// Runtime state of a schedule: the boundary marks and where the clock is.
struct Clock {
    schedule: ArrivalSchedule,
    marks: Vec<(f64, Option<usize>)>,
    /// Cycle number and mark index of the next boundary event.
    next: (u64, usize),
}

impl Clock {
    fn time_of(&self, (n, i): (u64, usize)) -> Time {
        n as f64 * self.schedule.cycle + self.marks[i].0
    }

    fn after(&self, (n, i): (u64, usize)) -> (u64, usize) {
        if i + 1 < self.marks.len() {
            (n, i + 1)
        } else {
            (n + 1, 0)
        }
    }
}

/// Creates items. With `OnBlocked::Wait` the next inter-arrival time starts when
/// the previous item leaves, so a blocked source stops generating (put a queue
/// after it to get true arrival-to-arrival timing). With `OnBlocked::Drop` an item
/// that cannot leave at once is destroyed and recorded as `lost`, so arrivals form
/// a renewal process. A breakdown pauses the arrival clock.
///
/// **Arrivals by period** ([`ArrivalSchedule`]). The source follows a repeating cycle of
/// periods. At every period boundary the pending arrival is discarded and a new one is
/// drawn from the boundary with the process of the new period, so nothing carries over
/// (a `Rate` period is then an exact non-homogeneous Poisson process with a piecewise
/// constant rate, by memorylessness). Two contiguous periods with the same process keep
/// the pending arrival. Gaps between periods and `Closed` periods create nothing; the
/// state is then `off_shift`. An item still waiting to leave at a boundary is not
/// affected: the next arrival is drawn when it leaves, with the process in force then.
/// Labels are drawn with the period in force when the item is created.
pub struct Source {
    on_blocked: OnBlocked,
    interarrival: Dist,
    first_at_zero: bool,
    max_arrivals: Option<u64>,
    labels: Vec<(String, Dist)>,
    created: u64,
    /// Items that left the source and items destroyed because they could not leave,
    /// both since the warm-up ended (the loss fraction is `lost / (lost + entered)`).
    entered: u64,
    lost: u64,
    /// Pending arrival and its due time.
    next: Option<(EventId, Time)>,
    /// Time left on the arrival clock while down.
    paused: Option<Time>,
    clock: Option<Clock>,
    /// Active period of the schedule (`None`: outside every period).
    current: Option<usize>,
}

impl Source {
    pub fn new(interarrival: Dist) -> Self {
        Source {
            on_blocked: OnBlocked::Wait,
            interarrival,
            first_at_zero: false,
            max_arrivals: None,
            labels: Vec::new(),
            created: 0,
            entered: 0,
            lost: 0,
            next: None,
            paused: None,
            clock: None,
            current: None,
        }
    }

    pub fn on_blocked(mut self, mode: OnBlocked) -> Self {
        self.on_blocked = mode;
        self
    }

    /// Creates the first item at time 0 instead of after one inter-arrival time
    /// (with a schedule, only when a period with arrivals is in force at 0).
    pub fn first_at_zero(mut self, yes: bool) -> Self {
        self.first_at_zero = yes;
        self
    }

    pub fn max_arrivals(mut self, max: Option<u64>) -> Self {
        self.max_arrivals = max;
        self
    }

    /// Assigns a label to every created item, sampled from `dist`.
    pub fn label(mut self, name: impl Into<String>, dist: Dist) -> Self {
        self.labels.push((name.into(), dist));
        self
    }

    /// Arrivals by period (see [`ArrivalSchedule`]). Validate the schedule first.
    pub fn schedule(mut self, schedule: Option<ArrivalSchedule>) -> Self {
        self.clock = schedule.map(|schedule| Clock {
            marks: schedule.marks(),
            schedule,
            next: (0, 0),
        });
        self
    }

    /// Process in force in period `k` (`None`: outside every period).
    fn process(&self, k: Option<usize>) -> Process<'_> {
        let Some(clock) = &self.clock else {
            return Process::Dist(&self.interarrival);
        };
        match k.map(|k| &clock.schedule.periods[k].arrivals) {
            None | Some(PeriodArrivals::Closed) => Process::Closed,
            Some(PeriodArrivals::Default) => Process::Dist(&self.interarrival),
            Some(PeriodArrivals::Interarrival(d)) => Process::Dist(d),
            Some(PeriodArrivals::Rate(r)) => Process::Rate(*r),
        }
    }

    /// Time to the next arrival under the current process (`None`: no arrivals now).
    fn gap(&self, cx: &mut Ctx) -> Option<Time> {
        match self.process(self.current) {
            Process::Dist(d) => Some(cx.sample_duration(d)),
            Process::Rate(r) => Some(cx.sample_duration(&Dist::Exp { mean: 1.0 / r })),
            Process::Closed => None,
        }
    }

    /// Schedules the next period boundary.
    fn schedule_boundary(&mut self, cx: &mut Ctx) {
        let Some(clock) = &self.clock else { return };
        if clock.marks.is_empty() {
            return;
        }
        let at = clock.time_of(clock.next);
        cx.schedule(at - cx.now(), BOUNDARY, None);
    }

    /// A period boundary: switch the process (see the type docs).
    fn on_boundary(&mut self, cx: &mut Ctx) {
        let Some(clock) = &mut self.clock else { return };
        let new = clock.marks[clock.next.1].1;
        clock.next = clock.after(clock.next);
        let old = self.current;
        let keep = old.is_some()
            && new.is_some()
            && self.process(old) == self.process(new)
            && self.process(new) != Process::Closed;
        self.current = new;
        self.schedule_boundary(cx);
        if keep {
            return;
        }
        if let Some((event, _)) = self.next.take() {
            cx.cancel(event);
        }
        if cx.is_down() {
            // Stopped: park the new clock until resume.
            self.paused = self.gap(cx);
            if self.paused.is_none() {
                cx.set_state(State::OffShift);
            }
            return;
        }
        self.paused = None;
        if cx.content_len() == 0 {
            self.schedule_next(cx);
        }
    }

    fn arrive_in(&mut self, cx: &mut Ctx, delay: Time) {
        let event = cx.schedule(delay, ARRIVAL, None);
        self.next = Some((event, cx.now() + delay));
        cx.set_state(State::Generating);
    }

    fn schedule_next(&mut self, cx: &mut Ctx) {
        if self.max_arrivals.is_some_and(|max| self.created >= max) {
            cx.set_state(State::Idle);
            return;
        }
        match self.gap(cx) {
            Some(dt) => self.arrive_in(cx, dt),
            None => cx.set_state(State::OffShift),
        }
    }
}

impl Behavior for Source {
    fn kind(&self) -> &'static str {
        "source"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        self.created = 0;
        self.entered = 0;
        self.lost = 0;
        self.next = None;
        self.paused = None;
        if let Some(clock) = &mut self.clock {
            // The period in force at t = 0, then the first mark after 0.
            self.current = clock.schedule.periods.iter().position(|p| p.start <= 0.0);
            clock.next = match clock.marks.iter().position(|(m, _)| *m > 0.0) {
                Some(i) => (0, i),
                None => (1, 0),
            };
        }
        self.schedule_boundary(cx);
        let open = self.process(self.current) != Process::Closed;
        if self.first_at_zero && self.max_arrivals != Some(0) && open {
            self.arrive_in(cx, 0.0);
        } else {
            self.schedule_next(cx);
        }
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        if tag == LOST_CHECK {
            // The kernel has resolved transfers by now: if the item is still here, it is lost.
            if let Some(id) = item.filter(|id| cx.contents().contains(id)) {
                cx.destroy(id);
                cx.record("lost", 1.0);
                self.lost += 1;
                if cx.is_down() {
                    // Stopped in this same instant: park the clock until resume.
                    self.paused = self.gap(cx);
                    if self.paused.is_none() {
                        cx.set_state(State::OffShift);
                    }
                } else {
                    self.schedule_next(cx);
                }
            }
            return;
        }
        if tag == BOUNDARY {
            self.on_boundary(cx);
            return;
        }
        if tag != ARRIVAL {
            return;
        }
        self.next = None;
        let id = cx.spawn_item();
        let overrides = match (&self.clock, self.current) {
            (Some(clock), Some(k)) => Some(&clock.schedule.periods[k].labels),
            _ => None,
        }
        .filter(|m| !m.is_empty());
        for (name, dist) in &self.labels {
            let dist = overrides.and_then(|m| m.get(name)).unwrap_or(dist);
            let value = cx.sample(dist);
            cx.item_mut(id).set_label(name.clone(), value);
        }
        for (name, dist) in overrides.into_iter().flatten() {
            if !self.labels.iter().any(|(n, _)| n == name) {
                let value = cx.sample(dist);
                cx.item_mut(id).set_label(name.clone(), value);
            }
        }
        self.created += 1;
        cx.set_state(State::Blocked);
        cx.release(id);
        if self.on_blocked == OnBlocked::Drop {
            cx.schedule(0.0, LOST_CHECK, Some(id));
        }
    }

    fn on_exit(&mut self, cx: &mut Ctx, _item: ItemId) {
        self.entered += 1;
        self.schedule_next(cx);
    }

    fn on_warmup(&mut self, _cx: &mut Ctx) {
        self.entered = 0;
        self.lost = 0;
    }

    /// For `Drop` sources: `lost.count`, `entered.count` and `loss_fraction`, always
    /// present (zero when nothing was lost) so they work as optimisation objectives.
    fn custom_metrics(&self, _now: Time) -> Vec<(String, f64)> {
        if self.on_blocked != OnBlocked::Drop {
            return Vec::new();
        }
        let total = (self.lost + self.entered) as f64;
        vec![
            ("lost.count".to_string(), self.lost as f64),
            ("entered.count".to_string(), self.entered as f64),
            (
                "loss_fraction".to_string(),
                if total > 0.0 {
                    self.lost as f64 / total
                } else {
                    0.0
                },
            ),
        ]
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        if let Some((event, due)) = self.next.take() {
            cx.cancel(event);
            self.paused = Some((due - cx.now()).max(0.0));
        }
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        if let Some(remaining) = self.paused.take() {
            self.arrive_in(cx, remaining);
        }
    }
}
