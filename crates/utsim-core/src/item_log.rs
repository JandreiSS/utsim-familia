//! Optional per-item metrics: for each item, the visits to each block, the time it stayed
//! in each one, the work done on it (setup and processing) and the part of that work done
//! by people (touch time). Off by default ([`crate::Simulation::enable_item_log`]): it
//! never changes the run or its report, only keeps memory per item.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::Time;
use crate::item::{ItemId, Value};

/// What happened to one item.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemRecord {
    /// Item id as in the trace ([`crate::trace::item_key`]).
    pub key: u64,
    pub created_at: Time,
    /// When it left the model (destroyed, usually in a sink); `None`: still inside at
    /// the end of the run.
    pub ended_at: Option<Time>,
    /// Block where it ended, or where it is at the end of the run (`None`: packed).
    pub block: Option<usize>,
    /// Labels at the end.
    pub labels: BTreeMap<String, Value>,
    /// Entries into each block (block index → count).
    pub visits: BTreeMap<usize, u32>,
    /// Time spent inside each block, every visit added (for items still inside, up to
    /// the end of the run).
    pub stay: BTreeMap<usize, Time>,
    /// Work time on the item in each block (setup and processing durations drawn).
    pub work: BTreeMap<usize, Time>,
    /// Part of `work` done by operators (people): the touch time.
    pub touch: BTreeMap<usize, Time>,
}

impl ItemRecord {
    pub fn total_work(&self) -> Time {
        self.work.values().sum()
    }

    pub fn total_touch(&self) -> Time {
        self.touch.values().sum()
    }

    /// Most visits to a single block minus one: the rework loops the item went through.
    pub fn loops(&self) -> u32 {
        self.visits
            .values()
            .max()
            .map_or(0, |v| v.saturating_sub(1))
    }
}

/// The log kept by the kernel while enabled.
#[derive(Default)]
pub(crate) struct ItemLog {
    pub open: HashMap<ItemId, ItemRecord>,
    pub done: Vec<ItemRecord>,
}

impl ItemLog {
    pub fn record(&mut self, id: ItemId, created_at: Time) -> &mut ItemRecord {
        self.open.entry(id).or_insert_with(|| ItemRecord {
            key: crate::trace::item_key(id),
            created_at,
            ..ItemRecord::default()
        })
    }
}
