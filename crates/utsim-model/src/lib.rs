//! Model description: what is written in a `.ron` file and how it becomes a [`Simulation`].

pub mod analysis;
pub mod csv;
pub mod experiment;
pub mod html;
pub mod replay;
mod shifts;
pub mod synthetic;
mod warnings;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use utsim_core::{
    Behavior, BlockId, Dist, Downtime, OutputRule, RunConfig, RunReport, ShiftTable, SimError,
    Simulation, TransferDef, TransferPath,
};
use utsim_flow::{Flow, FlowDef};
use utsim_fluid::{FluidDef, FluidNet};
use utsim_objects::network::NetworkDef;
use utsim_objects::traffic::Traffic;
use utsim_objects::{
    Batch, CombineMode, Combiner, Conveyor, Discipline, DispatchRule, Dispatcher, OnBlocked,
    OperatorUse, Processor, Queue, Rack, RackPlacement, SeparateMode, Separator, Sink, Source,
    Transporter,
};

pub use shifts::ShiftSpec;
pub use utsim_core::ShiftEnd;
pub use utsim_objects::{ArrivalPeriod, ArrivalSchedule, PeriodArrivals};

/// Reads only the `format` field; every other field is ignored.
#[derive(Deserialize)]
#[serde(rename = "Model")]
struct FormatProbe {
    #[serde(default)]
    format: Option<u32>,
}

/// One migration step: rewrites RON text from one format version to the next. It works on
/// text (not on a generic `ron::Value`) because a `Value` loses enum variants, which would
/// corrupt the typed deserialization that follows.
type Migration = fn(&str) -> Result<String, ModelError>;

/// Migrations to the current format, as `(source version, step)` in ascending order.
/// To add format 2: bump [`FORMAT_VERSION`], push `(1, v1_to_v2)` here (the step rewrites
/// the text and sets `format: 2`), keep an old-format example in the tests, and document it.
const MIGRATIONS: &[(u32, Migration)] = &[];

/// Applies every step whose source version is `>= from`, in order. A model already at the
/// current version passes through untouched.
fn migrate(text: &str, from: u32, steps: &[(u32, Migration)]) -> Result<String, ModelError> {
    let mut text = text.to_string();
    for (_, step) in steps.iter().filter(|(source, _)| *source >= from) {
        text = step(&text)?;
    }
    Ok(text)
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("não foi possível ler o modelo: {0}")]
    Io(#[from] std::io::Error),
    #[error("erro de sintaxe no modelo: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("modelo no formato {found}; esta versão do UTSim lê até {supported}")]
    Format { found: u32, supported: u32 },
    #[error("modelo inválido:\n  - {}", .0.join("\n  - "))]
    Invalid(Vec<String>),
    #[error(transparent)]
    Sim(#[from] SimError),
    /// The run was stopped from outside (the progress callback asked to stop).
    #[error("execução cancelada")]
    Cancelled,
}

/// Most arrivals a single source may be expected to create in one replication.
const MAX_ARRIVALS: f64 = 1.0e9;

/// Largest count that allocates or loops per use (rack positions, split copies, operators).
const MAX_COUNT: usize = 1_000_000;

/// Current version of the model file format (see `docs/versionamento.md`).
/// Files without a `format` field are version 1.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Model {
    /// Format version of the file. Absent means 1; newer than [`FORMAT_VERSION`] is rejected.
    #[serde(default)]
    pub format: Option<u32>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub run: RunSettings,
    /// Named shift tables that objects reference with `shifts: Calendar("name")`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub calendars: BTreeMap<String, ShiftTable>,
    pub objects: Vec<ObjectDef>,
    /// `(from, to)` pairs. Order defines port numbers and pull priority.
    #[serde(default)]
    pub connections: Vec<(String, String)>,
    /// Paths for mobile resources (straight lines when absent).
    #[serde(default)]
    pub network: Option<NetworkDef>,
    /// Token flows: user logic as graphs of activities (phase 6).
    #[serde(default)]
    pub flows: Vec<FlowDef>,
    /// Continuous (fluid) network (phase 7).
    #[serde(default)]
    pub fluids: Option<FluidDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunSettings {
    pub stop_time: f64,
    #[serde(default)]
    pub warmup: f64,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default = "one_u32")]
    pub replications: u32,
    /// Sample block content/state every this many time units (None = off).
    #[serde(default)]
    pub sample_interval: Option<f64>,
    /// Build histograms with this many bins (None = off).
    #[serde(default)]
    pub histogram_bins: Option<usize>,
    /// For models that ask an external agent (`Decide` with `Agent`): the option
    /// chosen when the model runs without one (`utsim run`, replay, 3D). `None`:
    /// such models only run through the step-by-step API (e.g. `utsim-env`).
    #[serde(default)]
    pub default_decision: Option<usize>,
    /// Default travel time for direct connections (an object's own `transfer` wins).
    /// Objects with `transport` and conveyors (and objects feeding them) are not affected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<TransferDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectDef {
    pub name: String,
    pub kind: ObjectKind,
    #[serde(default)]
    pub output: OutputRule,
    /// Layout position (x, y, z), used by the 3D view from phase 2 on.
    #[serde(default)]
    pub position: [f64; 3],
    /// Random breakdowns (MTBF/MTTR on the clock).
    #[serde(default)]
    pub downtime: Option<Downtime>,
    /// Scheduled off-shift windows, repeating every `period`: inline or one of the
    /// model's `calendars`.
    #[serde(default)]
    pub shifts: Option<ShiftSpec>,
    /// Transporter or dispatcher that carries items out of this object
    /// (instead of handing them over instantly).
    #[serde(default)]
    pub transport: Option<String>,
    /// Travel time of items leaving this object through direct connections:
    /// `delay + distance / speed`, with room reserved at the destination meanwhile.
    /// Absent means instantaneous (unless `run.transfer` is set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<TransferDef>,
    /// Script run on every item entering this object (e.g. to set labels).
    #[serde(default)]
    pub on_entry: Option<String>,
    /// How the object looks in 3D (asset, scale, colour). Presentation only: never
    /// read by the simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual: Option<VisualDef>,
}

/// Look of an object in 3D. Every field is optional; the viewer falls back to the
/// default shape of the object's kind.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VisualDef {
    /// Asset from the built-in library (e.g. "empilhadeira", "operador", "agv",
    /// "porta_paletes") or a `.glb` file, relative to the model file.
    #[serde(default)]
    pub asset: Option<String>,
    /// Uniform scale applied to the asset (1.0 = the asset's own size, in metres).
    #[serde(default)]
    pub scale: Option<f64>,
    /// Rotation around the vertical axis, in degrees (counter-clockwise seen from above).
    #[serde(default)]
    pub yaw: Option<f64>,
    /// Colour override, "#rrggbb".
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ObjectKind {
    Source {
        interarrival: Dist,
        #[serde(default)]
        first_at_zero: bool,
        #[serde(default)]
        max_arrivals: Option<u64>,
        #[serde(default)]
        labels: BTreeMap<String, Dist>,
        /// What to do with an item that cannot leave when created.
        #[serde(default)]
        on_blocked: OnBlocked,
        /// Arrivals by period of a repeating cycle (absent: `interarrival` all the time).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schedule: Option<ArrivalSchedule>,
    },
    Queue {
        #[serde(default)]
        capacity: Option<usize>,
        #[serde(default)]
        discipline: Discipline,
        /// Holds items and releases them in groups.
        #[serde(default)]
        batch: Option<Batch>,
    },
    Processor {
        #[serde(default = "one_usize")]
        capacity: usize,
        #[serde(default)]
        setup: Option<Dist>,
        process: Dist,
        /// Operators needed during setup and/or processing.
        #[serde(default)]
        operators: Option<OperatorUse>,
    },
    /// Input 0 takes the container; inputs 1.. take `components[k-1]` items each.
    Combiner {
        components: Vec<usize>,
        #[serde(default)]
        mode: CombineMode,
        #[serde(default)]
        setup: Option<Dist>,
        process: Dist,
    },
    Separator {
        #[serde(default)]
        mode: SeparateMode,
        process: Dist,
    },
    Sink,
    /// Mobile resource: forklift, AGV, person. Not connected by ports; objects
    /// point to it with `transport`, or processors with `operators`.
    Transporter {
        /// Maximum speed (length units per time unit).
        speed: f64,
        #[serde(default)]
        acceleration: Option<f64>,
        #[serde(default)]
        deceleration: Option<f64>,
        #[serde(default = "one_usize")]
        capacity: usize,
        #[serde(default = "zero")]
        load_time: Dist,
        #[serde(default = "zero")]
        unload_time: Dist,
        /// Drive back to this object's position when idle with nothing queued.
        #[serde(default)]
        home: bool,
        /// Follow the network edge by edge under traffic control (edge capacities),
        /// waiting when an edge is full. The AGV front implements it.
        #[serde(default)]
        agv: bool,
    },
    /// Kinematic belt: items travel `length` at `speed`, each taking `item_length`.
    Conveyor {
        length: f64,
        speed: f64,
        item_length: f64,
        /// Items queue up at the end when blocked (true) or the whole belt stops (false).
        #[serde(default = "yes")]
        accumulating: bool,
    },
    /// Addressable storage: `bays` × `levels` positions.
    Rack {
        bays: usize,
        levels: usize,
        /// Minimum time each item stays stored.
        #[serde(default)]
        dwell: Option<Dist>,
        #[serde(default)]
        placement: RackPlacement,
        /// 3D spacing between bays and between levels.
        #[serde(default = "one_f64")]
        bay_width: f64,
        #[serde(default = "one_f64")]
        level_height: f64,
    },
    /// Hands work to a team of transporters.
    Dispatcher {
        team: Vec<String>,
        #[serde(default)]
        rule: DispatchRule,
    },
}

fn yes() -> bool {
    true
}

fn one_f64() -> f64 {
    1.0
}

fn zero() -> Dist {
    Dist::Const(0.0)
}

/// Mobile resources and dispatchers live outside the port graph.
pub fn is_mobile(kind: &ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Transporter { .. } | ObjectKind::Dispatcher { .. }
    )
}

fn default_seed() -> u64 {
    1
}

fn one_u32() -> u32 {
    1
}

fn one_usize() -> usize {
    1
}

impl Model {
    pub fn parse(text: &str) -> Result<Model, ModelError> {
        let options = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
        let found = options.from_str::<FormatProbe>(text)?.format.unwrap_or(1);
        if found > FORMAT_VERSION {
            return Err(ModelError::Format {
                found,
                supported: FORMAT_VERSION,
            });
        }
        let text = migrate(text, found, MIGRATIONS)?;
        Ok(options.from_str(&text)?)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Model, ModelError> {
        Model::parse(&std::fs::read_to_string(path)?)
    }

    /// Travel time that applies to the items leaving `obj` through direct connections.
    /// The object's own `transfer` wins over `run.transfer`; mobile resources and objects
    /// carried by a transporter never have one. The run default skips conveyors and the
    /// objects that feed them: a belt carries items by itself and its position is its middle.
    pub fn effective_transfer(&self, obj: &ObjectDef) -> Option<TransferDef> {
        if obj.transport.is_some() || is_mobile(&obj.kind) {
            return None;
        }
        if obj.transfer.is_some() {
            return obj.transfer;
        }
        let default = self.run.transfer?;
        let is_belt = |name: &str| {
            self.objects
                .iter()
                .any(|o| o.name == name && matches!(o.kind, ObjectKind::Conveyor { .. }))
        };
        let touches_belt = matches!(obj.kind, ObjectKind::Conveyor { .. })
            || self
                .connections
                .iter()
                .any(|(from, to)| *from == obj.name && is_belt(to));
        (!touches_belt).then_some(default)
    }

    /// The shift table in force for `obj`: its own, or the calendar it references (with
    /// the object's `on_end` when it sets one). `None` without shifts or when the calendar
    /// does not exist (a validation error).
    pub fn shift_table(&self, obj: &ObjectDef) -> Option<ShiftTable> {
        match obj.shifts.as_ref()? {
            ShiftSpec::Inline(table) => Some(table.clone()),
            ShiftSpec::Calendar { name, on_end } => {
                let mut table = self.calendars.get(name)?.clone();
                if let Some(end) = on_end {
                    table.on_end = *end;
                }
                Some(table)
            }
        }
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let mut errors = Vec::new();
        let run = &self.run;
        if let Some(t) = &run.transfer {
            if let Err(e) = t.validate() {
                errors.push(format!("run: {e}"));
            }
            if t.path == TransferPath::Network && self.network.is_none() {
                errors.push("run.transfer.path = Network exige `network` no modelo".to_string());
            }
        }
        if !(run.stop_time.is_finite() && run.stop_time > 0.0) {
            errors.push("run.stop_time deve ser finito e > 0".to_string());
        }
        if !(run.warmup >= 0.0 && run.warmup < run.stop_time) {
            errors.push("run.warmup deve estar em [0, stop_time)".to_string());
        }
        if run.replications == 0 {
            errors.push("run.replications deve ser >= 1".to_string());
        }

        if run
            .sample_interval
            .is_some_and(|v| !(v.is_finite() && v > 0.0))
        {
            errors.push("run.sample_interval deve ser finito e > 0".to_string());
        }
        if run.histogram_bins == Some(0) {
            errors.push("run.histogram_bins deve ser >= 1".to_string());
        }

        if self.objects.is_empty() && self.flows.is_empty() && self.fluids.is_none() {
            errors.push("o modelo está vazio: adicione objetos ou fluxos".to_string());
        }
        for (name, table) in &self.calendars {
            if name.trim().is_empty() {
                errors.push("há um calendário sem nome".to_string());
            }
            if let Err(e) = table.validate() {
                errors.push(format!("calendário '{name}': {e}"));
            }
        }
        let mut names = HashSet::new();
        for obj in &self.objects {
            if obj.name.trim().is_empty() {
                errors.push("há um objeto sem nome".to_string());
            } else if !names.insert(obj.name.as_str()) {
                errors.push(format!("nome repetido: '{}'", obj.name));
            }
            let mut dists: Vec<(String, &Dist)> = Vec::new();
            match &obj.kind {
                ObjectKind::Source {
                    interarrival,
                    labels,
                    schedule,
                    ..
                } => {
                    dists.push(("interarrival".into(), interarrival));
                    dists.extend(labels.iter().map(|(k, d)| (format!("labels.{k}"), d)));
                    if let Some(s) = schedule {
                        errors.extend(
                            s.validate()
                                .into_iter()
                                .map(|e| format!("{}: {e}", obj.name)),
                        );
                    }
                }
                ObjectKind::Queue {
                    capacity, batch, ..
                } => {
                    if *capacity == Some(0) {
                        errors.push(format!("{}: capacity deve ser >= 1", obj.name));
                    }
                    if let Some(b) = batch {
                        if b.size == 0 {
                            errors.push(format!("{}: batch.size deve ser >= 1", obj.name));
                        }
                        if !(b.max_wait.is_finite() && b.max_wait >= 0.0) {
                            errors.push(format!(
                                "{}: batch.max_wait deve ser finito e >= 0",
                                obj.name
                            ));
                        }
                        if let Some(cap) = capacity {
                            if *cap < b.size {
                                errors.push(format!(
                                    "{}: capacity ({cap}) menor que batch.size ({}): o lote nunca fecharia",
                                    obj.name, b.size
                                ));
                            }
                        }
                    }
                }
                ObjectKind::Combiner {
                    components,
                    setup,
                    process,
                    ..
                } => {
                    if components.is_empty() {
                        errors.push(format!(
                            "{}: components precisa de ao menos uma porta de componentes",
                            obj.name
                        ));
                    }
                    if components.contains(&0) {
                        errors.push(format!(
                            "{}: cada entrada de components deve ser >= 1",
                            obj.name
                        ));
                    }
                    let n_in = self
                        .connections
                        .iter()
                        .filter(|(_, to)| *to == obj.name)
                        .count();
                    if n_in != components.len() + 1 {
                        errors.push(format!(
                            "{}: Combiner precisa de {} entradas (1 contêiner + {} de componentes), mas tem {n_in}",
                            obj.name,
                            components.len() + 1,
                            components.len()
                        ));
                    }
                    if let Some(s) = setup {
                        dists.push(("setup".into(), s));
                    }
                    dists.push(("process".into(), process));
                }
                ObjectKind::Separator { mode, process } => {
                    if *mode == SeparateMode::Split(0) {
                        errors.push(format!("{}: Split(n) exige n >= 1", obj.name));
                    }
                    if matches!(mode, SeparateMode::Split(n) if *n > MAX_COUNT) {
                        errors.push(format!(
                            "{}: Split(n) aceita no máximo n = {MAX_COUNT}",
                            obj.name
                        ));
                    }
                    dists.push(("process".into(), process));
                }
                ObjectKind::Processor {
                    capacity,
                    setup,
                    process,
                    operators,
                } => {
                    if *capacity == 0 {
                        errors.push(format!("{}: capacity deve ser >= 1", obj.name));
                    }
                    if let Some(s) = setup {
                        dists.push(("setup".into(), s));
                    }
                    dists.push(("process".into(), process));
                    if let Some(op) = operators {
                        if op.count == 0 {
                            errors.push(format!("{}: operators.count deve ser >= 1", obj.name));
                        }
                        if op.count > MAX_COUNT {
                            errors.push(format!(
                                "{}: operators.count deve ser no máximo {MAX_COUNT}",
                                obj.name
                            ));
                        }
                    }
                }
                ObjectKind::Sink => {}
                ObjectKind::Transporter {
                    speed,
                    acceleration,
                    deceleration,
                    capacity,
                    load_time,
                    unload_time,
                    ..
                } => {
                    if !(speed.is_finite() && *speed > 0.0) {
                        errors.push(format!("{}: speed deve ser finita e > 0", obj.name));
                    }
                    for (what, v) in [
                        ("acceleration", acceleration),
                        ("deceleration", deceleration),
                    ] {
                        if v.is_some_and(|a| !(a.is_finite() && a > 0.0)) {
                            errors.push(format!("{}: {what} deve ser finita e > 0", obj.name));
                        }
                    }
                    if *capacity == 0 {
                        errors.push(format!("{}: capacity deve ser >= 1", obj.name));
                    }
                    dists.push(("load_time".into(), load_time));
                    dists.push(("unload_time".into(), unload_time));
                }
                ObjectKind::Conveyor {
                    length,
                    speed,
                    item_length,
                    ..
                } => {
                    let positive = |v: f64| v.is_finite() && v > 0.0;
                    if !(positive(*length) && positive(*speed) && positive(*item_length)) {
                        errors.push(format!(
                            "{}: length, speed e item_length devem ser finitos e > 0",
                            obj.name
                        ));
                    } else if item_length > length {
                        errors.push(format!(
                            "{}: item_length maior que length: nenhum item cabe na esteira",
                            obj.name
                        ));
                    }
                }
                ObjectKind::Rack {
                    bays,
                    levels,
                    dwell,
                    bay_width,
                    level_height,
                    ..
                } => {
                    if *bays == 0 || *levels == 0 {
                        errors.push(format!("{}: bays e levels devem ser >= 1", obj.name));
                    }
                    if bays.checked_mul(*levels).is_none_or(|n| n > MAX_COUNT) {
                        errors.push(format!(
                            "{}: bays x levels deve ser no máximo {MAX_COUNT} posições",
                            obj.name
                        ));
                    }
                    if !(bay_width.is_finite() && *bay_width > 0.0)
                        || !(level_height.is_finite() && *level_height > 0.0)
                    {
                        errors.push(format!(
                            "{}: bay_width e level_height devem ser > 0",
                            obj.name
                        ));
                    }
                    if let Some(d) = dwell {
                        dists.push(("dwell".into(), d));
                    }
                }
                ObjectKind::Dispatcher { team, .. } => {
                    if team.is_empty() {
                        errors.push(format!("{}: team não pode ser vazio", obj.name));
                    }
                }
            }
            if let Some(t) = &obj.transfer {
                if let Err(e) = t.validate() {
                    errors.push(format!("{}: {e}", obj.name));
                }
                if t.path == TransferPath::Network && self.network.is_none() {
                    errors.push(format!(
                        "{}: transfer.path = Network exige `network` no modelo",
                        obj.name
                    ));
                }
                if obj.transport.is_some() {
                    errors.push(format!(
                        "{}: use `transport` ou `transfer`, não os dois (o transporte já leva o tempo da viagem)",
                        obj.name
                    ));
                }
                if is_mobile(&obj.kind) {
                    errors.push(format!("{}: recursos móveis não usam transfer", obj.name));
                }
            }
            if let Some(dt) = &obj.downtime {
                dists.push(("downtime.mtbf".into(), &dt.mtbf));
                dists.push(("downtime.mttr".into(), &dt.mttr));
            }
            match &obj.shifts {
                Some(ShiftSpec::Inline(table)) => {
                    if let Err(e) = table.validate() {
                        errors.push(format!("{}: {e}", obj.name));
                    }
                }
                Some(ShiftSpec::Calendar { name, .. }) if !self.calendars.contains_key(name) => {
                    let known: Vec<&str> = self.calendars.keys().map(String::as_str).collect();
                    errors.push(format!(
                        "{}: o calendário '{name}' não existe ({})",
                        obj.name,
                        if known.is_empty() {
                            "o modelo não tem calendários".to_string()
                        } else {
                            format!("calendários: {}", known.join(", "))
                        }
                    ));
                }
                _ => {}
            }
            let mut dists_ok = true;
            for (what, d) in dists {
                if let Err(e) = d.validate() {
                    dists_ok = false;
                    errors.push(format!("{}.{what}: {e}", obj.name));
                }
            }
            // A source that would create billions of items (or loops at one instant when
            // the interval is zero) never finishes: refuse it up front.
            if let (
                true,
                ObjectKind::Source {
                    interarrival,
                    max_arrivals,
                    schedule,
                    ..
                },
            ) = (dists_ok, &obj.kind)
            {
                let mean = match schedule {
                    // Mean time between arrivals over a whole cycle.
                    Some(s) if s.validate().is_empty() => {
                        s.cycle / s.expected_per_cycle(interarrival)
                    }
                    Some(_) => f64::NAN,
                    None => interarrival.mean(),
                };
                let limit = max_arrivals.map_or(f64::INFINITY, |n| n as f64);
                if mean.is_finite() && run.stop_time.is_finite() {
                    let expected = (run.stop_time / mean).min(limit);
                    if expected > MAX_ARRIVALS {
                        errors.push(format!(
                            "{}: interarrival médio ({mean}) é pequeno demais para o tempo de simulação \
                             ({}): seriam mais de 1e9 chegadas",
                            obj.name, run.stop_time
                        ));
                    }
                }
            }
            if let OutputRule::ByLabel(label) = &obj.output {
                if label.trim().is_empty() {
                    errors.push(format!(
                        "{}: ByLabel precisa do nome de um rótulo",
                        obj.name
                    ));
                }
            }
        }

        let kind_of: HashMap<&str, &ObjectKind> = self
            .objects
            .iter()
            .map(|o| (o.name.as_str(), &o.kind))
            .collect();
        self.validate_mobile(&kind_of, &mut errors);
        self.validate_logic(&mut errors);
        let mut outputs: HashMap<&str, usize> = HashMap::new();
        let mut seen = HashSet::new();
        for (from, to) in &self.connections {
            if !seen.insert((from, to)) {
                errors.push(format!("conexão ({from} -> {to}) repetida"));
            }
            for end in [from, to] {
                if !kind_of.contains_key(end.as_str()) {
                    errors.push(format!(
                        "conexão ({from} -> {to}): objeto '{end}' não existe"
                    ));
                }
            }
            if from == to {
                errors.push(format!(
                    "conexão ({from} -> {to}): um objeto não pode ligar em si mesmo"
                ));
            }
            if matches!(kind_of.get(from.as_str()), Some(ObjectKind::Sink)) {
                errors.push(format!("conexão ({from} -> {to}): Sink não tem saída"));
            }
            if matches!(kind_of.get(to.as_str()), Some(ObjectKind::Source { .. })) {
                errors.push(format!("conexão ({from} -> {to}): Source não tem entrada"));
            }
            for end in [from, to] {
                if kind_of.get(end.as_str()).is_some_and(|k| is_mobile(k)) {
                    errors.push(format!(
                        "conexão ({from} -> {to}): '{end}' é um recurso móvel e não se liga por portas; \
                         use `transport` ou `operators`"
                    ));
                }
            }
            *outputs.entry(from.as_str()).or_default() += 1;
        }
        for obj in &self.objects {
            let n_out = outputs.get(obj.name.as_str()).copied().unwrap_or(0);
            if !matches!(obj.kind, ObjectKind::Sink) && !is_mobile(&obj.kind) && n_out == 0 {
                errors.push(format!(
                    "{}: não tem saída conectada (os itens ficariam presos)",
                    obj.name
                ));
            }
            if let OutputRule::Weighted(weights) = &obj.output {
                let valid = weights.iter().all(|w| w.is_finite() && *w >= 0.0)
                    && weights.iter().any(|w| *w > 0.0);
                if weights.len() != n_out || !valid {
                    errors.push(format!(
                        "{}: Weighted precisa de um peso >= 0 por porta de saída ({n_out}), com soma > 0",
                        obj.name
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ModelError::Invalid(errors))
        }
    }

    /// References between objects that do not go through ports.
    fn validate_mobile(&self, kind_of: &HashMap<&str, &ObjectKind>, errors: &mut Vec<String>) {
        let mobile = |name: &str| kind_of.get(name).is_some_and(|k| is_mobile(k));
        let mut in_team: HashMap<&str, &str> = HashMap::new();
        for obj in &self.objects {
            if let Some(t) = &obj.transport {
                if !mobile(t) {
                    errors.push(format!(
                        "{}: transport '{t}' precisa ser um Transporter ou Dispatcher",
                        obj.name
                    ));
                }
                if is_mobile(&obj.kind) {
                    errors.push(format!("{}: recursos móveis não usam transport", obj.name));
                }
            }
            if let ObjectKind::Processor {
                operators: Some(op),
                ..
            } = &obj.kind
            {
                if !mobile(&op.team) {
                    errors.push(format!(
                        "{}: operators.team '{}' precisa ser um Transporter ou Dispatcher",
                        obj.name, op.team
                    ));
                }
            }
            if let ObjectKind::Dispatcher { team, .. } = &obj.kind {
                for member in team {
                    if !matches!(
                        kind_of.get(member.as_str()),
                        Some(ObjectKind::Transporter { .. })
                    ) {
                        errors.push(format!(
                            "{}: membro '{member}' da equipe precisa ser um Transporter",
                            obj.name
                        ));
                    } else if let Some(other) = in_team.insert(member.as_str(), obj.name.as_str()) {
                        errors.push(format!(
                            "{member}: está nas equipes de '{other}' e de '{}'; use um só despachante",
                            obj.name
                        ));
                    }
                }
            }
        }
        if let Some(net) = &self.network {
            errors.extend(net.validate());
        }
    }

    /// Flows and object scripts (phase 6).
    fn validate_logic(&self, errors: &mut Vec<String>) {
        let objects: HashSet<&str> = self.objects.iter().map(|o| o.name.as_str()).collect();
        let mut flow_names = HashSet::new();
        for flow in &self.flows {
            if objects.contains(flow.name.as_str()) || !flow_names.insert(flow.name.as_str()) {
                errors.push(format!(
                    "fluxo '{}': o nome já é usado por outro objeto ou fluxo",
                    flow.name
                ));
            }
            errors.extend(flow.validate(&objects));
        }
        if let Some(fluids) = &self.fluids {
            if objects.contains(fluids.name.as_str()) || flow_names.contains(fluids.name.as_str()) {
                errors.push(format!(
                    "rede de fluidos '{}': o nome já é usado por outro objeto ou fluxo",
                    fluids.name
                ));
            }
            errors.extend(fluids.validate(&objects));
        }
        for obj in &self.objects {
            if let Some(src) = &obj.on_entry {
                if let Err(e) = utsim_script::compile_item_script(src) {
                    errors.push(format!("{}: on_entry: {e}", obj.name));
                }
            }
            if let OutputRule::Script(src) = &obj.output {
                if let Err(e) = utsim_script::compile_port_script(src) {
                    errors.push(format!("{}: regra de saída Script: {e}", obj.name));
                }
            }
        }
    }

    /// Builds one replication. Call [`Model::validate`] first.
    pub fn build(&self, replication: u32) -> Simulation {
        let mut sim = Simulation::new(RunConfig {
            stop_time: self.run.stop_time,
            warmup: self.run.warmup,
            seed: self.run.seed,
            replication,
            sample_interval: self.run.sample_interval,
            histogram_bins: self.run.histogram_bins,
        });
        // Block ids follow object order, so references can be resolved up front.
        let ids: HashMap<&str, BlockId> = self
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| (o.name.as_str(), BlockId(i)))
            .collect();
        // One router and one traffic state per simulation (AGV edge control).
        let net_router = utsim_objects::network::network_router(self.network.as_ref());
        let traffic = match (&self.network, &net_router) {
            (Some(def), Some(r)) => Some(Arc::new(Mutex::new(Traffic::new(def, r.clone())))),
            _ => None,
        };
        let dispatcher_of: HashMap<&str, BlockId> = self
            .objects
            .iter()
            .filter_map(|o| match &o.kind {
                ObjectKind::Dispatcher { team, .. } => Some((team, ids[o.name.as_str()])),
                _ => None,
            })
            .flat_map(|(team, d)| team.iter().map(move |m| (m.as_str(), d)))
            .collect();
        for obj in &self.objects {
            let behavior: Box<dyn Behavior> = match &obj.kind {
                ObjectKind::Source {
                    interarrival,
                    first_at_zero,
                    max_arrivals,
                    labels,
                    on_blocked,
                    schedule,
                } => {
                    let source = Source::new(interarrival.clone())
                        .on_blocked(*on_blocked)
                        .first_at_zero(*first_at_zero)
                        .max_arrivals(*max_arrivals)
                        .schedule(schedule.clone());
                    Box::new(
                        labels
                            .iter()
                            .fold(source, |s, (k, d)| s.label(k.clone(), d.clone())),
                    )
                }
                ObjectKind::Queue {
                    capacity,
                    discipline,
                    batch,
                } => Box::new(Queue::new(*capacity).discipline(*discipline).batch(*batch)),
                ObjectKind::Combiner {
                    components,
                    mode,
                    setup,
                    process,
                } => Box::new(
                    Combiner::new(components.clone(), *mode, process.clone()).setup(setup.clone()),
                ),
                ObjectKind::Separator { mode, process } => {
                    Box::new(Separator::new(*mode, process.clone()))
                }
                ObjectKind::Processor {
                    capacity,
                    setup,
                    process,
                    operators,
                } => Box::new(
                    Processor::new(process.clone())
                        .capacity(*capacity)
                        .setup(setup.clone())
                        .operators(
                            operators
                                .as_ref()
                                .map(|op| (ids[op.team.as_str()], op.clone())),
                        ),
                ),
                ObjectKind::Sink => Box::new(Sink),
                ObjectKind::Transporter {
                    speed,
                    acceleration,
                    deceleration,
                    capacity,
                    load_time,
                    unload_time,
                    home,
                    agv,
                } => Box::new(
                    Transporter::new(*speed)
                        .agv(if *agv { traffic.clone() } else { None })
                        .acceleration(*acceleration, *deceleration)
                        .capacity(*capacity)
                        .home(*home)
                        .handling(load_time.clone(), unload_time.clone())
                        .dispatcher(dispatcher_of.get(obj.name.as_str()).copied()),
                ),
                ObjectKind::Conveyor {
                    length,
                    speed,
                    item_length,
                    accumulating,
                } => Box::new(Conveyor::new(*length, *speed, *item_length, *accumulating)),
                ObjectKind::Rack {
                    bays,
                    levels,
                    dwell,
                    placement,
                    ..
                } => Box::new(Rack::new(*bays, *levels, dwell.clone(), *placement)),
                ObjectKind::Dispatcher { team, rule } => Box::new(Dispatcher::new(
                    team.iter().map(|m| ids[m.as_str()]).collect(),
                    *rule,
                )),
            };
            let id = sim.add_block(obj.name.clone(), obj.output.clone(), behavior);
            sim.set_position(id, obj.position);
            if let Some(t) = &obj.transport {
                sim.set_transport(id, ids[t.as_str()]);
            }
            if let Some(script) = obj
                .on_entry
                .as_deref()
                .and_then(|s| utsim_script::compile_item_script(s).ok())
            {
                sim.set_entry_script(id, script);
            }
            if let OutputRule::Script(src) = &obj.output {
                if let Ok(script) = utsim_script::compile_port_script(src) {
                    sim.set_port_script(id, script);
                }
            }
            if let Some(dt) = &obj.downtime {
                sim.set_downtime(id, dt.clone());
            }
            if let Some(table) = self.shift_table(obj) {
                sim.set_shifts(id, table);
            }
            if let Some(transfer) = self.effective_transfer(obj) {
                sim.set_transfer(id, transfer);
            }
        }
        for (from, to) in &self.connections {
            sim.connect(ids[from.as_str()], ids[to.as_str()]);
        }
        // Flows run as extra blocks after the objects (their ids follow object order).
        let objects: HashMap<String, BlockId> =
            ids.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        for flow in &self.flows {
            let behavior = Box::new(Flow::new(flow.clone(), objects.clone()));
            sim.add_block(flow.name.clone(), OutputRule::FirstAvailable, behavior);
        }
        // The fluid network is the last block (after objects and flows).
        if let Some(fluids) = &self.fluids {
            let behavior = Box::new(FluidNet::new(fluids.clone(), objects.clone()));
            sim.add_block(fluids.name.clone(), OutputRule::FirstAvailable, behavior);
        }
        match net_router {
            Some(r) => sim.set_router(r),
            None => sim.set_router(utsim_objects::network::router(None)),
        }
        sim
    }

    /// Runs `run.replications` independent replications (in parallel with the `parallel` feature).
    /// Without the `parallel` feature (WebAssembly) they run one after another,
    /// with identical results and order.
    pub fn run_replications(&self) -> Result<Vec<RunReport>, ModelError> {
        self.run_replications_with(&|_| true)
    }

    /// Like [`Model::run_replications`], calling `progress(done)` after each finished
    /// replication (`done` counts the finished ones, in any order). When it returns
    /// `false`, no new replication starts and the run ends with
    /// [`ModelError::Cancelled`]. Results are identical to `run_replications`.
    pub fn run_replications_with(
        &self,
        progress: &(dyn Fn(u32) -> bool + Sync),
    ) -> Result<Vec<RunReport>, ModelError> {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        self.validate()?;
        let done = AtomicU32::new(0);
        let stop = AtomicBool::new(false);
        let one = |r: u32| -> Result<RunReport, ModelError> {
            if stop.load(Ordering::Relaxed) {
                return Err(ModelError::Cancelled);
            }
            let report = self.drive(&mut self.build(r))?;
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if !progress(n) {
                stop.store(true, Ordering::Relaxed);
            }
            Ok(report)
        };
        #[cfg(feature = "parallel")]
        let iter = (0..self.run.replications).into_par_iter();
        #[cfg(not(feature = "parallel"))]
        let iter = 0..self.run.replications;
        let reports: Vec<RunReport> = iter.map(one).collect::<Result<_, _>>()?;
        // A stop asked for while the last replications were already running still
        // counts: the caller asked not to wait for this result.
        if stop.load(Ordering::Relaxed) {
            return Err(ModelError::Cancelled);
        }
        Ok(reports)
    }

    /// Runs a built simulation to the end, answering agent decisions with
    /// `run.default_decision` when it is set.
    pub fn drive(&self, sim: &mut Simulation) -> Result<RunReport, ModelError> {
        let Some(choice) = self.run.default_decision else {
            return Ok(sim.run()?);
        };
        loop {
            match sim.run_until(f64::INFINITY)? {
                utsim_core::Step::Decision(d) => {
                    sim.decide(choice.min(d.options.saturating_sub(1)))?
                }
                utsim_core::Step::Paused(_) => {}
                utsim_core::Step::Finished => return Ok(sim.finish()),
            }
        }
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;

    const MINIMAL: &str = "Model(name: \"m\", run: (stop_time: 1.0), objects: [])";

    fn fake_v0_to_v1(text: &str) -> Result<String, ModelError> {
        // Fake rule: v0 called the model name `titulo`.
        Ok(text
            .replace("titulo:", "name:")
            .replace("format: 0", "format: 1"))
    }

    #[test]
    fn migrate_runs_steps() {
        let v0 = "Model(format: 0, titulo: \"m\", run: (stop_time: 1.0), objects: [])";
        assert!(Model::parse(v0).is_err());
        let steps: &[(u32, Migration)] = &[(0, fake_v0_to_v1)];
        let moved = migrate(v0, 0, steps).unwrap();
        assert_eq!(Model::parse(&moved).unwrap().name, "m");
        // Already at version 1: the v0 step is skipped.
        assert_eq!(migrate(v0, 1, steps).unwrap(), v0);
    }

    #[test]
    fn absent_format_is_version_1() {
        assert_eq!(Model::parse(MINIMAL).unwrap().format, None);
    }

    #[test]
    fn newer_format_is_rejected() {
        let text = MINIMAL.replace("Model(", "Model(format: 99, ");
        let err = Model::parse(&text).unwrap_err().to_string();
        assert_eq!(
            err,
            format!("modelo no formato 99; esta versão do UTSim lê até {FORMAT_VERSION}")
        );
    }
}
