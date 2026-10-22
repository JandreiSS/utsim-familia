//! What happened, in order: the event stream consumed by the 3D viewer (phase 2).
//!
//! The kernel emits a [`TraceEvent`] for every item creation, move, packing,
//! destruction and block state change. Observers only read; they can never change
//! the simulation, so a run with or without a viewer gives identical results.

use serde::{Deserialize, Serialize};
use slotmap::Key;

use crate::Time;
use crate::block::State;
use crate::item::ItemId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "e", rename_all = "snake_case")]
pub enum TraceEvent {
    /// A new item appeared inside `block`.
    Created { item: u64, block: usize },
    /// The item left `from` and entered `to` (instantaneous in the simulation).
    Moved { item: u64, from: usize, to: usize },
    /// The item was put inside `container` and no longer occupies the block.
    Packed { item: u64, container: u64 },
    /// The item was taken out of its container into `block`.
    Unpacked { item: u64, block: usize },
    /// The item (and anything packed in it, reported separately) left the model.
    Destroyed { item: u64 },
    /// The block entered a new state.
    State { block: usize, state: State },
    /// Number of tokens now in activity `activity` (index in the flow definition)
    /// of flow block `flow`.
    Tokens {
        flow: usize,
        activity: usize,
        count: u32,
    },
    /// The item was put in storage position `slot` of `block` (racks).
    Slot {
        item: u64,
        block: usize,
        slot: usize,
    },
    /// A mobile resource starts moving along `path`, arriving after `duration`.
    Travel {
        carrier: usize,
        path: Vec<[f64; 3]>,
        duration: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceRecord {
    pub t: Time,
    #[serde(flatten)]
    pub event: TraceEvent,
}

/// Live listener, e.g. a viewer streaming a running simulation.
pub trait Observer: Send {
    fn on_event(&mut self, t: Time, event: &TraceEvent);
}

/// Stable numeric id of an item, as used in traces.
pub fn item_key(id: ItemId) -> u64 {
    id.data().as_ffi()
}
