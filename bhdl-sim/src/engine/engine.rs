//! Main simulation engine implementation

use crate::error::{SimulationResult, SimulationError};
use crate::circuit::{CircuitState, CircuitLoader};
use crate::engine::{TimeManager, SimulationState, StateMachine, Event, SimulationConfig};
use crate::evaluation::simple::Evaluator;
use crate::propagation::simple::Propagator;
use crate::behavioral::simple::Processor as BehavioralProcessor;
use crate::output::simple::Manager as OutputManager;
use crate::debug::simple::Manager as DebugManager;
use crate::metrics::simple::Collector as StatsCollector;
use crate::events::dispatcher::{EventDispatcher, DispatcherConfig};
use crate::checkpoint::{CheckpointManager, CheckpointFormat};
use bhdl_netlist::Netlist;
use std::sync::Arc;

/// Main simulation engine that orchestrates all simulation components
pub struct SimulationEngine {
    /// Time management
    pub time_manager: TimeManager,
    
    /// Simulation state machine
    pub state_machine: StateMachine,
    
    /// Configuration
    pub config: SimulationConfig,
    
    /// Circuit state
    pub circuit_state: CircuitState,
    
    /// Netlist being simulated
    pub netlist: Arc<Netlist>,
    
    /// Expression evaluator
    pub evaluator: Evaluator,
    
    /// Signal propagator
    pub propagator: Propagator,
    
    /// Behavioral processor
    pub behavioral: BehavioralProcessor,
    
    /// Output manager
    pub output_manager: OutputManager,
    
    /// Debug manager
    pub debug_manager: DebugManager,
    
    /// Statistics collector
    pub stats_collector: StatsCollector,
    
    /// Event dispatcher
    pub event_dispatcher: EventDispatcher,
    
    /// Checkpoint manager
    pub checkpoint_manager: Option<CheckpointManager>,
    
    /// Circuit name
    circuit_name: String,
    
    /// Total simulation steps
    total_steps: u64,
}

impl SimulationEngine {
    /// Create a new simulation engine
    pub fn new(
        config: SimulationConfig,
        netlist: Netlist,
        circuit_name: String,
    ) -> SimulationResult<Self> {
        let netlist = Arc::new(netlist);
        
        // Initialize components
        let circuit_state = CircuitLoader::load_from_netlist(&netlist)?;
        let evaluator = Evaluator::new();
        let propagator = Propagator::new();
        let behavioral = BehavioralProcessor::new();
        let output_manager = OutputManager::new();
        let debug_manager = DebugManager::new();
        let stats_collector = StatsCollector::new();
        let event_dispatcher = EventDispatcher::new(DispatcherConfig::default());
        
        let mut engine = Self {
            time_manager: TimeManager::new(config.time_step),
            state_machine: StateMachine::new(),
            config,
            circuit_state,
            netlist,
            evaluator,
            propagator,
            behavioral,
            output_manager,
            debug_manager,
            stats_collector,
            event_dispatcher,
            checkpoint_manager: None,
            circuit_name,
            total_steps: 0,
        };
        
        // Initialize output manager with circuit
        engine.output_manager.initialize(&engine.netlist)?;
        
        Ok(engine)
    }
    
    /// Set checkpoint manager
    pub fn with_checkpoint_manager(mut self, manager: CheckpointManager) -> Self {
        self.checkpoint_manager = Some(manager);
        self
    }
    
    /// Get current simulation time
    pub fn current_time(&self) -> f64 {
        self.time_manager.current_time()
    }
    
    /// Get total steps
    pub fn total_steps(&self) -> u64 {
        self.total_steps
    }
    
    /// Set total steps (for restore)
    pub fn set_total_steps(&mut self, steps: u64) {
        self.total_steps = steps;
    }
    
    /// Get circuit name
    pub fn circuit_name(&self) -> &str {
        &self.circuit_name
    }
    
    /// Get current state
    pub fn state(&self) -> &SimulationState {
        self.state_machine.current_state()
    }
    
    /// Run the simulation
    pub async fn run(&mut self) -> SimulationResult<()> {
        self.state_machine.transition(Event::Start)?;
        
        while !self.should_stop() {
            // Check if paused
            if matches!(self.state_machine.current_state(), SimulationState::Paused) {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                continue;
            }
            
            // Perform simulation step
            if matches!(self.state_machine.current_state(), SimulationState::Running) {
                self.step().await?;
                
                // Check for auto-checkpoint
                if let Some(manager) = self.checkpoint_manager.as_ref() {
                    if manager.should_checkpoint(self.current_time()) {
                        // Need to temporarily take ownership to avoid borrow issues
                        let mut manager = self.checkpoint_manager.take().unwrap();
                        let result = manager.create_checkpoint(
                            self,
                            CheckpointFormat::CompressedBinary,
                            Some("Auto-checkpoint".to_string()),
                        );
                        self.checkpoint_manager = Some(manager);
                        result?;
                    }
                }
            }
        }
        
        self.state_machine.transition(Event::Complete)?;
        
        // Finalize output
        self.output_manager.finalize()?;
        
        Ok(())
    }
    
    /// Perform a single simulation step
    async fn step(&mut self) -> SimulationResult<()> {
        let start_time = std::time::Instant::now();
        
        // Check debug conditions
        if self.debug_manager.check_conditions(
            self.current_time(),
            &self.circuit_state,
        ) {
            self.state_machine.transition(Event::Pause)?;
            return Ok(());
        }
        
        // Begin timestep
        self.circuit_state.begin_timestep();
        
        // Process events for current time
        while let Some(event) = self.event_dispatcher.process_next() {
            if event.time > self.current_time() {
                // Put it back, it's for the future
                self.event_dispatcher.schedule(event)?;
                break;
            }
        }
        
        // Evaluate attributes
        self.evaluator.evaluate_all(&mut self.circuit_state)?;
        
        // Process behavioral models
        let current_time = self.current_time();
        let current_step = self.time_manager.current_step();
        self.behavioral.process(
            &mut self.circuit_state,
            current_time,
            &current_step,
        )?;
        
        // Propagate signals
        let changes = self.propagator.propagate(&mut self.circuit_state)?;
        
        // Capture output data
        self.output_manager.capture_timestep(
            self.current_time(),
            &self.circuit_state,
            &changes,
        )?;
        
        // Update statistics
        self.stats_collector.record_step(
            self.time_manager.current_step().value(),
            start_time.elapsed(),
            changes.len(),
        );
        
        // Commit timestep
        self.circuit_state.commit_timestep();
        
        // Advance time
        self.time_manager.advance();
        self.total_steps += 1;
        
        Ok(())
    }
    
    /// Check if simulation should stop
    fn should_stop(&self) -> bool {
        matches!(
            self.state_machine.current_state(),
            SimulationState::Completed | SimulationState::Error(_)
        ) || self.time_manager.current_time() >= self.config.max_time
    }
    
    /// Pause the simulation
    pub fn pause(&mut self) -> SimulationResult<()> {
        self.state_machine.transition(Event::Pause)
    }
    
    /// Resume the simulation
    pub fn resume(&mut self) -> SimulationResult<()> {
        self.state_machine.transition(Event::Resume)
    }
    
    /// Stop the simulation
    pub fn stop(&mut self) -> SimulationResult<()> {
        self.state_machine.transition(Event::Error("Stopped by user".to_string()))
    }
    
    /// Create a manual checkpoint
    pub fn create_checkpoint(
        &mut self,
        format: CheckpointFormat,
        description: Option<String>,
    ) -> SimulationResult<String> {
        // Extract the manager temporarily to avoid borrowing issues
        let mut manager = self.checkpoint_manager.take()
            .ok_or_else(|| SimulationError::Other("No checkpoint manager configured".to_string()))?;
        
        let result = manager.create_checkpoint(self, format, description);
        
        // Put the manager back
        self.checkpoint_manager = Some(manager);
        
        result
    }
}