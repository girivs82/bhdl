//! Interactive debugging support for simulations

pub mod breakpoint;
pub mod watchpoint;
pub mod debugger;
pub mod inspector;
pub mod simple;

// Export simple version
pub mod manager {
    pub use super::simple::Manager;
}

pub use breakpoint::{Breakpoint, BreakpointType, BreakpointCondition};
pub use watchpoint::{Watchpoint, WatchpointType, WatchpointTrigger};
pub use debugger::{Debugger, DebugCommand, DebugState};
pub use inspector::{StateInspector, InspectionResult};