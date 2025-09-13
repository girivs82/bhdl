// Results module - minimal stub for compilation

use std::collections::HashMap;
use crate::Result;

#[derive(Debug, Clone)]
pub struct SimulationResults {
    pub metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub struct SimulationSummary {
    pub description: String,
}