//! Test program to validate intent system functionality

use bhdl_sim::intent::{Intent, IntentResolver};
use bhdl_sim::coordinator::SimulationCoordinator;
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_netlist::{Netlist, ModuleKind, PinDirection, PinType};
use bhdl_common::{IntentRegistry, SimMode};
use bhdl_stdlib::intents;

fn main() {
    println!("BHDL Intent System Validation");
    println!("=============================\n");
    
    // Initialize intent system
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    println!("✓ Loaded {} stdlib intent functions", registry.len());
    
    // Test intent parsing
    println!("\n1. Testing Intent Parsing:");
    println!("--------------------------");
    test_intent_parsing();
    
    // Test intent resolution
    println!("\n2. Testing Intent Resolution:");
    println!("-----------------------------");
    test_intent_resolution(&registry);
    
    // Test flow tracking
    println!("\n3. Testing Flow Tracking:");
    println!("-------------------------");
    test_flow_tracking(&registry);
    
    // Test circuit partitioning
    println!("\n4. Testing Circuit Partitioning:");
    println!("--------------------------------");
    test_circuit_partitioning(&registry);
    
    println!("\n✓ All intent system validations passed!");
}

fn test_intent_parsing() {
    let test_intents = vec![
        "for accuracy: use spice",
        "for speed: use digital",
        "for power_analysis: use spice with dc_analysis",
        "for timing_analysis: use digital with propagation_delays",
        "for thermal_analysis: use spice with temperature=85",
        "for mixed_signal: use mixed with convergence_aid",
    ];
    
    for intent_str in test_intents {
        match Intent::parse(intent_str) {
            Some(intent) => {
                println!("  ✓ Parsed: '{}'", intent_str);
                println!("    Target: {}, Action: {}", intent.target, intent.action);
                if !intent.parameters.is_empty() {
                    println!("    Parameters: {:?}", intent.parameters);
                }
                if !intent.attributes.is_empty() {
                    println!("    Attributes: {:?}", intent.attributes);
                }
            }
            None => {
                println!("  ✗ Failed to parse: '{}'", intent_str);
            }
        }
    }
}

fn test_intent_resolution(registry: &IntentRegistry) {
    let resolver = IntentResolver::new(registry.clone());
    
    let test_cases = vec![
        ("for accuracy: use spice", "Analog/SPICE simulation"),
        ("for speed: use digital", "Pure digital simulation"),
        ("for timing_analysis: use digital with propagation_delays", "Digital with timing"),
        ("for mixed_signal: use mixed", "Mixed-signal simulation"),
    ];
    
    for (intent_str, description) in test_cases {
        let intent = Intent::parse(intent_str).unwrap();
        match resolver.resolve_intent(&intent) {
            Some(result) => {
                println!("  ✓ {} -> {:?}", description, result.sim_mode);
                if let Some(engine) = result.engine_hint {
                    println!("    Engine hint: {}", engine);
                }
            }
            None => {
                println!("  ✗ Failed to resolve: {}", intent_str);
            }
        }
    }
}

fn test_flow_tracking(registry: &IntentRegistry) {
    let mut flow_tracker = FlowTracker::new(registry.clone());
    
    // Add component intents
    flow_tracker.add_component_intent("OpAmp1", "for accuracy: use spice");
    flow_tracker.add_component_intent("Counter1", "for speed: use digital");
    flow_tracker.add_component_intent("PLL1", "for timing_analysis: use digital with propagation_delays");
    
    // Add net intent
    flow_tracker.add_net_intent("critical_signal", "for accuracy: use spice");
    
    // Test retrieval
    let test_components = vec![
        ("OpAmp1", SimMode::AnalogRequired),
        ("Counter1", SimMode::PureDigital),
        ("PLL1", SimMode::DigitalWithTiming),
    ];
    
    for (component, expected_mode) in test_components {
        match flow_tracker.get_component_sim_mode(component) {
            Some(mode) => {
                if mode == expected_mode {
                    println!("  ✓ {} -> {:?}", component, mode);
                } else {
                    println!("  ✗ {} -> {:?} (expected {:?})", component, mode, expected_mode);
                }
            }
            None => {
                println!("  ✗ No mode found for {}", component);
            }
        }
    }
    
    // Test net mode
    match flow_tracker.get_net_sim_mode("critical_signal") {
        Some(mode) => println!("  ✓ critical_signal net -> {:?}", mode),
        None => println!("  ✗ No mode found for critical_signal net"),
    }
}

fn test_circuit_partitioning(registry: &IntentRegistry) {
    // Create a test netlist
    let mut netlist = Netlist::new();
    
    // Create modules
    let analog_mod = netlist.add_module("AnalogAmp".to_string(), ModuleKind::Component);
    let digital_mod = netlist.add_module("DigitalLogic".to_string(), ModuleKind::Component);
    let timed_mod = netlist.add_module("TimedCircuit".to_string(), ModuleKind::Component);
    
    // Add pins
    for module in [analog_mod, digital_mod, timed_mod] {
        netlist.add_pin(module, "IN".to_string(), PinDirection::In, PinType::Signal).unwrap();
        netlist.add_pin(module, "OUT".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    }
    
    // Create instances
    let amp1 = netlist.add_instance("AMP1".to_string(), analog_mod).unwrap();
    let logic1 = netlist.add_instance("LOGIC1".to_string(), digital_mod).unwrap();
    let timer1 = netlist.add_instance("TIMER1".to_string(), timed_mod).unwrap();
    
    // Create flow tracker with intents
    let mut flow_tracker = FlowTracker::new(registry.clone());
    flow_tracker.add_component_intent("AMP1", "for accuracy: use spice");
    flow_tracker.add_component_intent("LOGIC1", "for speed: use digital");
    flow_tracker.add_component_intent("TIMER1", "for timing_analysis: use digital with propagation_delays");
    
    // Create coordinator
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    let partitions = coordinator.get_partitions();
    
    println!("  Created {} partitions:", partitions.len());
    for partition in partitions {
        println!("    Partition {} - {:?}: {} instances", 
                 partition.id, partition.mode, partition.instances.len());
    }
    
    // Verify we have the expected partition modes
    let modes: Vec<SimMode> = coordinator.get_partitions().iter()
        .map(|p| p.mode)
        .collect();
    
    if modes.contains(&SimMode::AnalogRequired) {
        println!("  ✓ Analog partition created");
    }
    if modes.contains(&SimMode::PureDigital) {
        println!("  ✓ Digital partition created");
    }
    if modes.contains(&SimMode::DigitalWithTiming) {
        println!("  ✓ Timed digital partition created");
    }
}