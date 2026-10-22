//! A recorded run with its layout: the input of the 3D viewer (phase 2).

use serde::{Deserialize, Serialize};
use utsim_core::{RunReport, TraceRecord};

use crate::{Model, ModelError, ObjectKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayBlock {
    pub name: String,
    /// Object type as written in the model ("Source", "Queue", "Processor"...).
    pub kind: String,
    pub position: [f64; 3],
}

/// Everything needed to animate one replication without running it again.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replay {
    pub model: String,
    pub replication: u32,
    pub stop_time: f64,
    /// Indexed like the `block` fields of the trace events.
    pub blocks: Vec<ReplayBlock>,
    /// `(from, to)` block indices, in connection order.
    pub connections: Vec<(usize, usize)>,
    pub records: Vec<TraceRecord>,
    pub report: RunReport,
    /// Fluid network layout (node names, kinds, positions), when the model has one.
    #[serde(default)]
    pub fluids: Option<utsim_fluid::FluidDef>,
}

pub fn kind_name(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Source { .. } => "Source",
        ObjectKind::Queue { .. } => "Queue",
        ObjectKind::Processor { .. } => "Processor",
        ObjectKind::Combiner { .. } => "Combiner",
        ObjectKind::Separator { .. } => "Separator",
        ObjectKind::Sink => "Sink",
        ObjectKind::Transporter { .. } => "Transporter",
        ObjectKind::Dispatcher { .. } => "Dispatcher",
        ObjectKind::Conveyor { .. } => "Conveyor",
        ObjectKind::Rack { .. } => "Rack",
    }
}

impl Model {
    /// Runs one replication recording its full trace.
    pub fn replay(&self, replication: u32) -> Result<Replay, ModelError> {
        self.validate()?;
        let mut sim = self.build(replication);
        sim.enable_trace();
        let report = sim.run()?;
        let index = |name: &str| {
            self.objects
                .iter()
                .position(|o| o.name == name)
                .expect("validated")
        };
        Ok(Replay {
            model: self.name.clone(),
            replication,
            stop_time: self.run.stop_time,
            blocks: self
                .objects
                .iter()
                .map(|o| ReplayBlock {
                    name: o.name.clone(),
                    kind: kind_name(&o.kind).to_string(),
                    position: o.position,
                })
                // Flows are blocks too (no position): keeps trace indices aligned.
                .chain(self.flows.iter().map(|f| ReplayBlock {
                    name: f.name.clone(),
                    kind: "Flow".to_string(),
                    position: [0.0; 3],
                }))
                .chain(self.fluids.iter().map(|f| ReplayBlock {
                    name: f.name.clone(),
                    kind: "Fluid".to_string(),
                    position: [0.0; 3],
                }))
                .collect(),
            connections: self
                .connections
                .iter()
                .map(|(a, b)| (index(a), index(b)))
                .collect(),
            records: sim.take_trace(),
            report,
            fluids: self.fluids.clone(),
        })
    }
}
