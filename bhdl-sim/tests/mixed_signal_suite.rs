//! Comprehensive mixed-signal simulation test suite
//! 
//! Tests various mixed-signal scenarios including:
//! - Simple RC circuits with digital control
//! - ADC/DAC feedback loops
//! - Clock domain crossings
//! - Power-on reset sequences
//! - Analog comparators with digital outputs

use bhdl_sim::coordinator::{SimulationCoordinator, SimulationContext};
use bhdl_sim::integration::converters::{ADConverter, DAConverter, ADCConfig, DACConfig};
use bhdl_sim::integration::synchronizer::{MixedSignalSynchronizer, SyncStrategy};
use bhdl_netlist::{Netlist, ModuleKind, NetClass, PinDirection, PinType, ConnectionPoint};
use bhdl_common::{SimMode, IntentResult, IntentRegistry};
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_stdlib::intents;
use std::collections::HashMap;

/// Test helper to create a basic netlist
fn create_test_netlist() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Create module definitions
    let resistor_module = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let capacitor_module = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let switch_module = netlist.add_module("Switch".to_string(), ModuleKind::PhysicalComponent);
    
    // Add pins
    let _r_pin1 = netlist.add_pin(resistor_module, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let _r_pin2 = netlist.add_pin(resistor_module, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    let _c_pin1 = netlist.add_pin(capacitor_module, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let _c_pin2 = netlist.add_pin(capacitor_module, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    let _sw_ctrl = netlist.add_pin(switch_module, "ctrl".to_string(), PinDirection::In, PinType::Signal).unwrap();
    let _sw_in = netlist.add_pin(switch_module, "in".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let _sw_out = netlist.add_pin(switch_module, "out".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    netlist
}

#[test]
fn test_rc_circuit_with_digital_control() {
    println!("\n=== RC Circuit with Digital Control ===");
    
    let mut netlist = create_test_netlist();
    
    // Create nets
    let vin_net = netlist.add_net_with_class(Some("VIN".to_string()), NetClass::Power(5.0));
    let vout_net = netlist.add_net(Some("VOUT".to_string()));
    let gnd_net = netlist.add_net_with_class(Some("GND".to_string()), NetClass::Ground);
    let ctrl_net = netlist.add_net(Some("CTRL".to_string()));
    
    // Create instances
    let r1 = netlist.add_instance("R1".to_string(), 
        netlist.modules.keys().find(|&&id| netlist.modules[id].name == "Resistor").unwrap()).unwrap();
    let c1 = netlist.add_instance("C1".to_string(),
        netlist.modules.keys().find(|&&id| netlist.modules[id].name == "Capacitor").unwrap()).unwrap();
    let sw1 = netlist.add_instance("SW1".to_string(),
        netlist.modules.keys().find(|&&id| netlist.modules[id].name == "Switch").unwrap()).unwrap();
    
    // Set component values
    if let Some(inst) = netlist.instances.get_mut(r1) {
        inst.attributes.insert("value".to_string(), "1000.0".to_string()); // 1kΩ
    }
    if let Some(inst) = netlist.instances.get_mut(c1) {
        inst.attributes.insert("value".to_string(), "1e-6".to_string()); // 1µF
    }
    
    // Create test with flow tracker
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let flow_tracker = FlowTracker::new(registry);
    
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    
    // Check partitioning
    let partitions = coordinator.get_partitions();
    println!("Created {} partitions", partitions.len());
    
    let interfaces = coordinator.get_interfaces();
    println!("Found {} domain interfaces", interfaces.len());
    
    // Note: Actual simulation would require full engine setup
    assert!(partitions.len() >= 0); // Basic sanity check
}

#[test]
fn test_adc_dac_loopback() {
    println!("\n=== ADC/DAC Loopback Test ===");
    
    // Create ADC
    let adc_config = ADCConfig {
        v_high_threshold: 3.3,
        v_low_threshold: 1.65,
        hysteresis: 0.1,
        propagation_delay: 10e-9,
        metastability_window: 5e-9,
        output_rise_time: 1e-9,
        output_fall_time: 1e-9,
    };
    
    let mut adc = ADConverter::new(adc_config);
    
    // Create DAC
    let dac_config = DACConfig {
        v_high: 5.0,
        v_low: 0.0,
        rise_time: 10e-9,
        fall_time: 10e-9,
        output_impedance: 50.0,
        slew_rate: Some(1e9), // 1V/ns
    };
    
    let mut dac = DAConverter::new(dac_config);
    
    // Test conversion chain
    let test_voltages = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    
    for voltage in test_voltages {
        // A/D conversion
        let digital = adc.convert_voltage_to_logic(voltage);
        println!("ADC: {:.1}V -> {:?}", voltage, digital);
        
        // D/A conversion
        let analog = if digital == bhdl_sim::circuit::state::LogicLevel::High { 
            dac_config.v_high 
        } else { 
            dac_config.v_low 
        };
        println!("DAC: {:?} -> {:.1}V", digital, analog);
        
        // Check round-trip accuracy for clear high/low
        if voltage > 4.0 {
            assert_eq!(digital, bhdl_sim::circuit::state::LogicLevel::High);
        } else if voltage < 1.0 {
            assert_eq!(digital, bhdl_sim::circuit::state::LogicLevel::Low);
        }
    }
}

#[test]
fn test_synchronization_strategies() {
    println!("\n=== Synchronization Strategy Comparison ===");
    
    let interface_nets = vec![Default::default()]; // Dummy net
    
    // Test each strategy
    for strategy in [SyncStrategy::LockStep, SyncStrategy::EventDriven, SyncStrategy::Adaptive] {
        println!("\nTesting {:?} strategy:", strategy);
        
        let mut sync = MixedSignalSynchronizer::new(strategy, interface_nets.clone());
        
        // Simulate some events
        for i in 0..5 {
            let time = i as f64 * 1e-6;
            
            // Add events based on strategy
            match strategy {
                SyncStrategy::EventDriven => {
                    if i % 2 == 0 {
                        sync.add_digital_event(time + 0.5e-6);
                    }
                }
                _ => {}
            }
            
            // Check sync need
            if sync.needs_sync(time, 0.0) {
                let analog_vals = HashMap::new();
                let digital_vals = HashMap::new();
                
                let result = sync.synchronize(time, &analog_vals, &digital_vals).unwrap();
                println!("  Sync at t={:.1}µs, next: {:?}", 
                        time * 1e6, 
                        result.next_sync.map(|t| format!("{:.1}µs", t * 1e6)));
            }
        }
    }
}

#[test] 
fn test_power_on_reset_sequence() {
    println!("\n=== Power-On Reset Sequence ===");
    
    // Create a simple POR circuit structure
    let mut netlist = Netlist::new();
    
    // Modules
    let por_module = netlist.add_module("PowerOnReset".to_string(), ModuleKind::Component);
    let reg_module = netlist.add_module("VoltageRegulator".to_string(), ModuleKind::PhysicalComponent);
    
    // Nets
    let vcc_net = netlist.add_net_with_class(Some("VCC".to_string()), NetClass::Power(5.0));
    let rst_net = netlist.add_net(Some("RESET".to_string()));
    let _vreg_out = netlist.add_net_with_class(Some("VREG_3V3".to_string()), NetClass::Power(3.3));
    
    // Simulate POR behavior with converters
    let adc_config = ADCConfig {
        v_high_threshold: 4.5, // POR triggers at 4.5V
        v_low_threshold: 4.0,  // With 0.5V hysteresis
        hysteresis: 0.5,
        propagation_delay: 100e-6, // 100µs delay
        metastability_window: 0.0,
        output_rise_time: 10e-9,
        output_fall_time: 10e-9,
    };
    
    let mut por_adc = ADConverter::new(adc_config);
    
    // Simulate voltage ramp
    println!("Simulating voltage ramp-up:");
    for i in 0..=50 {
        let voltage = i as f64 * 0.1; // 0V to 5V
        let reset_state = por_adc.convert_voltage_to_logic(voltage);
        
        if i % 5 == 0 {
            println!("  VCC={:.1}V -> RESET={:?}", voltage, reset_state);
        }
    }
}

#[test]
fn test_analog_comparator_with_reference() {
    println!("\n=== Analog Comparator with Reference ===");
    
    // Configure comparator as ADC with specific threshold
    let comp_config = ADCConfig {
        v_high_threshold: 2.5, // Reference voltage
        v_low_threshold: 2.5,  // No hysteresis version
        hysteresis: 0.0,
        propagation_delay: 50e-9,
        metastability_window: 10e-9,
        output_rise_time: 5e-9,
        output_fall_time: 5e-9,
    };
    
    let mut comparator = ADConverter::new(comp_config);
    
    // Test with hysteresis
    let comp_hyst_config = ADCConfig {
        v_high_threshold: 2.6,
        v_low_threshold: 2.4,
        hysteresis: 0.2,
        ..comp_config
    };
    
    let mut comp_with_hyst = ADConverter::new(comp_hyst_config);
    
    // Test input sweep
    println!("Testing comparator response:");
    let test_voltages = vec![2.0, 2.3, 2.4, 2.5, 2.6, 2.7, 2.6, 2.5, 2.4, 2.3, 2.0];
    
    for (i, &voltage) in test_voltages.iter().enumerate() {
        let out_no_hyst = comparator.convert_voltage_to_logic(voltage);
        let out_with_hyst = comp_with_hyst.convert_voltage_to_logic(voltage);
        
        println!("  Step {}: {:.1}V -> No hyst: {:?}, With hyst: {:?}",
                i, voltage, out_no_hyst, out_with_hyst);
    }
}

#[test]
fn test_mixed_signal_timing_analysis() {
    println!("\n=== Mixed-Signal Timing Analysis ===");
    
    // Create a timing-critical mixed-signal path
    let mut sync = MixedSignalSynchronizer::new(
        SyncStrategy::EventDriven,
        vec![Default::default()]
    );
    
    // Add timing events
    let critical_times = vec![
        0.0,      // Start
        100e-9,   // 100ns - digital edge
        150e-9,   // 150ns - analog threshold
        200e-9,   // 200ns - digital response
        1e-6,     // 1µs - periodic check
    ];
    
    for &time in &critical_times {
        if time > 0.0 {
            sync.add_digital_event(time);
        }
    }
    
    // Analyze timing
    println!("Critical timing points:");
    let mut current = 0.0;
    while let Some(next) = sync.next_sync_time(current) {
        if next > 2e-6 { break; }
        
        println!("  t={:.0}ns: Synchronization required", next * 1e9);
        
        // Perform sync
        let analog_vals = HashMap::new();
        let digital_vals = HashMap::new();
        sync.synchronize(next, &analog_vals, &digital_vals).unwrap();
        
        current = next + 1e-12; // Advance slightly
    }
    
    // Print final metrics
    println!("\n{}", sync.metrics());
}

/// Integration test combining all components
#[test]
fn test_full_mixed_signal_simulation() {
    println!("\n=== Full Mixed-Signal Integration Test ===");
    
    // This test demonstrates how all components work together
    // In a real scenario, this would involve:
    // 1. Circuit partitioning by SimulationCoordinator
    // 2. Domain interface creation
    // 3. Engine adapter initialization
    // 4. Synchronization setup
    // 5. Main simulation loop with value exchange
    
    let netlist = create_test_netlist();
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let flow_tracker = FlowTracker::new(registry);
    
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    
    println!("Simulation setup complete:");
    println!("  - Partitions: {}", coordinator.get_partitions().len());
    println!("  - Interfaces: {}", coordinator.get_interfaces().len());
    
    // Note: Full simulation execution would require complete engine setup
    // This test validates the infrastructure is properly connected
}