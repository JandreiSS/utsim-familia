//! Hooks for user logic (phase 6). The kernel does not know any scripting
//! language: the model compiles user scripts (Rhai, in `utsim-script`) into these
//! traits and registers them on blocks.

use crate::Time;
use crate::item::Item;
use crate::rng::SimRng;

/// Read/write access a script gets to the world at the moment it runs.
pub struct ScriptEnv<'a> {
    pub now: Time,
    /// The owning block's random stream, so scripts stay reproducible.
    pub rng: &'a mut SimRng,
    pub block: &'a str,
}

/// Runs when an item enters a block, before the block's own `on_entry`
/// (e.g. to set labels that the block or its routing will use).
pub trait ItemScript: Send {
    fn run(&mut self, item: &mut Item, env: &mut ScriptEnv) -> Result<(), String>;
}

/// Chooses the output port (0-based) for a released item: `OutputRule::Script`.
pub trait PortScript: Send {
    fn port(&mut self, item: &Item, outputs: usize, env: &mut ScriptEnv) -> Result<usize, String>;
}
