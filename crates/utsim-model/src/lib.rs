//! Model description: what is written in a `.ron` file and how it becomes a [`Simulation`].

mod aisles;
pub mod analysis;
mod arrivals;
pub mod csv;
pub mod dados;
pub mod experiment;
mod format2;
pub mod html;
mod items;
mod line;
mod maintenance;
mod movement;
mod multitask;
mod person;
pub mod quality;
pub mod replay;
mod shifts;
mod spans;
pub mod state;
pub mod synthetic;
pub mod teams;
mod warnings;

pub use line::SYMBOL_IDS;
pub use maintenance::{DowntimeDef, Every, OeeDef, PreventiveDef};
pub use movement::{Movement, NavSet};
pub use quality::{
    CharacteristicDef, DefectDef, DefectFactor, InspectionDef, MeasureDef, QualityDef, RejectTo,
    ReworkDef, SinkRole, ZoneDef, ZoneKind,
};
pub use spans::SpanDef;
pub use state::InitialState;
pub use utsim_cad::FloorPlanDef;
pub use utsim_nav::Obstacle;
pub use utsim_nav::{Aisle, AisleKind, FloorMarking, MarkShape, Pattern, Profile, ProfileSpec};

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(feature = "parallel")]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use utsim_core::{
    Behavior, BlockId, Dist, OutputRule, RunConfig, RunReport, ShiftTable, SimError, Simulation,
    TransferDef, TransferPath,
};
use utsim_flow::{Flow, FlowDef};
use utsim_fluid::{FluidDef, FluidNet};
use utsim_objects::network::NetworkDef;
use utsim_objects::traffic::Traffic;
use utsim_objects::{
    Batch, CombineMode, Combiner, Conveyor, Discipline, DispatchRule, Dispatcher, LabelOrder,
    Multitask, OnBlocked, OperatorUse, Preempt, Processor, Queue, Rack, RackPlacement,
    SeparateMode, Separator, Sink, Source, TaskPick, Transporter, TypedSink,
};

pub use arrivals::{AreaDef, ItemTypeDef};
pub use shifts::ShiftSpec;
pub use utsim_core::ShiftEnd;
pub use utsim_objects::{
    ArrivalPeriod, ArrivalRow, ArrivalSchedule, ArrivalTable, PeriodArrivals, Release, TableMode,
    TableRepeat,
};

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
/// To add format N + 1: bump [`FORMAT_VERSION`], push `(N, vN_to_vN1)` here (the step
/// rewrites the text and sets `format: N + 1`), keep an old-format example in the tests,
/// and document it.
///
/// - `1 → 2`: `run.movement: Legacy` written explicitly (the format-1 behaviour); in format
///   2 an absent `movement` means `Free` ([`Movement`]).
const MIGRATIONS: &[(u32, Migration)] = &[(1, format2::v1_to_v2)];

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
pub const FORMAT_VERSION: u32 = 2;

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
    /// Named floor areas (waiting areas, break rooms, office zones): markers outside the
    /// flow that transporters use as home or break place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<AreaDef>,
    /// Item types (name, colour, 3D look, default labels). An item of type `k` (1-based,
    /// in this order) carries the reserved label `type = k`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_types: Vec<ItemTypeDef>,
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
    /// Time between two points of the flow, one block per measurement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<SpanDef>,
    /// Distribution fields fed by observed values, fitted on load ([`dados`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<dados::DataInput>,
    /// CAD floor plan the layout was drawn on (record only: the simulation never reads the
    /// file; its walls and columns go to `obstacles`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_plan: Option<Box<FloorPlanDef>>,
    /// Polygons no mobile resource crosses in `Free` and `Network` movement, besides the
    /// areas of the objects: walls and columns of the plan or drawn by hand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obstacles: Vec<Obstacle>,
    /// Circulation aisles (pedestrian, forklift, mixed) drawn on the floor: the `Aisles`
    /// movement uses the network they generate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aisles: Vec<Aisle>,
    /// Floor marking profile (absent: Brasil, 10 cm line, 2 s stop before crossings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_marking: Option<Box<FloorMarking>>,
    /// Specialised line the model belongs to (`"lean_office"`): the interface and the
    /// reports use its vocabulary, library and indicators. Never read by the simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// Cross-cutting packs added to the line (`"qualidade"`, `"seguranca"`): more presets
    /// and reports in the interface. Never read by the simulation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<String>,
    /// Risk zones on the floor ([`ZoneDef`]): exposure analysis and 3D. Never read by the
    /// simulation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<ZoneDef>,
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
    /// How mobile resources move unless they set their own (format 2: absent = `Free`;
    /// models migrated from format 1 carry `Legacy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement: Option<Movement>,
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
    /// Random breakdowns (MTBF/MTTR on the clock) and, optionally, maintenance (repair
    /// crew, wear, preventive plans, costs, OEE).
    #[serde(default)]
    pub downtime: Option<DowntimeDef>,
    /// Scheduled off-shift windows, repeating every `period`: inline or one of the
    /// model's `calendars`.
    #[serde(default)]
    pub shifts: Option<ShiftSpec>,
    /// Transporter or dispatcher that carries items out of this object
    /// (instead of handing them over instantly).
    #[serde(default)]
    pub transport: Option<String>,
    /// Priority of this object's transport requests at the dispatcher (higher goes first,
    /// FIFO among equals). Absent: 0, as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_priority: Option<i32>,
    /// Item label whose value, when the item has it, replaces `transport_priority`
    /// (e.g. an urgent order created with `"prioridade": 9` goes ahead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_priority_label: Option<String>,
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
    /// Where mobile resources stop to serve this object in `Free` and `Network` movement,
    /// in the object's own coordinates (metres, turned with it). Absent: on its `-y` side,
    /// clear of its area by the resource's clearance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<[f64; 2]>,
    /// Transporter used as a person: fatigue, learning curve, skills, walking speed with
    /// a load, individual pace and manual lifting check ([`utsim_core::person`]). Absent:
    /// none of these effects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<Box<utsim_core::Person>>,
    /// Library preset the object was created from, `"<line>/<preset>"` (for example
    /// `"lean_office/aprovacao"`): the domain name, icon and role in the line's reports.
    /// Never read by the simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Department (sector, area of the organisation) the object belongs to: lanes of the
    /// swimlane chart and reports by department. Not a floor area of `Model::areas`.
    /// Never read by the simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    /// Flowchart symbol chosen for the object (one of [`SYMBOL_IDS`]); absent: the one
    /// the flowchart infers from its kind. Never read by the simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Defects and measured characteristics drawn for the items entering the object
    /// (`Processor`, `Combiner`, `Separator`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<Box<QualityDef>>,
    /// Inspection station: writes the output port of each item (`ByLabel("inspecao")`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inspection: Option<Box<InspectionDef>>,
    /// Rework station: counts passes and fixes defects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rework: Option<ReworkDef>,
    /// Role of a `Sink` in the quality reports (scrap or customer). Never read by the
    /// simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sink_role: Option<SinkRole>,
    /// Protective equipment required at this work station (names of
    /// [`quality::PPE_NAMES`] or others). Never read by the simulation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ppe: Vec<String>,
}

impl ObjectDef {
    /// Whether the object has any of the blocks that install the quality hook.
    pub fn has_quality_hook(&self) -> bool {
        self.quality.is_some() || self.inspection.is_some() || self.rework.is_some()
    }
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
        /// Arrival table: fixed instants (`Times`) or an ordered sequence (`Sequence`),
        /// each row with a count and labels. Not together with `schedule`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        table: Option<ArrivalTable>,
        /// Items created per arrival (rounded, at least 0) without a table; absent: one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        batch: Option<Dist>,
        /// How the items of one arrival (batch or table row) leave: `Together` or
        /// `OneByOne(Dist)`.
        #[serde(default, skip_serializing_if = "Release::is_together")]
        release: Release,
        /// Item type of every item (a name of the model's `item_types`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item_type: Option<String>,
        /// Item types drawn for each item, `(name, weight)`; not with `item_type`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        item_type_mix: Vec<(String, f64)>,
    },
    Queue {
        #[serde(default)]
        capacity: Option<usize>,
        #[serde(default)]
        discipline: Discipline,
        /// Holds items and releases them in groups.
        #[serde(default)]
        batch: Option<Batch>,
        /// Order by a numeric label (`Desc("prioridade")`, `Asc("prazo")`); wins over
        /// `discipline`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        order: Option<LabelOrder>,
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
        /// Own movement mode (absent: `run.movement`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        movement: Option<Movement>,
        /// Distance (m) kept from every obstacle in `Free` and `Network` movement. Absent:
        /// half the width of its role (person 0.3, forklift 0.6, AGV or vehicle 0.5) + 0.3.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clearance: Option<f64>,
        /// Home at this point instead of the object's own position (implies `home`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        home_at: Option<[f64; 3]>,
        /// Home at this object or area (its position; implies `home`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        home_object: Option<String>,
        /// Break place: point the member walks to when its shift pauses with nothing to do.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        break_at: Option<[f64; 3]>,
        /// Break place at an object or area (its position).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        break_object: Option<String>,
        /// A person that works on several cases at once: `max_open` operator jobs open
        /// together, worked in turn for `slice` each, losing `switch` at every turn
        /// (absent: one job at a time).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        multitask: Option<Multitask>,
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
        /// Which waiting task a member that becomes free takes (absent: `Priority`).
        #[serde(default, skip_serializing_if = "TaskPick::is_priority")]
        pick: TaskPick,
        /// Whether a more urgent request takes a member travelling empty (absent: `Never`).
        #[serde(default, skip_serializing_if = "Preempt::is_never")]
        preempt: Preempt,
        /// Team metrics in the report (`request_wait`, `requests.count`, per member
        /// `member.<name>.requests` and `.busy`). Off by default.
        #[serde(default, skip_serializing_if = "is_false")]
        metrics: bool,
    },
}

fn yes() -> bool {
    true
}

fn is_false(v: &bool) -> bool {
    !*v
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
        Model::parse_in(text, None)
    }

    /// Like [`Model::parse`], with data files (`data: [..]`) taken relative to `base`.
    pub fn parse_in(text: &str, base: Option<&Path>) -> Result<Model, ModelError> {
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
        let mut model: Model = options.from_str(&text)?;
        dados::resolve(&mut model, base)?;
        Ok(model)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Model, ModelError> {
        let path = path.as_ref();
        Model::parse_in(&std::fs::read_to_string(path)?, path.parent())
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
                    table,
                    batch,
                    release,
                    ..
                } => {
                    errors.extend(arrivals::validate_source(
                        &obj.name,
                        interarrival,
                        schedule,
                        table,
                    ));
                    if batch.is_some() && table.is_some() {
                        errors.push(format!(
                            "{}: com `table` a quantidade de cada chegada vem de rows[].count; tire `batch`",
                            obj.name
                        ));
                    }
                    if let Some(b) = batch {
                        dists.push(("batch".into(), b));
                    }
                    if let Release::OneByOne(d) = release {
                        dists.push(("release".into(), d));
                    }
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
                    capacity,
                    batch,
                    order,
                    discipline,
                } => {
                    if let Some(o) = order {
                        if o.label().trim().is_empty() {
                            errors
                                .push(format!("{}: order precisa do nome de um rótulo", obj.name));
                        }
                        if *discipline == Discipline::Lifo {
                            errors.push(format!(
                                "{}: use discipline: Lifo ou order, não os dois",
                                obj.name
                            ));
                        }
                    }
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
                    multitask,
                    ..
                } => {
                    if !(speed.is_finite() && *speed > 0.0) {
                        errors.push(format!("{}: speed deve ser finita e > 0", obj.name));
                    }
                    if let Some(m) = multitask {
                        if m.max_open == 0 {
                            errors.push(format!("{}: multitask.max_open deve ser >= 1", obj.name));
                        }
                        if matches!(m.slice, Dist::Const(v) if v.is_nan() || v <= 0.0) {
                            errors.push(format!(
                                "{}: multitask.slice deve ser > 0 (a pessoa trocaria de caso sem trabalhar)",
                                obj.name
                            ));
                        }
                        dists.push(("multitask.slice".into(), &m.slice));
                        dists.push(("multitask.switch".into(), &m.switch));
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
                    table,
                    batch,
                    ..
                },
            ) = (dists_ok, &obj.kind)
            {
                let per_arrival = batch.as_ref().map_or(1.0, |b| b.mean().max(0.0));
                let mean = match (table, schedule) {
                    (Some(t), _) => arrivals::mean_gap(t, interarrival),
                    // Mean time between arrivals over a whole cycle.
                    (None, Some(s)) if s.validate().is_empty() => {
                        s.cycle / s.expected_per_cycle(interarrival)
                    }
                    (None, Some(_)) => f64::NAN,
                    (None, None) => interarrival.mean(),
                };
                // Items, not arrivals: a batch multiplies them.
                let mean = if table.is_none() {
                    mean / per_arrival
                } else {
                    mean
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
        self.validate_item_types(&mut errors);
        self.validate_places(&mut errors);
        self.validate_mobile(&kind_of, &mut errors);
        self.validate_movement(&mut errors);
        self.validate_aisles(&mut errors);
        self.validate_logic(&mut errors);
        self.validate_quality(&mut errors);
        self.validate_spans(&mut errors);
        self.validate_maintenance(&mut errors);
        self.validate_people(&mut errors);
        self.validate_line(&mut errors);
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
            if obj.transport.is_none()
                && (obj.transport_priority.is_some() || obj.transport_priority_label.is_some())
            {
                errors.push(format!(
                    "{}: transport_priority e transport_priority_label só valem com `transport`",
                    obj.name
                ));
            }
            if obj
                .transport_priority_label
                .as_deref()
                .is_some_and(|l| l.trim().is_empty())
            {
                errors.push(format!(
                    "{}: transport_priority_label precisa do nome de um rótulo",
                    obj.name
                ));
            }
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
                if op
                    .priority_label
                    .as_deref()
                    .is_some_and(|l| l.trim().is_empty())
                {
                    errors.push(format!(
                        "{}: operators.priority_label precisa do nome de um rótulo",
                        obj.name
                    ));
                }
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

    /// Quality blocks, sink roles and risk zones (`docs/formato_qualidade_seguranca.md`).
    fn validate_quality(&self, errors: &mut Vec<String>) {
        for obj in &self.objects {
            let outputs = self
                .connections
                .iter()
                .filter(|(from, _)| *from == obj.name)
                .count();
            if obj.has_quality_hook()
                && !matches!(
                    obj.kind,
                    ObjectKind::Processor { .. }
                        | ObjectKind::Combiner { .. }
                        | ObjectKind::Separator { .. }
                        | ObjectKind::Queue { .. }
                )
            {
                errors.push(format!(
                    "{}: quality, inspection e rework só valem em Processor, Combiner, Separator ou Queue",
                    obj.name
                ));
            }
            if let Some(i) = &obj.inspection {
                if obj.output != OutputRule::ByLabel(i.label.clone()) {
                    errors.push(format!(
                        "{}: a inspeção pede output: ByLabel(\"{}\")",
                        obj.name, i.label
                    ));
                }
            }
            quality::validate(
                &obj.name,
                obj.quality.as_deref(),
                obj.inspection.as_deref(),
                obj.rework.as_ref(),
                outputs,
                errors,
            );
            if obj.sink_role.is_some() && !matches!(obj.kind, ObjectKind::Sink) {
                errors.push(format!("{}: sink_role só vale em Sink", obj.name));
            }
        }
        let mut names = HashSet::new();
        for z in &self.zones {
            if !names.insert(z.name.as_str()) {
                errors.push(format!("zona '{}' repetida", z.name));
            }
            if z.polygon.len() < 3 || z.polygon.iter().flatten().any(|v| !v.is_finite()) {
                errors.push(format!(
                    "zona '{}': o polígono precisa de 3 vértices finitos ou mais",
                    z.name
                ));
            }
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
        self.build_with(replication, &self.nav_set())
    }

    /// Like [`Model::build`], with the routers of the movement modes already made
    /// ([`Model::nav_set`]), so several replications share them (and their caches).
    pub fn build_with(&self, replication: u32, nav: &NavSet) -> Simulation {
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
        // Pedestrian × vehicle crossings (movement `Aisles`), one board per simulation.
        let crossings = nav.crossing_board();
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
                    table,
                    batch,
                    release,
                    item_type,
                    item_type_mix,
                } => {
                    let source = Source::new(interarrival.clone())
                        .batch(batch.clone())
                        .release(release.clone())
                        .on_blocked(*on_blocked)
                        .first_at_zero(*first_at_zero)
                        .max_arrivals(*max_arrivals)
                        .schedule(schedule.clone())
                        .table(table.clone());
                    let source = if self.item_types.is_empty() {
                        source
                    } else {
                        source.item_types(
                            self.item_type_specs(),
                            item_type.as_deref(),
                            item_type_mix,
                        )
                    };
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
                    order,
                } => Box::new(
                    Queue::new(*capacity)
                        .discipline(*discipline)
                        .batch(*batch)
                        .order(order.clone()),
                ),
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
                ObjectKind::Sink if !self.item_types.is_empty() => Box::new(TypedSink::new(
                    self.item_types.iter().map(|t| t.name.clone()).collect(),
                )),
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
                    home_at,
                    home_object,
                    break_at,
                    break_object,
                    multitask,
                    ..
                } => Box::new(
                    Transporter::new(*speed)
                        .multitask(multitask.clone())
                        .home_point(self.place_of(*home_at, home_object.as_deref()))
                        .break_point(self.place_of(*break_at, break_object.as_deref()))
                        .agv(if *agv { traffic.clone() } else { None })
                        .router(nav.router(&obj.name))
                        .crossings(nav.crossings_for(&obj.name, &crossings))
                        .acceleration(*acceleration, *deceleration)
                        .capacity(*capacity)
                        .home(*home || home_at.is_some() || home_object.is_some())
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
                ObjectKind::Dispatcher {
                    team,
                    rule,
                    pick,
                    preempt,
                    metrics,
                } => Box::new(
                    Dispatcher::new(team.iter().map(|m| ids[m.as_str()]).collect(), *rule)
                        .open_limits(team.iter().map(|m| self.open_limit(m)).collect())
                        .pick(*pick)
                        .preempt(*preempt)
                        .metrics(*metrics),
                ),
            };
            let id = sim.add_block(obj.name.clone(), obj.output.clone(), behavior);
            sim.set_position(id, obj.position);
            if let Some(t) = &obj.transport {
                sim.set_transport(id, ids[t.as_str()]);
                if obj.transport_priority.is_some() || obj.transport_priority_label.is_some() {
                    sim.set_transport_priority(
                        id,
                        obj.transport_priority.unwrap_or(0),
                        obj.transport_priority_label.clone(),
                    );
                }
            }
            let script = obj
                .on_entry
                .as_deref()
                .and_then(|s| utsim_script::compile_item_script(s).ok());
            if obj.has_quality_hook() {
                sim.set_entry_script(
                    id,
                    Box::new(quality::QualityHook::new(
                        self.run.seed,
                        replication,
                        id.0,
                        obj.quality.as_deref().cloned(),
                        obj.inspection.as_deref().cloned(),
                        obj.rework.clone(),
                        script,
                    )),
                );
            } else if let Some(script) = script {
                sim.set_entry_script(id, script);
            }
            if let OutputRule::Script(src) = &obj.output {
                if let Ok(script) = utsim_script::compile_port_script(src) {
                    sim.set_port_script(id, script);
                }
            }
            if let Some(dt) = &obj.downtime {
                sim.set_downtime(id, dt.downtime());
            }
            if let Some(m) = self.maintenance_of(obj, &ids) {
                sim.set_maintenance(id, m);
            }
            if let Some(p) = &obj.person {
                sim.set_person(id, (**p).clone());
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
        self.add_spans(&mut sim, &objects);
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
        self.run_replications_prepared(progress, &|_| {})
    }

    /// Like [`Model::run_replications`], keeping the instant of every observation of
    /// the given `(object, measure)` tallies (see [`Simulation::keep_timed`]): the
    /// reports carry them in `TallySummary::observations`. The statistics are the same
    /// as without it. Used by the warm-up analysis.
    pub fn run_replications_timed(
        &self,
        measures: &[(String, String)],
    ) -> Result<Vec<RunReport>, ModelError> {
        self.run_replications_prepared(&|_| true, &|sim: &mut Simulation| {
            for (object, measure) in measures {
                if let Some(id) = sim.block_id(object) {
                    sim.keep_timed(id, measure);
                }
            }
        })
    }

    fn run_replications_prepared(
        &self,
        progress: &(dyn Fn(u32) -> bool + Sync),
        prepare: &(dyn Fn(&mut Simulation) + Sync),
    ) -> Result<Vec<RunReport>, ModelError> {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        self.validate()?;
        let done = AtomicU32::new(0);
        let stop = AtomicBool::new(false);
        let nav = self.nav_set();
        let one = |r: u32| -> Result<RunReport, ModelError> {
            if stop.load(Ordering::Relaxed) {
                return Err(ModelError::Cancelled);
            }
            let mut sim = self.build_with(r, &nav);
            prepare(&mut sim);
            let report = self.drive(&mut sim)?;
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
    fn absent_format_is_version_1_migrated_to_format_2() {
        let m = Model::parse(MINIMAL).unwrap();
        assert_eq!(m.format, Some(2));
        // No mobile resources: nothing to keep.
        assert_eq!(m.run.movement, None);
    }

    #[test]
    fn format_2_without_movement_is_free() {
        let text = MINIMAL.replace("Model(", "Model(format: 2, ");
        let m = Model::parse(&text).unwrap();
        assert_eq!(m.run.movement, None);
        let t = "Model(format: 2, name: \"m\", run: (stop_time: 1.0), objects: [\
                 (name: \"p\", kind: Transporter(speed: 1.0))])";
        let m = Model::parse(t).unwrap();
        assert_eq!(m.movement_of(&m.objects[0]), Some(Movement::Free));
        let m = Model::parse(&t.replace("format: 2, ", "")).unwrap();
        assert_eq!(m.movement_of(&m.objects[0]), Some(Movement::Legacy));
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
