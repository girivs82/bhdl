//! Error types for testbench framework

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TestbenchError {
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Signal not found: {0}")]
    SignalNotFound(String),
    
    #[error("Waveform capture error: {0}")]
    WaveformError(String),
    
    #[error("Verification failed: {0}")]
    VerificationError(String),
    
    #[error("Stimulus generation error: {0}")]
    StimulusError(String),
    
    #[error("SPICE simulation error: {0}")]
    SpiceError(#[from] bhdl_spice::SpiceError),
    
    #[error("Behavioral simulation error: {0}")]
    BehavioralError(#[from] bhdl_sim::SimulationError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, TestbenchError>;