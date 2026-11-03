//! Runs with the optional per-item metrics of the kernel ([`utsim_core::item_log`]):
//! visits, stays, work and touch time of every item, for the indicators of the lines.

use utsim_core::{ItemRecord, RunReport};

use crate::{Model, ModelError};

impl Model {
    /// Runs replication `replication` keeping the per-item metrics. The report is the
    /// same as without them.
    pub fn run_item_log(
        &self,
        replication: u32,
    ) -> Result<(RunReport, Vec<ItemRecord>), ModelError> {
        self.validate()?;
        let mut sim = self.build(replication);
        sim.enable_item_log();
        let report = self.drive(&mut sim)?;
        Ok((report, sim.take_item_log()))
    }
}
