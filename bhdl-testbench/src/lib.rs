//! BHDL Testbench Framework
//! 
//! This crate provides testbench capabilities for BHDL circuits:
//! - Testbench definition and compilation
//! - Stimulus generation
//! - Waveform capture in multiple formats
//! - Verification and assertions
//! - Coordination with bhdl-sim and bhdl-spice

pub mod testbench;
pub mod waveform;
pub mod stimulus;
pub mod verification;
pub mod coordinator;
pub mod error;
pub mod compiler;
pub mod fault_injection;

pub use testbench::{
    Testbench, SimulationConfig, SolverType,
    Scope, CaptureMode, TriggerCondition, TriggerType,
    Stimulus, Waveform,
    Assertion, AssertionCondition, TimeConstraint, Severity,
    Measurement, MeasurementType,
};
pub use waveform::{WaveformCapture, WaveformFormat};
pub use stimulus::StimulusGenerator;
pub use verification::VerificationEngine;
pub use coordinator::TestbenchRunner;
pub use error::{TestbenchError, Result};
pub use compiler::compile_testbench;

/// Signal reference for testbenches
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
    
    /// Component voltage (C1.voltage)
    Voltage(String),
    
    /// Temperature (U1.junction_temp)
    Temperature(String),
}

impl SignalRef {
    pub fn parse(s: &str) -> Result<Self> {
        if s.starts_with('@') {
            Ok(SignalRef::Net(s[1..].to_string()))
        } else if s.ends_with(".current") {
            let instance = s.trim_end_matches(".current");
            Ok(SignalRef::Current(instance.to_string()))
        } else if s.ends_with(".power") {
            let instance = s.trim_end_matches(".power");
            Ok(SignalRef::Power(instance.to_string()))
        } else if s.ends_with(".voltage") {
            let instance = s.trim_end_matches(".voltage");
            Ok(SignalRef::Voltage(instance.to_string()))
        } else if s.ends_with(".junction_temp") {
            let instance = s.trim_end_matches(".junction_temp");
            Ok(SignalRef::Temperature(instance.to_string()))
        } else if let Some(dot_pos) = s.find('.') {
            let instance = &s[..dot_pos];
            let pin = &s[dot_pos + 1..];
            Ok(SignalRef::Pin {
                instance: instance.to_string(),
                pin: pin.to_string(),
            })
        } else {
            Err(TestbenchError::ParseError(
                format!("Invalid signal reference: {}", s)
            ))
        }
    }
    
    pub fn to_string(&self) -> String {
        match self {
            SignalRef::Net(name) => format!("@{}", name),
            SignalRef::Pin { instance, pin } => format!("{}.{}", instance, pin),
            SignalRef::Current(inst) => format!("{}.current", inst),
            SignalRef::Power(inst) => format!("{}.power", inst),
            SignalRef::Voltage(inst) => format!("{}.voltage", inst),
            SignalRef::Temperature(inst) => format!("{}.junction_temp", inst),
        }
    }
}

/// Time specification for testbench operations
#[derive(Debug, Clone, Copy)]
pub struct TimeSpec {
    pub value: f64,
    pub unit: TimeUnit,
}

#[derive(Debug, Clone, Copy)]
pub enum TimeUnit {
    Seconds,
    Milliseconds,
    Microseconds,
    Nanoseconds,
    Picoseconds,
}

impl TimeSpec {
    pub fn as_seconds(&self) -> f64 {
        match self.unit {
            TimeUnit::Seconds => self.value,
            TimeUnit::Milliseconds => self.value * 1e-3,
            TimeUnit::Microseconds => self.value * 1e-6,
            TimeUnit::Nanoseconds => self.value * 1e-9,
            TimeUnit::Picoseconds => self.value * 1e-12,
        }
    }
}