//! BHDL Behavioral Simulation Engine
//! 
//! This crate provides the core simulation infrastructure for BHDL circuits,
//! enabling time-based behavioral simulation with support for:
//! 
//! - Time-stepped simulation with adaptive time control
//! - Expression-based attribute evaluation
//! - Conditional behavior through when blocks
//! - Signal propagation and event detection
//! - Waveform capture and analysis

pub mod engine;
pub mod circuit;
pub mod evaluation;
pub mod propagation;
pub mod behavioral;
pub mod output;
pub mod debug;
pub mod metrics;
pub mod error;

pub use engine::{SimulationEngine, SimulationState, SimulationConfig};
pub use circuit::{CircuitState, CircuitLoader};
pub use error::{SimulationError, SimulationResult};

// Re-export commonly used types
pub use engine::control::{Command, Response};
pub use engine::time::TimeManager;
