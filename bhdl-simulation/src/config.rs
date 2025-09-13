// Simulation configuration module (stub)

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub max_time: f64,
}

#[derive(Debug, Clone)]
pub enum SolverType {
    DC,
    Transient,
}