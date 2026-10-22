//! Scenario experiments: one base model, several parameter patches, shared seeds.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use utsim_core::stats::Summary;
use utsim_core::summarize;

use crate::{Model, ModelError, ObjectKind};

/// An experiment file: a base model plus named scenarios that patch it.
#[derive(Clone, Debug, Deserialize)]
pub struct Experiment {
    pub name: String,
    /// Model path, relative to the experiment file.
    pub model: String,
    /// Overrides `run.replications` of the base model.
    #[serde(default)]
    pub replications: Option<u32>,
    pub scenarios: Vec<Scenario>,
    /// Metric keys to keep (e.g. `fila.content.mean`); empty keeps all.
    #[serde(default)]
    pub measures: Vec<String>,
    /// Directory of the experiment file, used to resolve `model`.
    #[serde(skip)]
    base_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Scenario {
    pub name: String,
    /// `<object>.<field>` or `run.<field>` to RON source text of the new value.
    #[serde(default)]
    pub set: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScenarioResult {
    pub name: String,
    pub summary: BTreeMap<String, Summary>,
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

    /// Runs every scenario. All scenarios keep the base seed, so replication `r`
    /// uses the same random streams in each one (common random numbers).
    pub fn run(&self) -> Result<ExperimentResult, ModelError> {
        let mut base = Model::load(self.base_dir.join(&self.model))?;
        if let Some(r) = self.replications {
            base.run.replications = r;
        }
        let mut scenarios = Vec::new();
        for sc in &self.scenarios {
            let model = apply_patches(&base, &sc.set)
                .map_err(|e| invalid(format!("cenário '{}': {e}", sc.name)))?;
            let mut summary = summarize(&model.run_replications()?);
            if !self.measures.is_empty() {
                summary.retain(|k, _| self.measures.contains(k));
            }
            scenarios.push(ScenarioResult {
                name: sc.name.clone(),
                summary,
            });
        }
        Ok(ExperimentResult {
            name: self.name.clone(),
            measures: self.measures.clone(),
            scenarios,
        })
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
            "default_decision" => set!(run.default_decision),
            _ => {
                return Err(invalid(format!(
                    "'{key}': campo 'run.{field}' não existe; campos válidos: stop_time, warmup, seed, replications, default_decision"
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
        "transport" => {
            set!(obj.transport);
            return Ok(());
        }
        "on_entry" => {
            set!(obj.on_entry);
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
            ],
        ),
        ObjectKind::Queue { .. } => ("Queue", &["capacity", "discipline", "batch"]),
        ObjectKind::Processor { .. } => {
            ("Processor", &["capacity", "setup", "process", "operators"])
        }
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
            ],
        ),
        ObjectKind::Dispatcher { .. } => ("Dispatcher", &["team", "rule"]),
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
        (ObjectKind::Queue { capacity, .. }, "capacity") => set!(*capacity),
        (ObjectKind::Queue { discipline, .. }, "discipline") => set!(*discipline),
        (ObjectKind::Queue { batch, .. }, "batch") => set!(*batch),
        (ObjectKind::Processor { capacity, .. }, "capacity") => set!(*capacity),
        (ObjectKind::Processor { setup, .. }, "setup") => set!(*setup),
        (ObjectKind::Processor { process, .. }, "process") => set!(*process),
        (ObjectKind::Processor { operators, .. }, "operators") => set!(*operators),
        (ObjectKind::Transporter { speed, .. }, "speed") => set!(*speed),
        (ObjectKind::Transporter { acceleration, .. }, "acceleration") => set!(*acceleration),
        (ObjectKind::Transporter { deceleration, .. }, "deceleration") => set!(*deceleration),
        (ObjectKind::Transporter { capacity, .. }, "capacity") => set!(*capacity),
        (ObjectKind::Transporter { load_time, .. }, "load_time") => set!(*load_time),
        (ObjectKind::Transporter { unload_time, .. }, "unload_time") => set!(*unload_time),
        (ObjectKind::Transporter { agv, .. }, "agv") => set!(*agv),
        (ObjectKind::Transporter { home, .. }, "home") => set!(*home),
        (ObjectKind::Dispatcher { team, .. }, "team") => set!(*team),
        (ObjectKind::Dispatcher { rule, .. }, "rule") => set!(*rule),
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
                    "transport",
                    "on_entry",
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
            e.contains("Processor") && e.contains("capacity, setup, process, operators, output"),
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
}
