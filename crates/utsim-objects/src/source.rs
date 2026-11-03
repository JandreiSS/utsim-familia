use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use utsim_core::init::Accepts;
use utsim_core::{Behavior, BlockState, Ctx, Dist, EventId, Item, ItemId, State, Time};

const ARRIVAL: u32 = 0;
const LOST_CHECK: u32 = 1;
const BOUNDARY: u32 = 2;
const TABLE_ROW: u32 = 3;
const SPAWN_NEXT: u32 = 4;

/// How the items of one arrival (a batch or a table row) leave the source.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Release {
    /// All at the instant of the arrival (the next object decides whether they fit).
    #[default]
    Together,
    /// One by one, this long apart (sampled for each gap).
    OneByOne(Dist),
}

impl Release {
    pub fn is_together(&self) -> bool {
        *self == Release::Together
    }
}

/// Most items one row of an arrival table may create.
pub const MAX_ROW_COUNT: u64 = 1_000_000;

/// How a source walks its [`ArrivalTable`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableMode {
    /// Arrival schedule: each row creates its items at the instant `at` of the cycle.
    #[default]
    Times,
    /// Arrival sequence: the rows in order, one per arrival, with the source's
    /// `interarrival` between two rows.
    Sequence,
}

/// What a source does at the end of its [`ArrivalTable`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum TableRepeat {
    /// Stop after the last row (state `idle`).
    #[default]
    Once,
    /// `Times` only: repeat the rows every this many time units (`at` inside the cycle).
    Cycle(f64),
    /// `Sequence` only: go back to the first row.
    Loop,
}

impl TableRepeat {
    pub fn is_once(&self) -> bool {
        *self == TableRepeat::Once
    }
}

/// An ordered table of arrivals ("horário" or "sequência de chegadas"). See [`Source`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArrivalTable {
    #[serde(default)]
    pub mode: TableMode,
    #[serde(default, skip_serializing_if = "TableRepeat::is_once")]
    pub repeat: TableRepeat,
    pub rows: Vec<ArrivalRow>,
}

/// One row of an [`ArrivalTable`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArrivalRow {
    /// Instant of the row (`Times` only), inside the cycle with `Cycle`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    /// Items the row creates at once (0 is allowed: the row creates nothing).
    #[serde(default = "one_u64")]
    pub count: u64,
    /// Constant labels of the row's items; they replace the source's labels of the same
    /// name (labels the source does not have are added).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, f64>,
}

fn one_u64() -> u64 {
    1
}

impl ArrivalTable {
    /// Checks the rows against the mode and the repetition. Messages are user-facing
    /// (Portuguese); rows are numbered from 1, as in experiment keys.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.rows.is_empty() {
            errors.push("table precisa de ao menos uma linha".into());
        }
        let cycle = match (self.mode, self.repeat) {
            (TableMode::Times, TableRepeat::Loop) => {
                errors.push(
                    "table: Loop só vale no modo Sequence; no modo Times use Cycle(duração)".into(),
                );
                None
            }
            (TableMode::Sequence, TableRepeat::Cycle(_)) => {
                errors.push(
                    "table: Cycle só vale no modo Times; no modo Sequence use Loop ou Once".into(),
                );
                None
            }
            (_, TableRepeat::Cycle(d)) => {
                if !(d.is_finite() && d > 0.0) {
                    errors.push(format!("table: Cycle({d}) precisa de duração finita e > 0"));
                    None
                } else {
                    Some(d)
                }
            }
            _ => None,
        };
        let mut prev = f64::NEG_INFINITY;
        for (k, row) in self.rows.iter().enumerate() {
            let n = k + 1;
            match (self.mode, row.at) {
                (TableMode::Times, None) => {
                    errors.push(format!("table.rows[{n}]: o modo Times precisa de `at`"));
                }
                (TableMode::Times, Some(at)) => {
                    if !(at.is_finite() && at >= 0.0) {
                        errors.push(format!(
                            "table.rows[{n}]: at deve ser finito e >= 0 (está {at})"
                        ));
                    } else {
                        if at < prev {
                            errors.push(format!(
                                "table.rows[{n}]: at ({at}) antes da linha anterior ({prev}); \
                                 as linhas precisam estar em ordem de at"
                            ));
                        }
                        if let Some(d) = cycle.filter(|d| at >= *d) {
                            errors.push(format!(
                                "table.rows[{n}]: at ({at}) precisa estar dentro do ciclo [0, {d})"
                            ));
                        }
                        prev = at;
                    }
                }
                (TableMode::Sequence, Some(_)) => errors.push(format!(
                    "table.rows[{n}]: `at` só vale no modo Times (na sequência o intervalo vem de interarrival)"
                )),
                (TableMode::Sequence, None) => {}
            }
            if row.count > MAX_ROW_COUNT {
                errors.push(format!(
                    "table.rows[{n}]: count deve ser no máximo {MAX_ROW_COUNT}"
                ));
            }
            for (name, v) in &row.labels {
                if !v.is_finite() {
                    errors.push(format!(
                        "table.rows[{n}].labels.{name}: o valor deve ser finito"
                    ));
                }
            }
        }
        errors
    }

    /// Items created per pass over the table.
    pub fn items_per_pass(&self) -> f64 {
        self.rows.iter().map(|r| r.count as f64).sum()
    }
}

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
    /// Point of the cycle at `t = 0` (the calendar start modulo the cycle).
    phase: f64,
}

impl Clock {
    fn time_of(&self, (n, i): (u64, usize)) -> Time {
        n as f64 * self.schedule.cycle + self.marks[i].0 - self.phase
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
    /// Length of the arrival clock now running (for saved states).
    gap: Time,
    clock: Option<Clock>,
    /// Active period of the schedule (`None`: outside every period).
    current: Option<usize>,
    table: Option<ArrivalTable>,
    /// Next row of the table and, for `Times` with `Cycle`, the cycle it belongs to.
    row: usize,
    pass: u64,
    /// Point of the table's cycle at `t = 0` (`Times`).
    table_phase: f64,
    /// The table has no rows left (`Once` after the last row).
    exhausted: bool,
    /// Items of the current arrival still in the source; the next arrival clock starts
    /// when it reaches zero (`Times` rows do not wait).
    outstanding: usize,
    /// Arrivals (rows, or single items without a table), total and since the warm-up.
    arrivals: u64,
    arrivals_window: u64,
    /// Items per arrival without a table (absent: one).
    batch: Option<Dist>,
    release: Release,
    /// Items of the arrivals not created yet (`OneByOne`): count left and table row.
    burst: VecDeque<(u64, Option<usize>)>,
    /// Pending `SPAWN_NEXT` and its due time; time left while stopped.
    spawn_next: Option<(EventId, Time)>,
    spawn_paused: Option<Time>,
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
            gap: 0.0,
            clock: None,
            current: None,
            table: None,
            row: 0,
            pass: 0,
            table_phase: 0.0,
            exhausted: false,
            outstanding: 0,
            arrivals: 0,
            arrivals_window: 0,
            batch: None,
            release: Release::Together,
            burst: VecDeque::new(),
            spawn_next: None,
            spawn_paused: None,
        }
    }

    /// Items created per arrival (rounded, at least 0) when there is no table; absent:
    /// one. Validate the distribution first.
    pub fn batch(mut self, batch: Option<Dist>) -> Self {
        self.batch = batch;
        self
    }

    /// How the items of one arrival leave: together or spaced (see [`Release`]).
    pub fn release(mut self, release: Release) -> Self {
        self.release = release;
        self
    }

    /// Whether the source reports `arrivals.count` (several items per arrival possible).
    fn counts_arrivals(&self) -> bool {
        self.table.is_some() || self.batch.is_some()
    }

    /// Arrival table (see [`ArrivalTable`]); validate it first. Not used with a schedule.
    pub fn table(mut self, table: Option<ArrivalTable>) -> Self {
        self.table = table;
        self
    }

    /// Whether arrivals come at the fixed instants of a `Times` table.
    fn timed(&self) -> bool {
        self.table.as_ref().map(|t| t.mode) == Some(TableMode::Times)
    }

    /// Schedules the next row of a `Times` table (or goes idle at its end).
    fn schedule_row(&mut self, cx: &mut Ctx) {
        let Some(table) = &self.table else { return };
        if self.exhausted || self.max_arrivals.is_some_and(|max| self.arrivals >= max) {
            self.next = None;
            if self.outstanding == 0 {
                cx.set_state(State::Idle);
            }
            return;
        }
        let at = table.rows[self.row].at.unwrap_or(0.0);
        let cycle = match table.repeat {
            TableRepeat::Cycle(d) => d,
            _ => 0.0,
        };
        let due = self.pass as f64 * cycle + at - self.table_phase;
        let delay = (due - cx.now()).max(0.0);
        let event = cx.schedule(delay, TABLE_ROW, None);
        self.next = Some((event, cx.now() + delay));
        if self.outstanding == 0 {
            cx.set_state(State::Generating);
        }
    }

    /// Moves to the next row after one was used.
    fn advance_row(&mut self) {
        let Some(table) = &self.table else { return };
        self.row += 1;
        if self.row < table.rows.len() {
            return;
        }
        self.row = 0;
        match table.repeat {
            TableRepeat::Once => self.exhausted = true,
            TableRepeat::Cycle(_) => self.pass += 1,
            TableRepeat::Loop => {}
        }
    }

    /// One arrival: creates `count` items with the source's labels, then the row's.
    /// With `OneByOne`, the first item now and the others queued (see [`Release`]).
    fn arrive(&mut self, cx: &mut Ctx, count: u64, row: Option<usize>) {
        self.arrivals += 1;
        self.arrivals_window += 1;
        self.outstanding += count as usize;
        if self.release.is_together() || count <= 1 {
            for _ in 0..count {
                self.spawn_one(cx, row);
            }
            return;
        }
        let first = if self.burst.is_empty() { 1 } else { 0 };
        if count > first {
            self.burst.push_back((count - first, row));
        }
        if first == 1 {
            self.spawn_one(cx, row);
            self.schedule_spawn(cx);
        }
    }

    /// Schedules the next item of a `OneByOne` arrival.
    fn schedule_spawn(&mut self, cx: &mut Ctx) {
        let Release::OneByOne(gap) = &self.release else {
            return;
        };
        if self.burst.is_empty() {
            return;
        }
        let gap = cx.sample_duration(gap);
        let event = cx.schedule(gap, SPAWN_NEXT, None);
        self.spawn_next = Some((event, cx.now() + gap));
    }

    /// Creates and releases one item of an arrival.
    fn spawn_one(&mut self, cx: &mut Ctx, row: Option<usize>) {
        let id = cx.spawn_item();
        self.label_item(cx, id);
        if let Some(r) = row {
            let labels = &self.table.as_ref().expect("row of a table").rows[r].labels;
            for (name, v) in labels {
                cx.item_mut(id).set_label(name.clone(), *v);
            }
        }
        self.created += 1;
        cx.set_state(State::Blocked);
        cx.release(id);
        if self.on_blocked == OnBlocked::Drop {
            cx.schedule(0.0, LOST_CHECK, Some(id));
        }
    }

    /// Items of one arrival without a table.
    fn batch_size(&self, cx: &mut Ctx) -> u64 {
        match &self.batch {
            None => 1,
            Some(d) => (cx.sample(d).round().max(0.0) as u64).min(MAX_ROW_COUNT),
        }
    }

    /// Labels drawn when an item is created (with the period in force).
    fn label_item(&self, cx: &mut Ctx, id: ItemId) {
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
    }

    /// The last item of an arrival left (or was lost): start the next arrival clock.
    fn arrival_done(&mut self, cx: &mut Ctx) {
        if self.timed() {
            cx.set_state(if self.next.is_some() {
                State::Generating
            } else {
                State::Idle
            });
            return;
        }
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
            phase: 0.0,
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
    fn gap(&mut self, cx: &mut Ctx) -> Option<Time> {
        let gap = match self.process(self.current) {
            Process::Dist(d) => Some(cx.sample_duration(d)),
            Process::Rate(r) => Some(cx.sample_duration(&Dist::Exp { mean: 1.0 / r })),
            Process::Closed => None,
        };
        if let Some(g) = gap {
            self.gap = g;
        }
        gap
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
        if self.exhausted || self.max_arrivals.is_some_and(|max| self.arrivals >= max) {
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
        self.gap = 0.0;
        self.row = 0;
        self.pass = 0;
        self.exhausted = false;
        self.outstanding = 0;
        self.arrivals = 0;
        self.arrivals_window = 0;
        self.burst.clear();
        self.spawn_next = None;
        self.spawn_paused = None;
        if let Some(table) = self.table.as_ref().filter(|t| t.mode == TableMode::Times) {
            // Where the run starts in the table (the calendar start, 0 unless the run
            // starts from an initial state), then the first row at or after it.
            let start = cx.calendar_start();
            self.table_phase = match table.repeat {
                TableRepeat::Cycle(d) => start.rem_euclid(d),
                _ => start,
            };
            let phase = self.table_phase;
            match table.rows.iter().position(|r| r.at.unwrap_or(0.0) >= phase) {
                Some(i) => self.row = i,
                None if matches!(table.repeat, TableRepeat::Cycle(_)) => self.pass = 1,
                None => self.exhausted = true,
            }
            self.schedule_row(cx);
            return;
        }
        if let Some(clock) = &mut self.clock {
            // The period in force at t = 0, then the first mark after it. The cycle is at
            // the calendar start (0 unless the run starts from an initial state).
            let phase = cx.calendar_start().rem_euclid(clock.schedule.cycle);
            clock.phase = phase;
            self.current = clock
                .schedule
                .periods
                .iter()
                .position(|p| p.start <= phase && phase < p.end);
            clock.next = match clock.marks.iter().position(|(m, _)| *m > phase) {
                Some(i) => (0, i),
                None => (1, 0),
            };
        }
        self.schedule_boundary(cx);
        let open = self.process(self.current) != Process::Closed;
        if self.first_at_zero && self.max_arrivals != Some(0) && open {
            self.gap = 0.0;
            self.arrive_in(cx, 0.0);
        } else {
            self.schedule_next(cx);
        }
    }

    /// `elapsed`: the arrival clock has run this long, so the next arrival draws the
    /// residual of the inter-arrival time. One item (`Wait` only) is an item that could
    /// not leave yet: the clock starts again when it leaves.
    fn restore(&mut self, cx: &mut Ctx, state: &BlockState) -> Result<(), String> {
        state.check(
            "Source",
            Accepts {
                items: true,
                elapsed: true,
                ..Accepts::default()
            },
        )?;
        let n = state.items.len();
        if n > 1 || (n == 1 && self.on_blocked == OnBlocked::Drop) {
            return Err(
                "uma fonte só guarda um item que ainda não saiu, e só com on_blocked: Wait".into(),
            );
        }
        if n == 1 {
            self.outstanding = 1;
            if state.elapsed.is_some() {
                return Err(
                    "'elapsed' não vale com um item parado na fonte (o relógio de chegadas recomeça quando ele sai)"
                        .into(),
                );
            }
            if let Some((event, _)) = self.next.take() {
                cx.cancel(event);
            }
            let id = cx.contents()[0];
            cx.set_state(State::Blocked);
            cx.release(id);
            return Ok(());
        }
        if self.timed() {
            return Ok(()); // the rows keep their instants
        }
        if let (Some(elapsed), Some((event, _))) = (state.elapsed, self.next) {
            let dist = match self.process(self.current) {
                Process::Dist(d) => Some(d.clone()),
                Process::Rate(_) | Process::Closed => None,
            };
            // A rate is a Poisson process: memoryless, the pending arrival stays.
            if let Some(d) = dist {
                cx.cancel(event);
                self.next = None;
                let left = cx.sample_residual(&d, elapsed);
                self.gap = elapsed + left;
                self.arrive_in(cx, left);
            }
        }
        Ok(())
    }

    fn save_state(&self, cx: &Ctx, state: &mut BlockState) -> Option<String> {
        if !state.items.is_empty() {
            return None;
        }
        let left = match (self.next, self.paused) {
            (Some((_, due)), _) => Some((due - cx.now()).max(0.0)),
            (None, Some(rest)) => Some(rest),
            (None, None) => None,
        };
        state.elapsed = left
            .map(|left| (self.gap - left).max(0.0))
            .filter(|e| *e > 0.0);
        None
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
                self.outstanding = self.outstanding.saturating_sub(1);
                if self.outstanding == 0 {
                    self.arrival_done(cx);
                }
            }
            return;
        }
        if tag == SPAWN_NEXT {
            self.spawn_next = None;
            if let Some((left, row)) = self.burst.front_mut() {
                let row = *row;
                *left -= 1;
                if *left == 0 {
                    self.burst.pop_front();
                }
                self.spawn_one(cx, row);
            }
            self.schedule_spawn(cx);
            return;
        }
        if tag == TABLE_ROW {
            self.next = None;
            let row = self.row;
            let count = self.table.as_ref().map_or(0, |t| t.rows[row].count);
            self.advance_row();
            self.arrive(cx, count, Some(row));
            self.schedule_row(cx);
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
        let (count, row) = match &self.table {
            // Sequence: the next row.
            Some(t) => (t.rows[self.row].count, Some(self.row)),
            None => (self.batch_size(cx), None),
        };
        if row.is_some() {
            self.advance_row();
        }
        self.arrive(cx, count, row);
        if self.outstanding == 0 {
            // A row with no items: the next one follows.
            self.arrival_done(cx);
        }
    }

    fn on_exit(&mut self, cx: &mut Ctx, _item: ItemId) {
        self.entered += 1;
        self.outstanding = self.outstanding.saturating_sub(1);
        if self.outstanding > 0 {
            return;
        }
        if self.timed() {
            self.arrival_done(cx);
        } else {
            self.schedule_next(cx);
        }
    }

    fn on_warmup(&mut self, _cx: &mut Ctx) {
        self.entered = 0;
        self.lost = 0;
        self.arrivals_window = 0;
    }

    /// For `Drop` sources: `lost.count`, `entered.count` and `loss_fraction`, always
    /// present (zero when nothing was lost) so they work as optimisation objectives.
    ///
    /// With a table: `arrivals.count` (arrivals since the warm-up) and `entered.count`
    /// (items that left).
    fn custom_metrics(&self, _now: Time) -> Vec<(String, f64)> {
        let mut out = Vec::new();
        if self.counts_arrivals() {
            out.push(("arrivals.count".to_string(), self.arrivals_window as f64));
            if self.on_blocked != OnBlocked::Drop {
                out.push(("entered.count".to_string(), self.entered as f64));
            }
        }
        if self.on_blocked != OnBlocked::Drop {
            return out;
        }
        let total = (self.lost + self.entered) as f64;
        out.extend([
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
        ]);
        out
    }

    fn on_stop(&mut self, cx: &mut Ctx) {
        if let Some((event, due)) = self.next.take() {
            cx.cancel(event);
            self.paused = Some((due - cx.now()).max(0.0));
        }
        if let Some((event, due)) = self.spawn_next.take() {
            cx.cancel(event);
            self.spawn_paused = Some((due - cx.now()).max(0.0));
        }
    }

    fn on_resume(&mut self, cx: &mut Ctx) {
        if let Some(left) = self.spawn_paused.take() {
            let event = cx.schedule(left, SPAWN_NEXT, None);
            self.spawn_next = Some((event, cx.now() + left));
        }
        if self.timed() {
            // Rows due while stopped arrive now, in order.
            if self.paused.take().is_some() {
                self.schedule_row(cx);
            }
            return;
        }
        if let Some(remaining) = self.paused.take() {
            self.arrive_in(cx, remaining);
        }
    }
}
