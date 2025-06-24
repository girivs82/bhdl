//! Error types for BHDL SPICE

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SpiceError>;

#[derive(Error, Debug)]
pub enum SpiceError {
    #[error("Circuit has no ground node")]
    NoGroundNode,
    
    #[error("Singular matrix - circuit has no unique solution")]
    SingularMatrix,
    
    #[error("Node {0} not found in circuit")]
    NodeNotFound(String),
    
    #[error("Component {0} not found")]
    ComponentNotFound(String),
    
    #[error("Invalid component model: {0}")]
    InvalidModel(String),
    
    #[error("Convergence failed after {0} iterations")]
    ConvergenceFailed(usize),
    
    #[error("Empty circuit - no components to analyze")]
    EmptyCircuit,
    
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
    
    #[error("Voltage constraint violation: {component} node {node} = {voltage}V exceeds limit {limit}V")]
    VoltageViolation {
        component: String,
        node: String,
        voltage: f64,
        limit: f64,
    },
    
    #[error("Current constraint violation: {component} = {current}A exceeds limit {limit}A")]
    CurrentViolation {
        component: String,
        current: f64,
        limit: f64,
    },
    
    #[error("Power constraint violation: {component} = {power}W exceeds limit {limit}W")]
    PowerViolation {
        component: String,
        power: f64,
        limit: f64,
    },
    
    #[error("Numerical error: {0}")]
    NumericalError(String),
    
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}