//! Comprehensive validation tests for the intent-based simulation system

use bhdl_sim::intent::{Intent, IntentResolver};
use bhdl_sim::coordinator::{SimulationCoordinator, SimPartition};
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_netlist::{Netlist, ModuleKind, PinDirection, PinType, NetClass, ConnectionPoint};
use bhdl_common::{IntentRegistry, SimMode};
use bhdl_stdlib::intents;

/// Test basic intent parsing and validation
#[test]
fn test_intent_validation() {
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    
    // Valid intents
    let valid_intents = vec![
        "for accuracy: use spice",
        "for speed: use digital",
        "for power_analysis: use spice with dc_analysis",
        "for timing_analysis: use digital with propagation_delays",
        "for thermal_analysis: use spice with temperature=85",
    ];
    
    for intent_str in valid_intents {
        let intent = Intent::parse(intent_str);
        assert!(intent.is_some(), "Failed to parse valid intent: {}", intent_str);
        
        let intent = intent.unwrap();
        assert!(!intent.target.is_empty(), "Intent target should not be empty");
        assert!(!intent.parameters.is_empty() || !intent.attributes.is_empty(), 
                "Intent should have parameters or attributes");
    }
    
    // Invalid intents - should not parse
    let invalid_intents = vec![
        "use spice",  // Missing 'for' keyword
        "for: use digital",  // Missing target
        "for accuracy use",  // Missing action
        "for accuracy: digital",  // Missing 'use' keyword
    ];
    
    for intent_str in invalid_intents {
        let intent = Intent::parse(intent_str);
        assert!(intent.is_none(), "Should not parse invalid intent: {}", intent_str);
    }
}

/// Test intent resolution to SimMode
#[test]
fn test_intent_resolution() {
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let resolver = IntentResolver::new(registry);
    
    // Test various intent patterns
    let test_cases = vec![
        ("for accuracy: use spice", SimMode::AnalogRequired),
        ("for speed: use digital", SimMode::PureDigital),
        ("for timing_analysis: use digital with propagation_delays", SimMode::DigitalWithTiming),
        ("for mixed_signal: use mixed", SimMode::MixedSignal),
        ("for behavioral: use behavioral", SimMode::PureDigital), // Behavioral maps to digital
    ];
    
    for (intent_str, expected_mode) in test_cases {
        let intent = Intent::parse(intent_str).expect("Should parse test intent");
        let resolved = resolver.resolve_intent(&intent);
        
        assert!(resolved.is_some(), "Should resolve intent: {}", intent_str);
        let resolved = resolved.unwrap();
        assert_eq!(resolved.sim_mode, expected_mode, 
                   "Intent '{}' should resolve to {:?}", intent_str, expected_mode);
    }
}

/// Test parameter extraction and validation
#[test]
fn test_intent_parameters() {
    let intent_str = "for power_analysis: use spice with temperature=85, tolerance=0.01";
    let intent = Intent::parse(intent_str).expect("Should parse intent with parameters");
    
    assert_eq!(intent.parameters.len(), 2);
    assert_eq!(intent.parameters.get("temperature"), Some(&"85".to_string()));
    assert_eq!(intent.parameters.get("tolerance"), Some(&"0.01".to_string()));
}

/// Test flow tracking with intents
#[test]
fn test_flow_tracking_integration() {
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let mut flow_tracker = FlowTracker::new(registry.clone());
    
    // Add component intent
    flow_tracker.add_component_intent("U1", "for accuracy: use spice");
    
    // Check component sim mode
    let mode = flow_tracker.get_component_sim_mode("U1");
    assert_eq!(mode, Some(SimMode::AnalogRequired));
    
    // Add net intent
    flow_tracker.add_net_intent("critical_path", "for timing_analysis: use digital with propagation_delays");
    
    // Check net sim mode
    let mode = flow_tracker.get_net_sim_mode("critical_path");
    assert_eq!(mode, Some(SimMode::DigitalWithTiming));
}

/// Test circuit partitioning based on intents
#[test]
fn test_intent_based_partitioning() {
    // Create a netlist with mixed components
    let mut netlist = create_mixed_netlist();
    
    // Create flow tracker with intents
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let mut flow_tracker = FlowTracker::new(registry);
    
    // Add different intents for different components
    flow_tracker.add_component_intent("AMP1", "for accuracy: use spice");
    flow_tracker.add_component_intent("LOGIC1", "for speed: use digital");
    flow_tracker.add_component_intent("TIMER1", "for timing_analysis: use digital with propagation_delays");
    
    // Create coordinator and verify partitioning
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    let partitions = coordinator.get_partitions();
    
    // Should have at least 3 partitions (one for each sim mode)
    assert!(partitions.len() >= 3, "Should have multiple partitions based on intents");
    
    // Verify each partition has the correct mode
    let modes: Vec<SimMode> = partitions.iter().map(|p| p.mode).collect();
    assert!(modes.contains(&SimMode::AnalogRequired), "Should have analog partition");
    assert!(modes.contains(&SimMode::PureDigital), "Should have digital partition");
    assert!(modes.contains(&SimMode::DigitalWithTiming), "Should have timed digital partition");
}

/// Test hierarchical intent propagation
#[test]
fn test_hierarchical_intent_propagation() {
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let mut flow_tracker = FlowTracker::new(registry);
    
    // Add module-level intent
    flow_tracker.add_module_intent("PowerSupply", "for accuracy: use spice");
    
    // Components within the module should inherit the intent
    flow_tracker.set_component_module("U1", "PowerSupply");
    flow_tracker.set_component_module("U2", "PowerSupply");
    
    // Both components should have analog mode
    assert_eq!(flow_tracker.get_component_sim_mode("U1"), Some(SimMode::AnalogRequired));
    assert_eq!(flow_tracker.get_component_sim_mode("U2"), Some(SimMode::AnalogRequired));
}

/// Test conflicting intent resolution
#[test]
fn test_conflicting_intent_resolution() {
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let mut flow_tracker = FlowTracker::new(registry);
    
    // Add conflicting intents to connected components
    flow_tracker.add_component_intent("U1", "for speed: use digital");
    flow_tracker.add_component_intent("U2", "for accuracy: use spice");
    
    // Connect them through a net
    flow_tracker.add_net_connection("signal1", vec!["U1", "U2"]);
    
    // The net should adopt the higher priority mode (analog)
    let net_mode = flow_tracker.get_net_sim_mode("signal1");
    assert_eq!(net_mode, Some(SimMode::AnalogRequired), 
               "Net should adopt highest priority mode from connected components");
}

/// Test intent attribute handling
#[test]
fn test_intent_attributes() {
    let intent_str = "for power_analysis: use spice with dc_analysis, transient_analysis";
    let intent = Intent::parse(intent_str).expect("Should parse intent with attributes");
    
    assert_eq!(intent.attributes.len(), 2);
    assert!(intent.attributes.contains(&"dc_analysis".to_string()));
    assert!(intent.attributes.contains(&"transient_analysis".to_string()));
}

/// Helper function to create a mixed netlist
fn create_mixed_netlist() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Create various module types
    let amp_module = netlist.add_module("Amplifier".to_string(), ModuleKind::Component);
    let logic_module = netlist.add_module("LogicGate".to_string(), ModuleKind::Component);
    let timer_module = netlist.add_module("Timer".to_string(), ModuleKind::Component);
    
    // Add pins
    let _amp_in = netlist.add_pin(amp_module, "IN".to_string(), PinDirection::In, PinType::Signal).unwrap();
    let _amp_out = netlist.add_pin(amp_module, "OUT".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    
    let _logic_a = netlist.add_pin(logic_module, "A".to_string(), PinDirection::In, PinType::Signal).unwrap();
    let _logic_y = netlist.add_pin(logic_module, "Y".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    
    let _timer_clk = netlist.add_pin(timer_module, "CLK".to_string(), PinDirection::In, PinType::Clock).unwrap();
    let _timer_out = netlist.add_pin(timer_module, "OUT".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    
    // Create instances
    let amp1 = netlist.add_instance("AMP1".to_string(), amp_module).unwrap();
    let logic1 = netlist.add_instance("LOGIC1".to_string(), logic_module).unwrap();
    let timer1 = netlist.add_instance("TIMER1".to_string(), timer_module).unwrap();
    
    // Create and connect nets
    let sig1 = netlist.add_net(Some("signal1".to_string()));
    let sig2 = netlist.add_net(Some("signal2".to_string()));
    let clk = netlist.add_net(Some("clock".to_string()));
    
    // Connect components
    netlist.connect_instance_pin(amp1, "OUT", sig1).unwrap();
    netlist.connect_instance_pin(logic1, "A", sig1).unwrap();
    netlist.connect_instance_pin(logic1, "Y", sig2).unwrap();
    netlist.connect_instance_pin(timer1, "CLK", clk).unwrap();
    
    netlist
}

/// Test intent priority and override behavior
#[test]
fn test_intent_priority() {
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let mut flow_tracker = FlowTracker::new(registry);
    
    // Add base intent
    flow_tracker.add_component_intent("U1", "for speed: use digital");
    assert_eq!(flow_tracker.get_component_sim_mode("U1"), Some(SimMode::PureDigital));
    
    // Override with higher priority intent
    flow_tracker.add_component_intent("U1", "for accuracy: use spice");
    assert_eq!(flow_tracker.get_component_sim_mode("U1"), Some(SimMode::AnalogRequired),
               "Later intent should override earlier one");
}

/// Test intent parameter type validation
#[test]
fn test_parameter_type_validation() {
    let test_cases = vec![
        ("for analysis: use spice with temperature=85", true),  // Valid number
        ("for analysis: use spice with temperature=85.5", true),  // Valid float
        ("for analysis: use spice with temperature=-40", true),  // Valid negative
        ("for analysis: use spice with enable=true", true),  // Valid boolean
        ("for analysis: use spice with mode=fast", true),  // Valid identifier
    ];
    
    for (intent_str, should_parse) in test_cases {
        let intent = Intent::parse(intent_str);
        assert_eq!(intent.is_some(), should_parse, 
                   "Intent '{}' parse result mismatch", intent_str);
    }
}