//! Initial state file (`estado.ron`): start a run from a given state instead of an empty
//! system ("simular a partir do estado atual", "a partir de segunda às 6 h").
//!
//! The file names objects of the model; each entry is a [`BlockState`] (counts or items
//! with their elapsed times and labels, processor phases, belt positions, rack slots,
//! breakdowns, transporter positions, tank levels). `clock` is the calendar instant at
//! `t = 0`, so shift tables and arrival schedules start at the right point of their cycle.
//! The same file is what [`Model::state_at`] writes, so runs can be chained:
//!
//! ```text
//! InitialState(
//!     clock: 21600.0,
//!     objects: {
//!         "fila": (count: 12),
//!         "M1": (items: [(phase: Processing, elapsed: 4.5)], up: 300.0),
//!         "M2": (down: 120.0),
//!         "esteira": (items: [(at: 9.0), (at: 6.5)]),
//!         "empilhadeira": (position: (12.0, 4.0, 0.0)),
//!         "fluidos": (levels: {"tanque": 40.0}),
//!     },
//! )
//! ```
//!
//! With a state the warm-up is 0 unless the file says otherwise (`warmup`): the state is
//! already a system in operation. Models without a state run exactly as before.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use utsim_core::{BlockId, BlockState, Dist, RunReport, Simulation, StartState, Step, WorkPhase};
use utsim_fluid::FluidKind;

use crate::{Model, ModelError, ObjectKind};

/// The contents of an initial state file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "InitialState", deny_unknown_fields)]
pub struct InitialState {
    /// Calendar instant that corresponds to `t = 0` (same time unit as the model).
    #[serde(default, alias = "relogio")]
    pub clock: f64,
    /// Warm-up of the run started from this state. Absent: none.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "aquecimento"
    )]
    pub warmup: Option<f64>,
    /// State of each object (by name); objects not listed start as usual.
    #[serde(default, alias = "objetos")]
    pub objects: BTreeMap<String, BlockState>,
}

impl InitialState {
    pub fn parse(text: &str) -> Result<InitialState, ModelError> {
        ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
            .from_str(text)
            .map_err(|e| {
                ModelError::Invalid(vec![format!("erro de sintaxe no estado inicial: {e}")])
            })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<InitialState, ModelError> {
        InitialState::parse(&std::fs::read_to_string(path)?)
    }

    /// The state as RON text, readable by [`InitialState::parse`]. Only the outer
    /// struct is named, as in a hand-written file.
    pub fn to_ron(&self) -> String {
        let config = ron::ser::PrettyConfig::default();
        let body = ron::ser::to_string_pretty(self, config).expect("an initial state serializes");
        format!("InitialState{body}\n")
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ModelError> {
        std::fs::write(path, self.to_ron())?;
        Ok(())
    }
}

impl Model {
    /// Block id of the fluid network (after the objects and the flows).
    fn fluid_block(&self) -> Option<(BlockId, &str)> {
        let fluids = self.fluids.as_ref()?;
        Some((
            BlockId(self.objects.len() + self.flows.len()),
            fluids.name.as_str(),
        ))
    }

    /// The model the state runs on: warm-up from the state (0 by default) and the tank
    /// levels of the state as the tanks' initial levels.
    fn with_state_settings(&self, state: &InitialState) -> Model {
        let mut model = self.clone();
        model.run.warmup = state.warmup.unwrap_or(0.0);
        if let Some(fluids) = &mut model.fluids {
            if let Some(entry) = state.objects.get(&fluids.name) {
                for node in &mut fluids.nodes {
                    if let (FluidKind::Tank { initial, .. }, Some(level)) =
                        (&mut node.kind, entry.levels.get(&node.name))
                    {
                        *initial = *level;
                    }
                }
            }
        }
        model
    }

    /// The kernel's view of the state: names resolved to block ids, tank levels left out
    /// (they are the tanks' initial levels).
    fn start_state(&self, state: &InitialState) -> StartState {
        let ids: BTreeMap<&str, BlockId> = self
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| (o.name.as_str(), BlockId(i)))
            .chain(self.fluid_block().map(|(id, name)| (name, id)))
            .collect();
        let blocks = state
            .objects
            .iter()
            .filter_map(|(name, bs)| {
                let id = *ids.get(name.as_str())?;
                let mut bs = bs.clone();
                bs.levels.clear();
                (!bs.is_empty()).then_some((id, bs))
            })
            .collect();
        StartState {
            clock: state.clock,
            blocks,
        }
    }

    /// Checks the state against the model: names, the fields each object type accepts,
    /// capacities, phases, positions. Problems the kernel finds are reported the same way
    /// (it builds replication 0 and starts it). Returns the model the state runs on.
    fn prepare_state(&self, state: &InitialState) -> Result<Model, ModelError> {
        let mut errors = Vec::new();
        if !state.clock.is_finite() {
            errors.push(format!(
                "estado inicial: clock deve ser finito (está {})",
                state.clock
            ));
        }
        if let Some(w) = state.warmup {
            if !(w.is_finite() && w >= 0.0 && w < self.run.stop_time) {
                errors.push(format!(
                    "estado inicial: warmup deve ficar entre 0 e run.stop_time ({}) (está {w})",
                    self.run.stop_time
                ));
            }
        }
        let fluid = self.fluid_block().map(|(_, name)| name);
        for (name, bs) in &state.objects {
            if Some(name.as_str()) == fluid {
                let only_levels = BlockState {
                    levels: bs.levels.clone(),
                    ..BlockState::default()
                };
                if *bs != only_levels {
                    errors.push(format!(
                        "estado inicial de {name}: a rede de fluidos só aceita 'levels'"
                    ));
                }
                let nodes = &self.fluids.as_ref().expect("fluid block exists").nodes;
                for (tank, level) in &bs.levels {
                    match nodes.iter().find(|n| n.name == *tank).map(|n| &n.kind) {
                        Some(FluidKind::Tank { capacity, .. }) => {
                            if !(level.is_finite() && *level >= 0.0 && level <= capacity) {
                                errors.push(format!(
                                    "estado inicial de {name}: o nível de '{tank}' deve ficar entre 0 e a capacidade ({capacity}) (está {level})"
                                ));
                            }
                        }
                        _ => errors.push(format!(
                            "estado inicial de {name}: '{tank}' não é um tanque da rede de fluidos"
                        )),
                    }
                }
                continue;
            }
            if !self.objects.iter().any(|o| o.name == *name) {
                errors.push(format!(
                    "estado inicial: o objeto '{name}' não existe no modelo"
                ));
            } else if !bs.levels.is_empty() {
                errors.push(format!(
                    "estado inicial de {name}: 'levels' só vale para a rede de fluidos"
                ));
            }
        }
        if !errors.is_empty() {
            return Err(ModelError::Invalid(errors));
        }
        let model = self.with_state_settings(state);
        model.validate()?;
        let mut sim = model.build(0);
        sim.set_start_state(model.start_state(state));
        sim.begin().map_err(|e| match e {
            utsim_core::SimError::Model { messages, .. } => ModelError::Invalid(messages),
        })?;
        Ok(model)
    }

    /// Checks `state` against the model (see the module docs); on success, returns the
    /// warnings (inconsistent elapsed times, items without age).
    pub fn check_state(&self, state: &InitialState) -> Result<Vec<String>, ModelError> {
        self.prepare_state(state)?;
        Ok(self.state_warnings(state))
    }

    /// Non-fatal remarks about a state: items counted without age (their stay is
    /// underestimated) and elapsed times past the end of the distribution (the activity
    /// should already be over: its residual is 0).
    pub fn state_warnings(&self, state: &InitialState) -> Vec<String> {
        let mut out = Vec::new();
        let past = |name: &str, what: String, dist: &Dist, elapsed: f64, out: &mut Vec<String>| {
            if !dist.can_exceed(elapsed) {
                out.push(format!(
                    "{name}: {what} já dura {elapsed}, além do máximo da distribuição ({}); o tempo restante será 0",
                    dist.support_max()
                ));
            }
        };
        for (name, bs) in &state.objects {
            let Some(obj) = self.objects.iter().find(|o| o.name == *name) else {
                continue;
            };
            if let Some(n) = bs.count.filter(|n| *n > 0) {
                out.push(format!(
                    "{name}: {n} itens sem idade (count) entram com permanência zero; a permanência e o tempo no sistema dos primeiros itens ficam subestimados"
                ));
            }
            if let Some(dt) = &obj.downtime {
                if let Some(e) = bs.down {
                    past(name, "a quebra".into(), &dt.mttr, e, &mut out);
                }
                if let Some(e) = bs.up {
                    past(
                        name,
                        "o tempo desde o último reparo".into(),
                        &dt.mtbf,
                        e,
                        &mut out,
                    );
                }
            }
            for (k, it) in bs.items.iter().enumerate() {
                let n = k + 1;
                match &obj.kind {
                    ObjectKind::Processor { setup, process, .. } => {
                        let phase = it.phase.unwrap_or(if setup.is_some() {
                            WorkPhase::Setup
                        } else {
                            WorkPhase::Processing
                        });
                        match (phase, setup) {
                            (WorkPhase::Setup, Some(d)) => past(
                                name,
                                format!("o setup do item {n}"),
                                d,
                                it.elapsed,
                                &mut out,
                            ),
                            (WorkPhase::Processing, _) => past(
                                name,
                                format!("o processamento do item {n}"),
                                process,
                                it.elapsed,
                                &mut out,
                            ),
                            _ => {}
                        }
                    }
                    ObjectKind::Rack { dwell: Some(d), .. } if it.phase.is_none() => past(
                        name,
                        format!("a permanência do item {n}"),
                        d,
                        it.elapsed,
                        &mut out,
                    ),
                    _ => {}
                }
            }
            if let (
                Some(e),
                ObjectKind::Source {
                    interarrival,
                    schedule: None,
                    table: None,
                    ..
                },
            ) = (bs.elapsed, &obj.kind)
            {
                past(
                    name,
                    "o intervalo entre chegadas".into(),
                    interarrival,
                    e,
                    &mut out,
                );
            }
        }
        out
    }

    /// Builds one replication that starts from `state` (`None`: the usual empty start).
    /// Call [`Model::check_state`] first; problems left are reported when the run starts.
    pub fn build_from(&self, replication: u32, state: Option<&InitialState>) -> Simulation {
        let Some(state) = state else {
            return self.build(replication);
        };
        let model = self.with_state_settings(state);
        let mut sim = model.build(replication);
        sim.set_start_state(model.start_state(state));
        sim
    }

    /// Like [`Model::run_replications`], every replication starting from `state` (each
    /// with its own streams, so the residual times differ between replications).
    pub fn run_replications_from(
        &self,
        state: &InitialState,
    ) -> Result<Vec<RunReport>, ModelError> {
        let model = self.prepare_state(state)?;
        let start = model.start_state(state);
        model.run_replications_prepared(&|_| true, &|sim: &mut Simulation| {
            sim.set_start_state(start.clone())
        })
    }

    /// The state of a running replication, as an initial state file: what
    /// [`Simulation::save_state`] gives, with object names. The notes say what could not
    /// be represented.
    pub fn save_state(&self, sim: &mut Simulation) -> (InitialState, Vec<String>) {
        let (start, mut notes) = sim.save_state();
        let fluid = self.fluid_block();
        let mut objects = BTreeMap::new();
        for (id, bs) in start.blocks {
            let name = if let Some(obj) = self.objects.get(id.0) {
                obj.name.clone()
            } else if let Some((_, name)) = fluid.filter(|(f, _)| *f == id) {
                name.to_string()
            } else {
                notes.push(format!(
                    "o bloco {} (fluxo ou medição) não entra no estado gravado",
                    id.0
                ));
                continue;
            };
            objects.insert(name, bs);
        }
        let state = InitialState {
            clock: start.clock,
            warmup: None,
            objects,
        };
        (state, notes)
    }

    /// Runs replication `replication` (from `start`, or empty) up to time `t` and returns
    /// the state at `t`, ready to start another run from there. Decisions for an external
    /// agent take `run.default_decision`.
    pub fn state_at(
        &self,
        t: f64,
        start: Option<&InitialState>,
        replication: u32,
    ) -> Result<(InitialState, Vec<String>), ModelError> {
        if !(t.is_finite() && t >= 0.0 && t <= self.run.stop_time) {
            return Err(ModelError::Invalid(vec![format!(
                "o instante para gravar o estado deve ficar entre 0 e run.stop_time ({}) (está {t})",
                self.run.stop_time
            )]));
        }
        let model = match start {
            Some(state) => self.prepare_state(state)?,
            None => {
                self.validate()?;
                self.clone()
            }
        };
        let mut sim = model.build(replication);
        if let Some(state) = start {
            sim.set_start_state(model.start_state(state));
        }
        loop {
            match sim.run_until(t)? {
                Step::Decision(d) => match model.run.default_decision {
                    Some(choice) => sim.decide(choice.min(d.options.saturating_sub(1)))?,
                    None => {
                        return Err(ModelError::Invalid(vec![
                            "o modelo pede decisões de um agente externo; defina run.default_decision para gravar o estado"
                                .into(),
                        ]));
                    }
                },
                Step::Finished if t >= model.run.stop_time => {
                    sim.finish();
                    break;
                }
                _ => break,
            }
        }
        Ok(model.save_state(&mut sim))
    }
}
