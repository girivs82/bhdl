//! Extended Circuit Analysis Module
//! 
//! Provides advanced analysis capabilities for SPICE circuits including:
//! - Component role detection through topology-based analysis and simulation
//! - Functional block recognition without naming dependencies
//! - Design intent inference from circuit structure
//! - Real AC, DC, transient, and noise simulation
//!
//! ## Key Features
//!
//! ### Topology-Based Component Role Detection
//! The `ComponentRoleDetector` analyzes circuit connectivity patterns and electrical
//! characteristics to determine component functions. Unlike traditional approaches,
//! it doesn't rely on node names (VIN, GND) or component names (C_in, R_load).
//!
//! ### Real Simulation Integration  
//! The `SimulationEngine` provides actual SPICE analysis including:
//! - DC operating point and regulation metrics
//! - AC frequency response with stability margins
//! - Transient step response and settling time
//! - Noise analysis with PSRR measurements
//!
//! See README.md for detailed documentation and examples.

pub mod component_role_detector;
pub mod simulation_engine;

pub use component_role_detector::{
    ComponentRoleDetector, 
    ComponentRole, 
    CircuitPerformance, 
    ComponentImpact
};

pub use simulation_engine::{
    SimulationEngine,
    AcAnalysisResult,
    TransientAnalysisResult,
    NoiseAnalysisResult,
    FrequencyPoint,
};