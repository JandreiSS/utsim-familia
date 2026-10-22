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
    pub location: Option<BlockId>,
}

impl Item {
    pub fn label(&self, name: &str) -> Option<&Value> {
        self.labels.get(name)
    }

    pub fn set_label(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.labels.insert(name.into(), value.into());
    }
}
