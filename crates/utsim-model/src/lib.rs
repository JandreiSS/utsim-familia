//! Model description: what is written in a `.ron` file and how it becomes a [`Simulation`].

pub mod experiment;
pub mod replay;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use utsim_core::{
    Behavior, BlockId, Dist, Downtime, OutputRule, RunConfig, RunReport, ShiftTable, SimError,
    Simulation,
};
use utsim_objects::network::NetworkDef;
use utsim_objects::{
    Batch, CombineMode, Combiner, Conveyor, Discipline, DispatchRule, Dispatcher, OnBlocked,
    OperatorUse, Processor, Queue, Rack, RackPlacement, SeparateMode, Separator, Sink, Source,
    Transporter,
};

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("não foi possível ler o modelo: {0}")]
    Io(#[from] std::io::Error),
    #[error("erro de sintaxe no modelo: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("modelo inválido:\n  - {}", .0.join("\n  - "))]
    Invalid(Vec<String>),
    #[error(transparent)]
    Sim(#[from] SimError),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub run: RunSettings,
    pub objects: Vec<ObjectDef>,
    /// `(from, to)` pairs. Order defines port numbers and pull priority.
    #[serde(default)]
    pub connections: Vec<(String, String)>,
    /// Paths for mobile resources (straight lines when absent).
    #[serde(default)]
    pub network: Option<NetworkDef>,
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
    /// Scheduled off-shift windows, repeating every `period`.
    #[serde(default)]
    pub shifts: Option<ShiftTable>,
    /// Transporter or dispatcher that carries items out of this object
    /// (instead of handing them over instantly).
    #[serde(default)]
    pub transport: Option<String>,
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
        Ok(options.from_str(text)?)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Model, ModelError> {
        Model::parse(&std::fs::read_to_string(path)?)
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let mut errors = Vec::new();
        let run = &self.run;
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

        if self.objects.is_empty() {
            errors.push("o modelo está vazio: adicione objetos".to_string());
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
                    ..
                } => {
                    dists.push(("interarrival".into(), interarrival));
                    dists.extend(labels.iter().map(|(k, d)| (format!("labels.{k}"), d)));
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
            if let Some(dt) = &obj.downtime {
                dists.push(("downtime.mtbf".into(), &dt.mtbf));
                dists.push(("downtime.mttr".into(), &dt.mttr));
            }
            if let Some(Err(e)) = obj.shifts.as_ref().map(ShiftTable::validate) {
                errors.push(format!("{}: {e}", obj.name));
            }
            for (what, d) in dists {
                if let Err(e) = d.validate() {
                    errors.push(format!("{}.{what}: {e}", obj.name));
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
                } => {
                    let source = Source::new(interarrival.clone())
                        .on_blocked(*on_blocked)
                        .first_at_zero(*first_at_zero)
                        .max_arrivals(*max_arrivals);
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
                    ..
                } => Box::new(
                    Transporter::new(*speed)
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
            if let Some(dt) = &obj.downtime {
                sim.set_downtime(id, dt.clone());
            }
            if let Some(table) = &obj.shifts {
                sim.set_shifts(id, table.clone());
            }
        }
        for (from, to) in &self.connections {
            sim.connect(ids[from.as_str()], ids[to.as_str()]);
        }
        sim.set_router(utsim_objects::network::router(self.network.as_ref()));
        sim
    }

    /// Runs `run.replications` independent replications in parallel.
    pub fn run_replications(&self) -> Result<Vec<RunReport>, ModelError> {
        self.validate()?;
        (0..self.run.replications)
            .into_par_iter()
            .map(|r| self.build(r).run().map_err(ModelError::from))
            .collect()
    }
}
