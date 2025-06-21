//! Simulation state machine

use crate::error::{SimulationError, SimulationResult};
use std::collections::HashMap;

/// Possible states of the simulation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimulationState {
    /// Simulation not yet started
    Idle,
    
    /// Initializing circuit and configuration
    Initializing,
    
    /// Actively running simulation steps
    Running,
    
    /// Temporarily paused
    Paused,
    
    /// Executing single steps
    Stepping,
    
    /// Simulation completed successfully
    Completed,
    
    /// Simulation stopped due to error
    Error(String),
}

impl std::fmt::Display for SimulationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Initializing => write!(f, "Initializing"),
            Self::Running => write!(f, "Running"),
            Self::Paused => write!(f, "Paused"),
            Self::Stepping => write!(f, "Stepping"),
            Self::Completed => write!(f, "Completed"),
            Self::Error(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl SimulationState {
    /// Check if this state is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(self, SimulationState::Completed | SimulationState::Error(_))
    }
}

/// Events that trigger state transitions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Event {
    Start,
    Initialize,
    Run,
    Pause,
    Resume,
    Step,
    Complete,
    Error(String),
    Reset,
}

/// State machine for managing simulation lifecycle
pub struct StateMachine {
    state: SimulationState,
    transitions: HashMap<(SimulationState, Event), SimulationState>,
}

impl StateMachine {
    /// Create a new state machine
    pub fn new() -> Self {
        let mut machine = Self {
            state: SimulationState::Idle,
            transitions: HashMap::new(),
        };
        
        machine.setup_transitions();
        machine
    }
    
    /// Set up valid state transitions
    fn setup_transitions(&mut self) {
        use SimulationState::*;
        use Event::*;
        
        // From Idle
        self.add_transition(Idle, Start, Initializing);
        self.add_transition(Idle, Event::Error(String::new()), SimulationState::Error(String::new()));
        
        // From Initializing
        self.add_transition(Initializing, Run, Running);
        self.add_transition(Initializing, Event::Error(String::new()), SimulationState::Error(String::new()));
        
        // From Running
        self.add_transition(Running, Pause, Paused);
        self.add_transition(Running, Complete, Completed);
        self.add_transition(Running, Event::Error(String::new()), SimulationState::Error(String::new()));
        self.add_transition(Running, Step, Stepping);
        
        // From Paused
        self.add_transition(Paused, Resume, Running);
        self.add_transition(Paused, Step, Stepping);
        self.add_transition(Paused, Complete, Completed);
        self.add_transition(Paused, Event::Error(String::new()), SimulationState::Error(String::new()));
        
        // From Stepping
        self.add_transition(Stepping, Run, Running);
        self.add_transition(Stepping, Pause, Paused);
        self.add_transition(Stepping, Step, Stepping);
        self.add_transition(Stepping, Complete, Completed);
        self.add_transition(Stepping, Event::Error(String::new()), SimulationState::Error(String::new()));
        
        // From Completed
        self.add_transition(Completed, Reset, Idle);
        
        // From Error
        self.add_transition(SimulationState::Error(String::new()), Reset, Idle);
    }
    
    /// Add a state transition
    fn add_transition(&mut self, from: SimulationState, event: Event, to: SimulationState) {
        // For error events, we use a placeholder - actual error message is handled in transition
        let key = match (&from, &event) {
            (SimulationState::Error(_), _) => (SimulationState::Error(String::new()), event),
            (_, Event::Error(_)) => (from, Event::Error(String::new())),
            _ => (from, event),
        };
        self.transitions.insert(key, to);
    }
    
    /// Get current state
    pub fn current_state(&self) -> &SimulationState {
        &self.state
    }
    
    /// Check if a transition is valid
    pub fn can_transition(&self, event: &Event) -> bool {
        let key = match (&self.state, event) {
            (SimulationState::Error(_), _) => (SimulationState::Error(String::new()), event.clone()),
            (_, Event::Error(_)) => (self.state.clone(), Event::Error(String::new())),
            _ => (self.state.clone(), event.clone()),
        };
        self.transitions.contains_key(&key)
    }
    
    /// Perform a state transition
    pub fn transition(&mut self, event: Event) -> SimulationResult<()> {
        let key = match (&self.state, &event) {
            (SimulationState::Error(_), _) => (SimulationState::Error(String::new()), event.clone()),
            (_, Event::Error(_)) => (self.state.clone(), Event::Error(String::new())),
            _ => (self.state.clone(), event.clone()),
        };
        
        if let Some(new_state) = self.transitions.get(&key).cloned() {
            // Handle error state specially to preserve message
            self.state = match (&event, new_state) {
                (Event::Error(msg), SimulationState::Error(_)) => SimulationState::Error(msg.clone()),
                (_, state) => state,
            };
            
            tracing::info!("State transition: {} -> {}", key.0, self.state);
            Ok(())
        } else {
            Err(SimulationError::StateError(
                format!("Invalid transition: {:?} from state {:?}", event, self.state)
            ))
        }
    }
    
    /// Check if simulation is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, SimulationState::Completed | SimulationState::Error(_))
    }
    
    /// Check if simulation can be started
    pub fn can_start(&self) -> bool {
        matches!(self.state, SimulationState::Idle)
    }
    
    /// Check if simulation is running
    pub fn is_running(&self) -> bool {
        matches!(self.state, SimulationState::Running | SimulationState::Stepping)
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_transitions() {
        let mut sm = StateMachine::new();
        
        assert_eq!(sm.current_state(), &SimulationState::Idle);
        
        // Start -> Initialize
        sm.transition(Event::Start).unwrap();
        assert_eq!(sm.current_state(), &SimulationState::Initializing);
        
        // Initialize -> Run
        sm.transition(Event::Run).unwrap();
        assert_eq!(sm.current_state(), &SimulationState::Running);
        
        // Run -> Pause
        sm.transition(Event::Pause).unwrap();
        assert_eq!(sm.current_state(), &SimulationState::Paused);
        
        // Pause -> Resume
        sm.transition(Event::Resume).unwrap();
        assert_eq!(sm.current_state(), &SimulationState::Running);
        
        // Run -> Complete
        sm.transition(Event::Complete).unwrap();
        assert_eq!(sm.current_state(), &SimulationState::Completed);
    }
    
    #[test]
    fn test_invalid_transition() {
        let mut sm = StateMachine::new();
        
        // Can't go from Idle to Running directly
        assert!(sm.transition(Event::Run).is_err());
        
        // Can't pause when not running
        assert!(sm.transition(Event::Pause).is_err());
    }
    
    #[test]
    fn test_error_handling() {
        let mut sm = StateMachine::new();
        
        sm.transition(Event::Start).unwrap();
        sm.transition(Event::Run).unwrap();
        
        // Transition to error state
        sm.transition(Event::Error("Test error".to_string())).unwrap();
        assert!(matches!(sm.current_state(), SimulationState::Error(msg) if msg == "Test error"));
        
        // Can reset from error
        sm.transition(Event::Reset).unwrap();
        assert_eq!(sm.current_state(), &SimulationState::Idle);
    }
}