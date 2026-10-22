//! Standard fixed-resource blocks.

mod processor;
mod queue;
mod sink;
mod source;

pub use processor::Processor;
pub use queue::{Discipline, Queue};
pub use sink::Sink;
pub use source::Source;
