//! Scenario experiments: one base model, several parameter patches, shared seeds.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use utsim_core::stats::Summary;
use utsim_core::summarize;

use crate::{Model, ModelError, ObjectKind};

/// An experiment file: a base model plus named scenarios that patch it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "Experiment")]
pub struct Experiment {
    pub name: String,
    /// Model path, relative to the experiment file.
    pub model: String,
    /// Overrides `run.replications` of the base model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replications: Option<u32>,
    pub scenarios: Vec<Scenario>,
    /// Metric keys to keep (e.g. `fila.content.mean`); empty keeps all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measures: Vec<String>,
    /// Directory of the experiment file, used to resolve `model`.
    #[serde(skip)]
    base_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    /// `<object>.<field>` or `run.<field>` to RON source text of the new value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScenarioResult {
    pub name: String,
    pub summary: BTreeMap<String, Summary>,
    /// Value of every kept metric in each replication (paired across scenarios by
    /// replication, since every scenario uses the same seeds).
    #[serde(skip)]
    pub per_replication: BTreeMap<String, Vec<f64>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExperimentResult {
    pub name: String,
    pub measures: Vec<String>,
    pub scenarios: Vec<ScenarioResult>,
}

fn options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

fn invalid(msg: String) -> ModelError {
    ModelError::Invalid(vec![msg])
}

impl Experiment {
    /// An experiment over the model file `model` (relative to where it will be saved).
    pub fn new(
        name: impl Into<String>,
        model: impl Into<String>,
        replications: Option<u32>,
        scenarios: Vec<Scenario>,
        measures: Vec<String>,
    ) -> Experiment {
        Experiment {
            name: name.into(),
            model: model.into(),
            replications,
            scenarios,
            measures,
            base_dir: PathBuf::new(),
        }
    }

    pub fn parse(text: &str, base_dir: impl Into<PathBuf>) -> Result<Experiment, ModelError> {
        let mut exp: Experiment = options().from_str(text)?;
        exp.base_dir = base_dir.into();
        Ok(exp)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Experiment, ModelError> {
        let path = path.as_ref();
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Experiment::parse(&std::fs::read_to_string(path)?, dir)
    }

    /// Path of the base model (`model` resolved against the experiment file).
    pub fn model_path(&self) -> PathBuf {
        self.base_dir.join(&self.model)
    }

    /// The experiment as RON text, ready to be saved and read back by [`Experiment::parse`].
    pub fn to_ron(&self) -> String {
        let config = ron::ser::PrettyConfig::new().struct_names(true);
        let body = ron::ser::to_string_pretty(self, config).expect("experiment serializes");
        format!("// Experimento do UTSim. Uso: utsim experiment <arquivo>\n{body}\n")
    }

    /// Runs every scenario. All scenarios keep the base seed, so replication `r`
    /// uses the same random streams in each one (common random numbers).
    pub fn run(&self) -> Result<ExperimentResult, ModelError> {
        let mut base = Model::load(self.model_path())?;
        if let Some(r) = self.replications {
            base.run.replications = r;
        }
        let scenarios = run_scenarios(&base, &self.scenarios, &self.measures, &|_, _| true)?;
        Ok(ExperimentResult {
            name: self.name.clone(),
            measures: self.measures.clone(),
            scenarios,
        })
    }
}

/// Runs `scenarios` over `base` (its `run.replications`, same seeds in every scenario).
/// `progress(scenario, done)` is called after each finished replication of a scenario;
/// returning `false` stops with [`ModelError::Cancelled`].
pub fn run_scenarios(
    base: &Model,
    scenarios: &[Scenario],
    measures: &[String],
    progress: &(dyn Fn(usize, u32) -> bool + Sync),
) -> Result<Vec<ScenarioResult>, ModelError> {
    let mut out = Vec::new();
    for (i, sc) in scenarios.iter().enumerate() {
        let model = apply_patches(base, &sc.set)
            .map_err(|e| invalid(format!("cenário '{}': {e}", sc.name)))?;
        let reports = model.run_replications_with(&|done| progress(i, done))?;
        let mut summary = summarize(&reports);
        let mut per_replication = crate::analysis::per_replication(&reports);
        if !measures.is_empty() {
            summary.retain(|k, _| measures.contains(k));
            per_replication.retain(|k, _| measures.contains(k));
        }
        out.push(ScenarioResult {
            name: sc.name.clone(),
            summary,
            per_replication,
        });
    }
    Ok(out)
}

impl ExperimentResult {
    /// Every metric of the result: `measures` when given, else all, sorted.
    pub fn metric_names(&self) -> Vec<String> {
        if !self.measures.is_empty() {
            return self.measures.clone();
        }
        let mut all: Vec<String> = self
            .scenarios
            .iter()
            .flat_map(|s| s.summary.keys().cloned())
            .collect();
        all.sort();
        all.dedup();
        all
    }

    /// Paired difference of `metric` between scenario `i` and the first one
    /// (`None` for the first scenario itself or when the metric is missing).
    pub fn difference_to_first(&self, i: usize, metric: &str) -> Option<Summary> {
        if i == 0 {
            return None;
        }
        let a = self.scenarios.get(i)?.per_replication.get(metric)?;
        let b = self.scenarios.first()?.per_replication.get(metric)?;
        Some(crate::analysis::paired_difference(a, b))
    }
}

/// Applies every `set` entry to a copy of `base` and validates the result.
pub fn apply_patches(base: &Model, set: &BTreeMap<String, String>) -> Result<Model, ModelError> {
    let mut model = base.clone();
    for (key, value) in set {
        patch(&mut model, key, value)?;
    }
    model.validate()?;
    Ok(model)
}

fn parse_value<T: DeserializeOwned>(key: &str, text: &str) -> Result<T, ModelError> {
    options()
        .from_str(text)
        .map_err(|e| invalid(format!("valor inválido para '{key}' ({text}): {e}")))
}

/// Sets one field, addressed as `<object>.<field>` or `run.<field>`.
///
/// Typed on purpose: going through `ron::Value` loses enum variants.
pub fn patch(model: &mut Model, key: &str, text: &str) -> Result<(), ModelError> {
    let (target, field) = key.split_once('.').ok_or_else(|| {
        invalid(format!(
            "chave '{key}' inválida: use <objeto>.<campo> ou run.<campo>"
        ))
    })?;

    macro_rules! set {
        ($place:expr) => {
            $place = parse_value(key, text)?
        };
    }

    if target == "calendars" {
        // `calendars.<name>`: replaces (or adds) a calendar; `None` removes it.
        // `calendars.<name>.on_end` changes only the end-of-shift behaviour.
        if let Some(name) = field.strip_suffix(".on_end") {
            let table = model
                .calendars
                .get_mut(name)
                .ok_or_else(|| invalid(format!("'{key}': o calendário '{name}' não existe")))?;
            set!(table.on_end);
            return Ok(());
        }
        if text.trim() == "None" {
            model.calendars.remove(field);
        } else {
            let table: utsim_core::ShiftTable = parse_value(key, text)?;
            model.calendars.insert(field.to_string(), table);
        }
        return Ok(());
    }

    if target == "flow" {
        // `flow.<name>`: replaces (or adds) the whole flow; `None` removes it.
        let name = field;
        if text.trim() == "None" {
            model.flows.retain(|f| f.name != name);
            return Ok(());
        }
        let mut flow: utsim_flow::FlowDef = parse_value(key, text)?;
        flow.name = name.to_string();
        match model.flows.iter_mut().find(|f| f.name == name) {
            Some(existing) => *existing = flow,
            None => model.flows.push(flow),
        }
        return Ok(());
    }

    if target == "run" {
        let run = &mut model.run;
        match field {
            "stop_time" => set!(run.stop_time),
            "warmup" => set!(run.warmup),
            "seed" => set!(run.seed),
            "replications" => set!(run.replications),
            "sample_interval" => set!(run.sample_interval),
            "histogram_bins" => set!(run.histogram_bins),
            "default_decision" => set!(run.default_decision),
            "transfer" => set!(run.transfer),
            "transfer.speed" | "transfer.delay" => {
                patch_transfer(&mut run.transfer, key, field, text)?;
            }
            _ => {
                return Err(invalid(format!(
                    "'{key}': campo 'run.{field}' não existe; campos válidos: stop_time, warmup, seed, replications, sample_interval, histogram_bins, default_decision, transfer, transfer.speed, transfer.delay"
                )));
            }
        }
        return Ok(());
    }

    let known: Vec<String> = model.objects.iter().map(|o| o.name.clone()).collect();
    let obj = model
        .objects
        .iter_mut()
        .find(|o| o.name == target)
        .ok_or_else(|| {
            invalid(format!(
                "'{key}': objeto '{target}' não existe; objetos: {}",
                known.join(", ")
            ))
        })?;

    match field {
        "output" => {
            set!(obj.output);
            return Ok(());
        }
        "position" => {
            set!(obj.position);
            return Ok(());
        }
        "downtime" => {
            set!(obj.downtime);
            return Ok(());
        }
        "shifts" => {
            set!(obj.shifts);
            return Ok(());
        }
        "shifts.on_end" => {
            let end: utsim_core::ShiftEnd = parse_value(key, text)?;
            match &mut obj.shifts {
                Some(crate::ShiftSpec::Inline(table)) => table.on_end = end,
                Some(crate::ShiftSpec::Calendar { on_end, .. }) => *on_end = Some(end),
                None => {
                    return Err(invalid(format!(
                        "'{key}': '{target}' não tem turnos; defina `shifts` primeiro"
                    )));
                }
            }
            return Ok(());
        }
        "transport" => {
            set!(obj.transport);
            return Ok(());
        }
        "on_entry" => {
            set!(obj.on_entry);
            return Ok(());
        }
        "transfer" => {
            set!(obj.transfer);
            return Ok(());
        }
        "transfer.speed" | "transfer.delay" => {
            return patch_transfer(&mut obj.transfer, key, field, text);
        }
        // Quality and safety blocks (`docs/formato_qualidade_seguranca.md`): the whole
        // block, or the numbers a teaching mission or an experiment varies.
        "quality" => {
            set!(obj.quality);
            return Ok(());
        }
        "inspection" => {
            set!(obj.inspection);
            return Ok(());
        }
        "rework" => {
            set!(obj.rework);
            return Ok(());
        }
        "sink_role" => {
            set!(obj.sink_role);
            return Ok(());
        }
        "ppe" => {
            set!(obj.ppe);
            return Ok(());
        }
        // Who the person is (sex, height, mass, age, experience, capacity) and how it tires
        // and learns: the whole `person` block, `None` to remove it.
        // The waiting area of a queue (a table, a pallet, a shelf, a floor), `None` for a
        // plain queue.
        "waiting_area" => {
            set!(obj.waiting_area);
            return Ok(());
        }
        "person" => {
            set!(obj.person);
            return Ok(());
        }
        "inspection.every"
        | "inspection.detect"
        | "inspection.false_reject"
        | "inspection.max_rework"
        | "inspection.cost" => {
            let Some(i) = obj.inspection.as_deref_mut() else {
                return Err(invalid(format!(
                    "'{key}': '{target}' não tem `inspection`; defina o bloco primeiro"
                )));
            };
            match field {
                "inspection.every" => set!(i.every),
                "inspection.detect" => set!(i.detect),
                "inspection.false_reject" => set!(i.false_reject),
                "inspection.max_rework" => set!(i.max_rework),
                _ => set!(i.cost),
            }
            return Ok(());
        }
        "rework.fix" | "rework.cost" => {
            let Some(r) = obj.rework.as_mut() else {
                return Err(invalid(format!(
                    "'{key}': '{target}' não tem `rework`; defina o bloco primeiro"
                )));
            };
            if field == "rework.fix" {
                set!(r.fix);
            } else {
                set!(r.cost);
            }
            return Ok(());
        }
        _ => {}
    }
    let (kind_name, kind_fields): (&str, &[&str]) = match &obj.kind {
        ObjectKind::Source { .. } => (
            "Source",
            &[
                "interarrival",
                "first_at_zero",
                "max_arrivals",
                "labels",
                "on_blocked",
                "schedule",
                "schedule.cycle",
                "schedule[n]",
                "schedule[n].start",
                "schedule[n].end",
                "schedule[n].arrivals",
                "schedule[n].rate",
                "schedule[n].interarrival",
                "schedule[n].labels",
                "batch",
                "release",
                "item_type",
                "item_type_mix",
                "table",
                "table.rows[n]",
                "table.rows[n].at",
                "table.rows[n].count",
                "table.rows[n].labels",
                "table.rows[n].labels.<nome>",
            ],
        ),
        ObjectKind::Queue { .. } => ("Queue", &["capacity", "discipline", "batch", "order"]),
        ObjectKind::Processor { .. } => (
            "Processor",
            &[
                "capacity",
                "setup",
                "process",
                "operators",
                "also",
                "changeover",
                "recipe",
            ],
        ),
        ObjectKind::Transporter { .. } => (
            "Transporter",
            &[
                "speed",
                "acceleration",
                "deceleration",
                "capacity",
                "load_time",
                "unload_time",
                "home",
                "agv",
                "home_at",
                "home_object",
                "break_at",
                "break_object",
            ],
        ),
        ObjectKind::Crane { .. } => (
            "Crane",
            &[
                "bridge",
                "trolley",
                "hoist",
                "hook_height",
                "attach",
                "detach",
                "capacity_t",
                "motion",
                "weight_label",
                "home",
                "load_zone",
                "operator",
                "bridge_gap",
                "keep_clear",
            ],
        ),
        ObjectKind::Dispatcher { .. } => (
            "Dispatcher",
            &["team", "rule", "pick", "preempt", "metrics"],
        ),
        ObjectKind::Conveyor { .. } => (
            "Conveyor",
            &["length", "speed", "item_length", "accumulating"],
        ),
        ObjectKind::Rack { .. } => (
            "Rack",
            &[
                "bays",
                "levels",
                "dwell",
                "placement",
                "bay_width",
                "level_height",
            ],
        ),
        ObjectKind::Combiner { .. } => ("Combiner", &["components", "mode", "setup", "process"]),
        ObjectKind::Separator { .. } => ("Separator", &["mode", "process"]),
        ObjectKind::Sink => ("Sink", &[]),
    };
    match (&mut obj.kind, field) {
        (ObjectKind::Source { interarrival, .. }, "interarrival") => set!(*interarrival),
        (ObjectKind::Source { first_at_zero, .. }, "first_at_zero") => set!(*first_at_zero),
        (ObjectKind::Source { max_arrivals, .. }, "max_arrivals") => set!(*max_arrivals),
        (ObjectKind::Source { labels, .. }, "labels") => set!(*labels),
        (ObjectKind::Source { on_blocked, .. }, "on_blocked") => set!(*on_blocked),
        (ObjectKind::Source { schedule, .. }, "schedule") => set!(*schedule),
        (ObjectKind::Source { schedule, .. }, f) if f.starts_with("schedule") => {
            return patch_schedule(schedule, key, &f["schedule".len()..], text);
        }
        (ObjectKind::Source { table, .. }, "table") => set!(*table),
        (ObjectKind::Source { batch, .. }, "batch") => set!(*batch),
        (ObjectKind::Source { release, .. }, "release") => set!(*release),
        (ObjectKind::Source { item_type, .. }, "item_type") => set!(*item_type),
        (ObjectKind::Source { item_type_mix, .. }, "item_type_mix") => set!(*item_type_mix),
        (ObjectKind::Source { table, .. }, f) if f.starts_with("table.rows[") => {
            return patch_table(table, key, &f["table.rows".len()..], text);
        }
        (ObjectKind::Queue { capacity, .. }, "capacity") => set!(*capacity),
        (ObjectKind::Queue { discipline, .. }, "discipline") => set!(*discipline),
        (ObjectKind::Queue { batch, .. }, "batch") => set!(*batch),
        (ObjectKind::Queue { order, .. }, "order") => set!(*order),
        (ObjectKind::Processor { capacity, .. }, "capacity") => set!(*capacity),
        (ObjectKind::Processor { setup, .. }, "setup") => set!(*setup),
        (ObjectKind::Processor { process, .. }, "process") => set!(*process),
        (ObjectKind::Processor { operators, .. }, "operators") => set!(*operators),
        (ObjectKind::Processor { also, .. }, "also") => set!(*also),
        (ObjectKind::Processor { changeover, .. }, "changeover") => set!(*changeover),
        (ObjectKind::Processor { recipe, .. }, "recipe") => set!(*recipe),
        (ObjectKind::Transporter { speed, .. }, "speed") => set!(*speed),
        (ObjectKind::Transporter { acceleration, .. }, "acceleration") => set!(*acceleration),
        (ObjectKind::Transporter { deceleration, .. }, "deceleration") => set!(*deceleration),
        (ObjectKind::Transporter { capacity, .. }, "capacity") => set!(*capacity),
        (ObjectKind::Transporter { load_time, .. }, "load_time") => set!(*load_time),
        (ObjectKind::Transporter { unload_time, .. }, "unload_time") => set!(*unload_time),
        (ObjectKind::Transporter { agv, .. }, "agv") => set!(*agv),
        (ObjectKind::Transporter { home, .. }, "home") => set!(*home),
        (ObjectKind::Transporter { home_at, .. }, "home_at") => set!(*home_at),
        (ObjectKind::Transporter { home_object, .. }, "home_object") => set!(*home_object),
        (ObjectKind::Transporter { break_at, .. }, "break_at") => set!(*break_at),
        (ObjectKind::Transporter { break_object, .. }, "break_object") => set!(*break_object),
        (ObjectKind::Crane { bridge, .. }, "bridge") => set!(*bridge),
        (ObjectKind::Crane { trolley, .. }, "trolley") => set!(*trolley),
        (ObjectKind::Crane { hoist, .. }, "hoist") => set!(*hoist),
        (ObjectKind::Crane { hook_height, .. }, "hook_height") => set!(*hook_height),
        (ObjectKind::Crane { attach, .. }, "attach") => set!(*attach),
        (ObjectKind::Crane { detach, .. }, "detach") => set!(*detach),
        (ObjectKind::Crane { capacity_t, .. }, "capacity_t") => set!(*capacity_t),
        (ObjectKind::Crane { motion, .. }, "motion") => set!(*motion),
        (ObjectKind::Crane { weight_label, .. }, "weight_label") => set!(*weight_label),
        (ObjectKind::Crane { home, .. }, "home") => set!(*home),
        (ObjectKind::Crane { load_zone, .. }, "load_zone") => set!(*load_zone),
        (ObjectKind::Crane { operator, .. }, "operator") => set!(*operator),
        (ObjectKind::Crane { bridge_gap, .. }, "bridge_gap") => set!(*bridge_gap),
        (ObjectKind::Crane { keep_clear, .. }, "keep_clear") => set!(*keep_clear),
        (ObjectKind::Dispatcher { team, .. }, "team") => set!(*team),
        (ObjectKind::Dispatcher { rule, .. }, "rule") => set!(*rule),
        (ObjectKind::Dispatcher { pick, .. }, "pick") => set!(*pick),
        (ObjectKind::Dispatcher { preempt, .. }, "preempt") => set!(*preempt),
        (ObjectKind::Dispatcher { metrics, .. }, "metrics") => set!(*metrics),
        (ObjectKind::Conveyor { length, .. }, "length") => set!(*length),
        (ObjectKind::Conveyor { speed, .. }, "speed") => set!(*speed),
        (ObjectKind::Conveyor { item_length, .. }, "item_length") => set!(*item_length),
        (ObjectKind::Conveyor { accumulating, .. }, "accumulating") => set!(*accumulating),
        (ObjectKind::Rack { bays, .. }, "bays") => set!(*bays),
        (ObjectKind::Rack { levels, .. }, "levels") => set!(*levels),
        (ObjectKind::Rack { dwell, .. }, "dwell") => set!(*dwell),
        (ObjectKind::Rack { placement, .. }, "placement") => set!(*placement),
        (ObjectKind::Rack { bay_width, .. }, "bay_width") => set!(*bay_width),
        (ObjectKind::Rack { level_height, .. }, "level_height") => set!(*level_height),
        (ObjectKind::Combiner { components, .. }, "components") => set!(*components),
        (ObjectKind::Combiner { mode, .. }, "mode") => set!(*mode),
        (ObjectKind::Combiner { setup, .. }, "setup") => set!(*setup),
        (ObjectKind::Combiner { process, .. }, "process") => set!(*process),
        (ObjectKind::Separator { mode, .. }, "mode") => set!(*mode),
        (ObjectKind::Separator { process, .. }, "process") => set!(*process),
        _ => {
            let valid: Vec<&str> = kind_fields
                .iter()
                .copied()
                .chain([
                    "output",
                    "position",
                    "downtime",
                    "shifts",
                    "shifts.on_end",
                    "transport",
                    "on_entry",
                    "transfer",
                    "transfer.speed",
                    "transfer.delay",
                    "quality",
                    "inspection",
                    "inspection.every",
                    "inspection.detect",
                    "inspection.false_reject",
                    "inspection.max_rework",
                    "inspection.cost",
                    "rework",
                    "rework.fix",
                    "rework.cost",
                    "sink_role",
                    "ppe",
                ])
                .collect();
            return Err(invalid(format!(
                "'{key}': '{target}' ({kind_name}) não tem o campo '{field}'; campos válidos: {}",
                valid.join(", ")
            )));
        }
    }
    Ok(())
}

/// Sets part of a source's `schedule`. `rest` is what follows `schedule` in the key:
/// `.cycle`, `[n]` (a whole period, numbered from 1) or `[n].<field>` with `start`, `end`,
/// `arrivals`, `labels`, and the shortcuts `rate` (`arrivals: Rate(v)`) and `interarrival`
/// (`arrivals: Interarrival(d)`).
fn patch_schedule(
    schedule: &mut Option<utsim_objects::ArrivalSchedule>,
    key: &str,
    rest: &str,
    text: &str,
) -> Result<(), ModelError> {
    use utsim_objects::PeriodArrivals;
    let Some(s) = schedule.as_mut() else {
        return Err(invalid(format!(
            "'{key}': a fonte não tem `schedule`; defina a tabela de períodos primeiro"
        )));
    };
    if rest == ".cycle" {
        s.cycle = parse_value(key, text)?;
        return Ok(());
    }
    let bad = || {
        invalid(format!(
            "'{key}': use schedule, schedule.cycle, schedule[n] ou schedule[n].<campo> \
             (start, end, arrivals, rate, interarrival, labels), com n a partir de 1"
        ))
    };
    let inner = rest.strip_prefix('[').ok_or_else(bad)?;
    let (index, after) = inner.split_once(']').ok_or_else(bad)?;
    let n: usize = index.trim().parse().map_err(|_| bad())?;
    let count = s.periods.len();
    if n == 0 || n > count {
        return Err(invalid(format!(
            "'{key}': período {n} não existe; a tabela tem {count} período(s), numerados de 1 a {count}"
        )));
    }
    let period = &mut s.periods[n - 1];
    match after {
        "" => period.clone_from(&parse_value(key, text)?),
        ".start" => period.start = parse_value(key, text)?,
        ".end" => period.end = parse_value(key, text)?,
        ".arrivals" => period.arrivals = parse_value(key, text)?,
        ".rate" => period.arrivals = PeriodArrivals::Rate(parse_value(key, text)?),
        ".interarrival" => {
            period.arrivals = PeriodArrivals::Interarrival(parse_value(key, text)?);
        }
        ".labels" => period.labels = parse_value(key, text)?,
        _ => return Err(bad()),
    }
    Ok(())
}

/// Sets part of a source's arrival `table`. `rest` is what follows `table.rows` in the
/// key: `[n]` (a whole row, numbered from 1) or `[n].<field>` with `at`, `count`,
/// `labels` and `labels.<name>` (one constant label).
fn patch_table(
    table: &mut Option<utsim_objects::ArrivalTable>,
    key: &str,
    rest: &str,
    text: &str,
) -> Result<(), ModelError> {
    let Some(t) = table.as_mut() else {
        return Err(invalid(format!(
            "'{key}': a fonte não tem `table`; defina a tabela de chegadas primeiro"
        )));
    };
    let bad = || {
        invalid(format!(
            "'{key}': use table.rows[n] ou table.rows[n].<campo> (at, count, labels, \
             labels.<nome>), com n a partir de 1"
        ))
    };
    let inner = rest.strip_prefix('[').ok_or_else(bad)?;
    let (index, after) = inner.split_once(']').ok_or_else(bad)?;
    let n: usize = index.trim().parse().map_err(|_| bad())?;
    let count = t.rows.len();
    if n == 0 || n > count {
        return Err(invalid(format!(
            "'{key}': linha {n} não existe; a tabela tem {count} linha(s), numeradas de 1 a {count}"
        )));
    }
    let row = &mut t.rows[n - 1];
    match after {
        "" => row.clone_from(&parse_value(key, text)?),
        ".at" => row.at = parse_value(key, text)?,
        ".count" => row.count = parse_value(key, text)?,
        ".labels" => row.labels = parse_value(key, text)?,
        f if f.starts_with(".labels.") && f.len() > ".labels.".len() => {
            row.labels
                .insert(f[".labels.".len()..].to_string(), parse_value(key, text)?);
        }
        _ => return Err(bad()),
    }
    Ok(())
}

/// Sets `transfer.speed` or `transfer.delay` of an existing transfer definition.
fn patch_transfer(
    transfer: &mut Option<utsim_core::TransferDef>,
    key: &str,
    field: &str,
    text: &str,
) -> Result<(), ModelError> {
    let value: f64 = parse_value(key, text)?;
    let Some(def) = transfer.as_mut() else {
        return Err(invalid(format!(
            "'{key}': defina `transfer` primeiro (por exemplo (speed: 1.0)); só então dá para variar {field}"
        )));
    };
    if field.ends_with("speed") {
        def.speed = value;
    } else {
        def.delay = value;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use utsim_core::{Dist, OutputRule};
    use utsim_objects::Discipline;

    use super::*;

    fn model() -> Model {
        Model::parse(
            r#"Model(
                name: "t",
                run: (stop_time: 100.0),
                objects: [
                    (name: "src", kind: Source(interarrival: Exp(mean: 1.0))),
                    (name: "q", kind: Queue()),
                    (name: "m", kind: Processor(process: Exp(mean: 0.8))),
                    (name: "out", kind: Sink),
                ],
                connections: [("src", "q"), ("q", "m"), ("m", "out")],
            )"#,
        )
        .unwrap()
    }

    fn set(pairs: &[(&str, &str)]) -> Result<Model, ModelError> {
        let map = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        apply_patches(&model(), &map)
    }

    fn kind<'a>(m: &'a Model, name: &str) -> &'a ObjectKind {
        &m.objects.iter().find(|o| o.name == name).unwrap().kind
    }

    #[test]
    fn patches_dist_and_primitive_fields() {
        let m = set(&[
            ("m.process", "Const(2.5)"),
            ("m.capacity", "3"),
            ("run.stop_time", "50.0"),
            ("run.seed", "9"),
        ])
        .unwrap();
        match kind(&m, "m") {
            ObjectKind::Processor {
                capacity, process, ..
            } => {
                assert_eq!(*capacity, 3);
                assert_eq!(*process, Dist::Const(2.5));
            }
            _ => unreachable!(),
        }
        assert_eq!((m.run.stop_time, m.run.seed), (50.0, 9));
    }

    #[test]
    fn patches_series_and_histograms_of_the_run() {
        let m = set(&[
            ("run.sample_interval", "10.0"),
            ("run.histogram_bins", "Some(20)"),
        ])
        .unwrap();
        assert_eq!(m.run.sample_interval, Some(10.0));
        assert_eq!(m.run.histogram_bins, Some(20));
        let m = set(&[("run.sample_interval", "None")]).unwrap();
        assert_eq!(m.run.sample_interval, None);
    }

    #[test]
    fn patches_options_with_implicit_some() {
        let m = set(&[
            ("q.capacity", "5"),
            ("m.setup", "Const(1.0)"),
            ("src.max_arrivals", "10"),
        ])
        .unwrap();
        assert!(matches!(
            kind(&m, "q"),
            ObjectKind::Queue {
                capacity: Some(5),
                ..
            }
        ));
        assert!(matches!(
            kind(&m, "m"),
            ObjectKind::Processor {
                setup: Some(Dist::Const(_)),
                ..
            }
        ));
        assert!(matches!(
            kind(&m, "src"),
            ObjectKind::Source {
                max_arrivals: Some(10),
                ..
            }
        ));
        let mut m = m;
        patch(&mut m, "q.capacity", "None").unwrap();
        assert!(matches!(
            kind(&m, "q"),
            ObjectKind::Queue { capacity: None, .. }
        ));
    }

    #[test]
    fn patches_enums_bool_map_and_object_fields() {
        let m = set(&[
            ("q.discipline", "Lifo"),
            ("src.first_at_zero", "true"),
            ("src.on_blocked", "Drop"),
            ("src.labels", r#"{"tipo": Uniform(min: 1.0, max: 2.0)}"#),
            ("m.output", "RoundRobin"),
            ("m.position", "(1.0, 2.0, 3.0)"),
            ("m.downtime", "(mtbf: Exp(mean: 50.0), mttr: Const(2.0))"),
        ])
        .unwrap();
        assert!(matches!(
            kind(&m, "q"),
            ObjectKind::Queue {
                discipline: Discipline::Lifo,
                ..
            }
        ));
        match kind(&m, "src") {
            ObjectKind::Source {
                first_at_zero,
                on_blocked,
                labels,
                ..
            } => {
                assert!(*first_at_zero);
                assert_eq!(*on_blocked, utsim_objects::OnBlocked::Drop);
                assert!(labels.contains_key("tipo"));
            }
            _ => unreachable!(),
        }
        let obj = m.objects.iter().find(|o| o.name == "m").unwrap();
        assert_eq!(obj.output, OutputRule::RoundRobin);
        assert_eq!(obj.position, [1.0, 2.0, 3.0]);
        assert!(obj.downtime.is_some());
    }

    #[test]
    fn unknown_object_lists_valid_ones() {
        let e = set(&[("xyz.process", "Const(1.0)")])
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("objeto 'xyz' não existe") && e.contains("src, q, m, out"),
            "{e}"
        );
    }

    #[test]
    fn unknown_field_lists_valid_ones() {
        let e = set(&[("m.velocidade", "1")]).unwrap_err().to_string();
        assert!(
            e.contains("Processor")
                && e.contains(
                    "capacity, setup, process, operators, also, changeover, recipe, output"
                ),
            "{e}"
        );
        let e = set(&[("out.process", "Const(1.0)")])
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("Sink") && e.contains("output, position, downtime"),
            "{e}"
        );
        let e = set(&[("run.foo", "1")]).unwrap_err().to_string();
        assert!(e.contains("stop_time, warmup, seed, replications"), "{e}");
        let e = set(&[("semponto", "1")]).unwrap_err().to_string();
        assert!(e.contains("<objeto>.<campo>"), "{e}");
    }

    #[test]
    fn bad_value_reports_key_and_text() {
        let e = set(&[("m.capacity", "muitas")]).unwrap_err().to_string();
        assert!(
            e.contains("valor inválido para 'm.capacity' (muitas)"),
            "{e}"
        );
        let e = set(&[("m.process", "Exp(")]).unwrap_err().to_string();
        assert!(e.contains("m.process"), "{e}");
    }

    #[test]
    fn patched_model_is_validated() {
        let e = set(&[("m.capacity", "0")]).unwrap_err().to_string();
        assert!(e.contains("capacity deve ser >= 1"), "{e}");
    }

    #[test]
    fn parses_experiment_file() {
        let exp = Experiment::parse(
            r#"Experiment(name: "e", model: "m.ron", replications: 3,
                scenarios: [(name: "a"), (name: "b", set: {"m.capacity": "2"})],
                measures: ["q.content.mean"])"#,
            ".",
        )
        .unwrap();
        assert_eq!(exp.replications, Some(3));
        assert_eq!(exp.scenarios[1].set["m.capacity"], "2");
    }

    fn scenario(name: &str, set: &[(&str, &str)]) -> Scenario {
        Scenario {
            name: name.into(),
            set: set
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn experiment_round_trips_through_ron() {
        let exp = Experiment::new(
            "e",
            "m.ron",
            Some(4),
            vec![
                scenario("base", &[]),
                scenario("dois", &[("m.capacity", "2")]),
            ],
            vec!["q.content.mean".into()],
        );
        let text = exp.to_ron();
        assert!(text.contains("Experiment("), "{text}");
        let back = Experiment::parse(&text, "dir").unwrap();
        assert_eq!(back.scenarios, exp.scenarios);
        assert_eq!(back.replications, Some(4));
        assert_eq!(back.measures, exp.measures);
        assert_eq!(back.model_path(), PathBuf::from("dir").join("m.ron"));
    }

    #[test]
    fn scenarios_are_paired_by_replication() {
        let mut base = model();
        base.run.replications = 4;
        base.run.stop_time = 5000.0;
        let scenarios = vec![
            scenario("base", &[]),
            scenario("rapida", &[("m.process", "Exp(mean: 0.4)")]),
        ];
        let measures = vec!["q.content.mean".to_string()];
        let scenarios = run_scenarios(&base, &scenarios, &measures, &|_, _| true).unwrap();
        let result = ExperimentResult {
            name: "e".into(),
            measures: measures.clone(),
            scenarios,
        };
        assert_eq!(
            result.scenarios[1].per_replication["q.content.mean"].len(),
            4
        );
        assert!(result.difference_to_first(0, "q.content.mean").is_none());
        let d = result.difference_to_first(1, "q.content.mean").unwrap();
        assert!(d.mean < 0.0 && crate::analysis::is_significant(&d), "{d:?}");
        assert_eq!(result.metric_names(), measures);
        let stopped = run_scenarios(&base, &[scenario("a", &[])], &[], &|_, _| false);
        assert!(matches!(stopped, Err(ModelError::Cancelled)));
    }
}
