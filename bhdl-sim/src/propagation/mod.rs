//! Signal propagation subsystem
//! Handles propagation of electrical signals through pins and nets

pub mod pin_propagator;
pub mod net_resolver;
pub mod signal_integrity;
pub mod drive_strength;
pub mod impedance;
pub mod delay_model;
pub mod simple;

pub use pin_propagator::{PinPropagator, PropagationResult};
pub use net_resolver::{NetResolver, NetConflict};
pub use signal_integrity::{SignalIntegrityChecker, IntegrityViolation};
pub use drive_strength::{DriveStrengthResolver, DriveConflict};
pub use impedance::{ImpedanceCalculator, ImpedanceMismatch};
pub use delay_model::{DelayModel, PropagationDelay};

// Re-export common types from circuit module
pub use crate::circuit::{LogicLevel, DriveStrength};

use bhdl_netlist::InstanceId;

/// Pin update information
#[derive(Debug, Clone)]
pub struct PinUpdate {
    pub instance: InstanceId,
    pub pin: String,
    pub old_voltage: f64,
    pub new_voltage: f64,
}