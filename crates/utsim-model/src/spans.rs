//! Model-level measurements: the time each item takes between two points of the flow.
//!
//! `spans: [(name: "espera_ate_exame", from: "triagem", to: "exame")]` measures, per
//! item, the time from the instant it leaves `from` to the instant it enters `to` (queues
//! and travel in between included). Each measurement becomes a block of kind `span`
//! named after it, with the tally `value`: the report shows its mean, confidence
//! interval and histogram, and the metrics are `<name>.value.mean` and
//! `<name>.value.count`, usable in experiments and optimisation like any other.
//!
//! An item that never reaches `to` is not counted; one that passes `from` again
//! restarts its clock; an item entering `to` without having left `from` is ignored.
//! The kernel keeps the clock on the item (`Simulation::add_meter`), so it also works
//! when `to` is a sink that destroys the item on arrival.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use utsim_core::{BlockId, OutputRule, Simulation};
use utsim_objects::Span;

use crate::Model;

/// One measurement: time from leaving object `from` to entering object `to`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpanDef {
    /// Name of the measurement (also the name of its block in the report).
    pub name: String,
    pub from: String,
    pub to: String,
}

impl Model {
    /// Problems with `spans`: empty or repeated names, unknown objects.
    pub(crate) fn validate_spans(&self, errors: &mut Vec<String>) {
        let objects: HashSet<&str> = self.objects.iter().map(|o| o.name.as_str()).collect();
        let mut used: HashSet<&str> = objects.clone();
        used.extend(self.flows.iter().map(|f| f.name.as_str()));
        used.extend(self.fluids.iter().map(|f| f.name.as_str()));
        for m in &self.spans {
            if m.name.trim().is_empty() {
                errors.push("há uma medição sem nome".to_string());
            } else if !used.insert(m.name.as_str()) {
                errors.push(format!(
                    "medição '{}': o nome já é usado por outro objeto, fluxo ou medição",
                    m.name
                ));
            }
            for (field, target) in [("from", &m.from), ("to", &m.to)] {
                if !objects.contains(target.as_str()) {
                    errors.push(format!(
                        "medição '{}': {field} '{target}' não é um objeto do modelo",
                        m.name
                    ));
                }
            }
        }
    }

    /// Adds one `span` block per measurement (after objects, flows and fluids).
    pub(crate) fn add_spans(&self, sim: &mut Simulation, ids: &HashMap<String, BlockId>) {
        for m in &self.spans {
            let into = sim.add_block(m.name.clone(), OutputRule::FirstAvailable, Box::new(Span));
            sim.add_meter(ids[&m.from], ids[&m.to], into);
        }
    }
}
