//! Error types for the simulation engine

use thiserror::Error;

/// Result type for simulation operations
pub type SimulationResult<T> = Result<T, SimulationError>;

/// Main error type for simulation operations
#[derive(Debug, Clone, Error)]
pub enum SimulationError {
    #[error("Circuit loading failed: {0}")]
    LoadError(String),
    
    #[error("Invalid configuration: {0}")]
    ConfigError(String),
    
    #[error("Time step error: {0}")]
    TimeError(String),
    
    #[error("Evaluation error: {0}")]
    EvaluationError(String),
    
    #[error("State error: {0}")]
    StateError(String),
    
    #[error("Control error: {0}")]
    ControlError(String),
    
    #[error("Convergence failed after {iterations} iterations")]
    ConvergenceError { iterations: usize },
    
    #[error("Numerical overflow in {context}")]
    NumericalOverflow { context: String },
    
    #[error("Invalid pin access: {0}")]
    PinAccessError(String),
    
    #[error("Communication error: {0}")]
    CommunicationError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Probe error: {0}")]
    ProbeError(String),
    
    #[error("Debug error: {0}")]
    DebugError(String),
    
    // #[error("Analysis error: {0}")]
    // AnalysisError(#[from] bhdl_analyzer::AnalysisError),
    
    #[error("Other error: {0}")]
    Other(String),
}

/// Breakpoint types for debugging
#[derive(Debug, Clone, PartialEq)]
pub enum Breakpoint {
    /// Break at specific simulation time
    TimeBreakpoint(f64),
    
    /// Break when condition is met
    ConditionBreakpoint {
        condition: String,
        id: BreakpointId,
    },
    
    /// Break on attribute change
    AttributeBreakpoint {
        attribute: String,
        id: BreakpointId,
    },
    
    /// Break on pin value change
    PinBreakpoint {
        pin: String,
        id: BreakpointId,
    },
}

/// Unique identifier for breakpoints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BreakpointId(pub u32);

impl BreakpointId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}