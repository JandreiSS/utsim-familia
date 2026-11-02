//! Flow items: the entities that move between blocks.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Time;
use crate::block::BlockId;

slotmap::new_key_type! {
    /// Stable handle to a flow item. Stays invalid after the item is destroyed.
    pub struct ItemId;
}

/// A label value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Num(f64),
    Text(String),
}

impl Value {
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Value::Num(v) => Some(*v),
            Value::Text(_) => None,
        }
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Num(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub labels: BTreeMap<String, Value>,
    pub created_at: Time,
    /// Time the item entered its current block.
    pub entered_at: Time,
    /// Block holding the item; `None` while it is packed inside a container.
    pub location: Option<BlockId>,
    /// Container this item is packed in, if any.
    pub container: Option<ItemId>,
    /// Items packed inside this one (pallets, totes...).
    pub contents: Vec<ItemId>,
    /// Released and assigned to a transporter, waiting to be picked up.
    pub awaiting_transport: bool,
    /// Instants the item left the origin of a meter (see `Simulation::add_meter`), by
    /// meter index, until it reaches the meter's destination.
    pub marks: Vec<(usize, Time)>,
}

impl Item {
    pub fn label(&self, name: &str) -> Option<&Value> {
        self.labels.get(name)
    }

    pub fn set_label(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.labels.insert(name.into(), value.into());
    }
}
