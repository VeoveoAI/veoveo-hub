//! Console administration surfaces that the Gateway owns: Audit reads and their stream limits.
//! Console snapshots and change streams belong to the Console's own service.
pub(crate) mod audit;
mod limits;

pub(crate) use limits::ConsoleStreamLimits;
