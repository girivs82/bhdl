//! Basic simulation integration test

use bhdl_sim::{SimulationEngine, SimulationConfig};
use bhdl_netlist::Netlist;

#[tokio::test]
async fn test_basic_simulation_setup() {
    // Create a simple netlist
    let netlist = Netlist::new();

    // Create simulation config
    let config = SimulationConfig::fast();

    // Create engine (current API takes the netlist directly)
    let engine = SimulationEngine::new(config, netlist, "test".to_string()).unwrap();

    // Check initial state
    assert_eq!(engine.current_time(), 0.0);
    assert!(!engine.state().is_terminal());
}

#[test]
fn test_time_manager() {
    use bhdl_sim::engine::time::TimeManager;
    
    let mut tm = TimeManager::new(1e-6); // 1 microsecond
    
    assert_eq!(tm.current_time(), 0.0);
    assert_eq!(tm.time_step(), 1e-6);
    
    // Advance time
    let dt = tm.advance();
    assert_eq!(dt, 1e-6);
    assert_eq!(tm.current_time(), 1e-6);
}

#[test]
fn test_state_machine() {
    use bhdl_sim::engine::state::{StateMachine, Event};
    
    let mut sm = StateMachine::new();
    
    // Valid transition sequence
    assert!(sm.transition(Event::Start).is_ok());
    assert!(sm.transition(Event::Run).is_ok());
    assert!(sm.is_running());
}

#[test]
fn test_configuration() {
    let config = SimulationConfig::default();
    assert!(config.validate().is_ok());
    
    let fast_config = SimulationConfig::fast();
    assert!(fast_config.validate().is_ok());
    
    let precise_config = SimulationConfig::precise();
    assert!(precise_config.validate().is_ok());
}