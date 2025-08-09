//! BHDL Simulation Infrastructure
//! 
//! This crate provides simulation capabilities for BHDL circuits including:
//! - Testbench support with stimulus and verification
//! - Waveform capture in multiple formats (VCD, FST, CSV)
//! - Integration with SPICE and behavioral simulators
//! - Mixed-signal simulation coordination

pub mod testbench;
pub mod waveform;
pub mod stimulus;
pub mod verification;
pub mod coordinator;
pub mod config;
pub mod results;
pub mod fault_injection;

pub use config::{SimulationConfig, SolverType};
pub use coordinator::SimulationCoordinator;
pub use results::{SimulationResults, SimulationSummary};
pub use testbench::{Testbench, Scope, CaptureMode};
pub use waveform::{WaveformFormat, WaveformWriter};
pub use stimulus::{Stimulus, Waveform};
pub use verification::{Assertion, Measurement};
pub use fault_injection::{
    FaultInjection, FaultType, FaultTarget, FaultCondition,
    FaultInjectionManager, ComponentFaultBehavior
};

/// Simulation error type
#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Solver error: {0}")]
    SolverError(#[from] bhdl_spice::SpiceError),
    
    #[error("Waveform capture error: {0}")]
    WaveformError(String),
    
    #[error("Verification failed: {0}")]
    VerificationError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, SimulationError>;

/// Signal reference for identifying signals in the circuit
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SignalRef {
    /// Net reference (@VCC, @GND)
    Net(String),
    
    /// Component pin (U1.FB)
    Pin { instance: String, pin: String },
    
    /// Component current (R1.current)
    Current(String),
    
    /// Component power (R1.power)
    Power(String),
    
    /// Derived signal from expression
    Expression(String),
}

impl SignalRef {
    /// Parse a signal reference from string
    pub fn parse(s: &str) -> Result<Self> {
        if s.starts_with('@') {
            Ok(SignalRef::Net(s[1..].to_string()))
        } else if s.ends_with(".current") {
            let instance = s.trim_end_matches(".current");
            Ok(SignalRef::Current(instance.to_string()))
        } else if s.ends_with(".power") {
            let instance = s.trim_end_matches(".power");
            Ok(SignalRef::Power(instance.to_string()))
        } else if let Some(dot_pos) = s.find('.') {
            let instance = &s[..dot_pos];
            let pin = &s[dot_pos + 1..];
            Ok(SignalRef::Pin {
                instance: instance.to_string(),
                pin: pin.to_string(),
            })
        } else {
            Err(SimulationError::ParseError(
                format!("Invalid signal reference: {}", s)
            ))
        }
    }
}

/// Time window for various simulation operations
#[derive(Debug, Clone, Copy)]
pub struct TimeWindow {
    pub start: f64,
    pub end: f64,
}

impl TimeWindow {
    pub fn new(start: f64, end: f64) -> Self {
        Self { start, end }
    }
    
    pub fn contains(&self, time: f64) -> bool {
        time >= self.start && time <= self.end
    }
    
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }
}