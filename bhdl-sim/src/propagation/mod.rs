//! Signal propagation subsystem
//! Handles propagation of electrical signals through pins and nets

pub mod pin_propagator;
pub mod net_resolver;
pub mod signal_integrity;
pub mod drive_strength;
pub mod impedance;
pub mod delay_model;

pub use pin_propagator::{PinPropagator, PropagationResult};
pub use net_resolver::{NetResolver, NetConflict};
pub use signal_integrity::{SignalIntegrityChecker, IntegrityViolation};
pub use drive_strength::{DriveStrengthResolver, DriveConflict};
pub use impedance::{ImpedanceCalculator, ImpedanceMismatch};
pub use delay_model::{DelayModel, PropagationDelay};