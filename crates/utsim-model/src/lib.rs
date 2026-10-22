//! Model description: what is written in a `.ron` file and how it becomes a [`Simulation`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Dist, Downtime, OutputRule, RunConfig, RunReport, SimError, Simulation};
use utsim_objects::{Discipline, Processor, Queue, Sink, Source};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub run: RunSettings,
    pub objects: Vec<ObjectDef>,
    /// `(from, to)` pairs. Order defines port numbers and pull priority.
    #[serde(default)]
    pub connections: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSettings {
    pub stop_time: f64,
    #[serde(default)]
    pub warmup: f64,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default = "one_u32")]
    pub replications: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ObjectKind {
    Source {
        interarrival: Dist,
        #[serde(default)]
        first_at_zero: bool,
        #[serde(default)]
        max_arrivals: Option<u64>,
        #[serde(default)]
        labels: BTreeMap<String, Dist>,
    },
    Queue {
        #[serde(default)]
        capacity: Option<usize>,
        #[serde(default)]
        discipline: Discipline,
    },
    Processor {
        #[serde(default = "one_usize")]
        capacity: usize,
        #[serde(default)]
        setup: Option<Dist>,
        process: Dist,
    },
    Sink,
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

        let mut names = HashSet::new();
        for obj in &self.objects {
            if obj.name.trim().is_empty() {
                errors.push("há um objeto sem nome".to_string());
            } else if !names.insert(obj.name.as_str()) {
                errors.push(format!("nome repetido: '{}'", obj.name));
            }
            let mut dists: Vec<(String, &Dist)> = Vec::new();
            match &obj.kind {
                ObjectKind::Source { interarrival, labels, .. } => {
                    dists.push(("interarrival".into(), interarrival));
                    dists.extend(labels.iter().map(|(k, d)| (format!("labels.{k}"), d)));
                }
                ObjectKind::Queue { capacity, .. } => {
                    if *capacity == Some(0) {
                        errors.push(format!("{}: capacity deve ser >= 1", obj.name));
                    }
                }
                ObjectKind::Processor { capacity, setup, process } => {
                    if *capacity == 0 {
                        errors.push(format!("{}: capacity deve ser >= 1", obj.name));
                    }
                    if let Some(s) = setup {
                        dists.push(("setup".into(), s));
                    }
                    dists.push(("process".into(), process));
                }
                ObjectKind::Sink => {}
            }
            if let Some(dt) = &obj.downtime {
                dists.push(("downtime.mtbf".into(), &dt.mtbf));
                dists.push(("downtime.mttr".into(), &dt.mttr));
            }
            for (what, d) in dists {
                if let Err(e) = d.validate() {
                    errors.push(format!("{}.{what}: {e}", obj.name));
                }
            }
            if let OutputRule::ByLabel(label) = &obj.output {
                if label.trim().is_empty() {
                    errors.push(format!("{}: ByLabel precisa do nome de um rótulo", obj.name));
                }
            }
        }

        let kind_of: HashMap<&str, &ObjectKind> =
            self.objects.iter().map(|o| (o.name.as_str(), &o.kind)).collect();
        let mut outputs: HashMap<&str, usize> = HashMap::new();
        let mut seen = HashSet::new();
        for (from, to) in &self.connections {
            if !seen.insert((from, to)) {
                errors.push(format!("conexão ({from} -> {to}) repetida"));
            }
            for end in [from, to] {
                if !kind_of.contains_key(end.as_str()) {
                    errors.push(format!("conexão ({from} -> {to}): objeto '{end}' não existe"));
                }
            }
            if from == to {
                errors.push(format!("conexão ({from} -> {to}): um objeto não pode ligar em si mesmo"));
            }
            if matches!(kind_of.get(from.as_str()), Some(ObjectKind::Sink)) {
                errors.push(format!("conexão ({from} -> {to}): Sink não tem saída"));
            }
            if matches!(kind_of.get(to.as_str()), Some(ObjectKind::Source { .. })) {
                errors.push(format!("conexão ({from} -> {to}): Source não tem entrada"));
            }
            *outputs.entry(from.as_str()).or_default() += 1;
        }
        for obj in &self.objects {
            let n_out = outputs.get(obj.name.as_str()).copied().unwrap_or(0);
            if !matches!(obj.kind, ObjectKind::Sink) && n_out == 0 {
                errors.push(format!("{}: não tem saída conectada (os itens ficariam presos)", obj.name));
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

        if errors.is_empty() { Ok(()) } else { Err(ModelError::Invalid(errors)) }
    }

    /// Builds one replication. Call [`Model::validate`] first.
    pub fn build(&self, replication: u32) -> Simulation {
        let mut sim = Simulation::new(RunConfig {
            stop_time: self.run.stop_time,
            warmup: self.run.warmup,
            seed: self.run.seed,
            replication,
        });
        let mut ids = HashMap::new();
        for obj in &self.objects {
            let behavior: Box<dyn Behavior> = match &obj.kind {
                ObjectKind::Source { interarrival, first_at_zero, max_arrivals, labels } => {
                    let source = Source::new(interarrival.clone())
                        .first_at_zero(*first_at_zero)
                        .max_arrivals(*max_arrivals);
                    Box::new(labels.iter().fold(source, |s, (k, d)| s.label(k.clone(), d.clone())))
                }
                ObjectKind::Queue { capacity, discipline } => {
                    Box::new(Queue::new(*capacity).discipline(*discipline))
                }
                ObjectKind::Processor { capacity, setup, process } => {
                    Box::new(Processor::new(process.clone()).capacity(*capacity).setup(setup.clone()))
                }
                ObjectKind::Sink => Box::new(Sink),
            };
            let id = sim.add_block(obj.name.clone(), obj.output.clone(), behavior);
            if let Some(dt) = &obj.downtime {
                sim.set_downtime(id, dt.clone());
            }
            ids.insert(obj.name.as_str(), id);
        }
        for (from, to) in &self.connections {
            sim.connect(ids[from.as_str()], ids[to.as_str()]);
        }
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
