//! Core simulation engine modules

pub mod time;
pub mod state;
pub mod control;
pub mod config;

pub use time::TimeManager;
pub use state::{SimulationState, StateMachine, Event};
pub use control::{SimulationControl, Command, Response};
pub use config::{SimulationConfig, PerformanceConfig, OutputConfig};

use crate::error::SimulationResult;
use crate::circuit::CircuitState;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Main simulation engine that orchestrates the simulation
pub struct SimulationEngine {
    /// Time management
    time_manager: TimeManager,
    
    /// Simulation state machine
    state_machine: StateMachine,
    
    /// Configuration
    config: SimulationConfig,
    
    /// Circuit being simulated
    circuit: Arc<Mutex<CircuitState>>,
    
    /// Control interface
    control: Option<SimulationControl>,
}

impl SimulationEngine {
    /// Create a new simulation engine
    pub fn new(config: SimulationConfig, circuit: CircuitState) -> Self {
        Self {
            time_manager: TimeManager::new(config.time_step),
            state_machine: StateMachine::new(),
            config,
            circuit: Arc::new(Mutex::new(circuit)),
            control: None,
        }
    }
    
    /// Set up control interface
    pub fn with_control(mut self, control: SimulationControl) -> Self {
        self.control = Some(control);
        self
    }
    
    /// Get current simulation time
    pub fn current_time(&self) -> f64 {
        self.time_manager.current_time()
    }
    
    /// Get current state
    pub fn state(&self) -> &SimulationState {
        self.state_machine.current_state()
    }
    
    /// Run the simulation
    pub async fn run(&mut self) -> SimulationResult<()> {
        self.state_machine.transition(Event::Start)?;
        
        while !self.should_stop() {
            // Check for control commands
            if let Some(ref mut control) = self.control {
                control.process_commands(&mut self.state_machine).await?;
            }
            
            // Perform simulation step if running
            if matches!(self.state_machine.current_state(), SimulationState::Running) {
                self.step().await?;
            } else {
                // Wait a bit if paused or idle
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
        
        self.state_machine.transition(Event::Complete)?;
        Ok(())
    }
    
    /// Perform a single simulation step
    async fn step(&mut self) -> SimulationResult<()> {
        // Advance time
        let _dt = self.time_manager.advance();
        
        // Update circuit state
        let mut circuit = self.circuit.lock().await;
        circuit.begin_timestep();
        
        // TODO: Evaluate attributes, process when blocks, propagate signals
        
        circuit.commit_timestep();
        
        Ok(())
    }
    
    /// Check if simulation should stop
    fn should_stop(&self) -> bool {
        matches!(
            self.state_machine.current_state(),
            SimulationState::Completed | SimulationState::Error(_)
        ) || self.time_manager.current_time() >= self.config.max_time
    }
}