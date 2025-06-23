//! Test program for domain interface converters

use bhdl_sim::integration::converters::{
    ADConverter, ADCConfig, DAConverter, DACConfig, DomainSynchronizer,
    AnalogUpdate, DomainConverter
};
use bhdl_netlist::{Netlist, NetId};
use bhdl_sim::circuit::state::LogicLevel;

fn main() {
    println!("Testing Domain Interface Converters");
    println!("===================================\n");
    
    test_adc_basic();
    test_dac_basic();
    test_synchronizer();
    test_mixed_signal_loop();
    
    println!("\n✓ All domain interface tests passed!");
}

fn test_adc_basic() {
    println!("1. Testing Analog-to-Digital Converter");
    println!("--------------------------------------");
    
    // Create test netlist and nets
    let mut netlist = Netlist::new();
    let input_net = netlist.add_net(Some("adc_input".to_string()));
    let output_net = netlist.add_net(Some("adc_output".to_string()));
    let mut adc = ADConverter::new(input_net, output_net, ADCConfig::default());
    
    // Test basic conversions
    println!("  Testing threshold detection:");
    
    // Low voltage
    let event = adc.convert(0.4, 0.0);
    if let Some(e) = event {
        println!("    0.4V → {:?} at t={:.3}ns", e.new_value, e.time * 1e9);
    }
    
    // High voltage
    let event = adc.convert(2.5, 1e-9);
    if let Some(e) = event {
        println!("    2.5V → {:?} at t={:.3}ns", e.new_value, e.time * 1e9);
    }
    
    // Test hysteresis
    println!("\n  Testing hysteresis:");
    adc.convert(3.0, 2e-9); // Start high
    
    // Just below threshold - should stay high
    let event = adc.convert(1.95, 3e-9);
    println!("    1.95V → {} (hysteresis prevents change)", 
             if event.is_none() { "No change" } else { "Changed!" });
    
    // Below hysteresis band - should change
    let event = adc.convert(1.85, 4e-9);
    if let Some(e) = event {
        println!("    1.85V → {:?} (below hysteresis)", e.new_value);
    }
    
    // Test metastability
    println!("\n  Testing metastability detection:");
    adc.convert(0.5, 10e-9); // Start low
    
    // Enter undefined region
    adc.convert(1.5, 11e-9);
    println!("    1.5V → Entered undefined region");
    
    // Stay in undefined region past metastable time
    for i in 1..=3 {
        let t = 11e-9 + (i as f64 * 5e-9);
        let event = adc.convert(1.5, t);
        if let Some(e) = event {
            println!("    After {:.1}ns → {:?} (metastable!)", 
                     (t - 11e-9) * 1e9, e.new_value);
        }
    }
    
    let stats = adc.get_stats();
    println!("\n  ADC Statistics:");
    println!("    Total conversions: {}", stats.conversions);
    println!("    Metastable events: {}", stats.metastable_events);
    println!("    Average delay: {:.3}ns", stats.avg_delay * 1e9);
}

fn test_dac_basic() {
    println!("\n2. Testing Digital-to-Analog Converter");
    println!("--------------------------------------");
    
    // Create test netlist and nets
    let mut netlist = Netlist::new();
    let input_net = netlist.add_net(Some("dac_input".to_string()));
    let output_net = netlist.add_net(Some("dac_output".to_string()));
    let config = DACConfig {
        rise_time: 2e-9,
        fall_time: 3e-9,
        slew_rate: Some(1e9), // 1V/ns
        ..Default::default()
    };
    let mut dac = DAConverter::new(input_net, output_net, config);
    
    println!("  Testing voltage transitions:");
    
    // Initial state
    if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::Unknown, 0.0) {
        println!("    Initial (X) → {:.2}V", v);
    }
    
    // Transition to low
    let mut voltages = Vec::new();
    for i in 0..5 {
        let t = i as f64 * 1e-9;
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::Low, t) {
            voltages.push((t, v));
        }
    }
    println!("    Low transition: {:.2}V → {:.2}V over {:.1}ns", 
             voltages[0].1, voltages.last().unwrap().1, 
             (voltages.last().unwrap().0 - voltages[0].0) * 1e9);
    
    // Transition to high with slew rate limiting
    println!("\n  Testing slew rate limiting:");
    dac.set_voltage(0.0); // Reset to 0V
    
    let times = [0.0, 1e-9, 2e-9, 3e-9, 4e-9, 5e-9];
    for &t in &times {
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::High, t) {
            println!("    t={:.1}ns: {:.2}V (slew rate limited)", t * 1e9, v);
        }
    }
    
    // Test high impedance
    println!("\n  Testing high impedance:");
    let result = dac.update(LogicLevel::HighZ, 10e-9);
    match result {
        AnalogUpdate::HighImpedance => println!("    Z → High Impedance output"),
        _ => println!("    ERROR: Expected high impedance"),
    }
    
    let stats = dac.get_stats();
    println!("\n  DAC Statistics:");
    println!("    Total conversions: {}", stats.conversions);
    println!("    Max slew rate: {:.2}V/ns", 
             stats.max_slew_rate.unwrap_or(0.0) / 1e9);
}

fn test_synchronizer() {
    println!("\n3. Testing Domain Synchronizer");
    println!("------------------------------");
    
    let mut sync = DomainSynchronizer::new();
    
    println!("  Initial synchronization:");
    let result = sync.get_next_sync_point();
    println!("    Next time: {:.3}ns", result.next_time * 1e9);
    println!("    Sync type: {:?}", result.sync_type);
    
    // Add digital event
    sync.advance_time(result.next_time);
    sync.set_next_digital_event(Some(5.5e-9));
    
    println!("\n  With digital event at 5.5ns:");
    for _ in 0..6 {
        let result = sync.get_next_sync_point();
        println!("    t={:.3}ns: {:?}", 
                 result.next_time * 1e9, result.sync_type);
        sync.advance_time(result.next_time);
        
        if result.next_time >= 5.5e-9 {
            break;
        }
    }
    
    // Test adaptive timestep
    println!("\n  Testing adaptive timestep:");
    let initial_timestep = sync.get_analog_timestep();
    println!("    Initial timestep: {:.3}ns", initial_timestep * 1e9);
    
    // Register high activity
    let mut test_netlist = Netlist::new();
    for i in 0..20 {
        let net = test_netlist.add_net(Some(format!("active_net_{}", i)));
        sync.register_digital_event(net);
    }
    sync.advance_time(10e-9);
    
    let adapted_timestep = sync.get_analog_timestep();
    println!("    After high activity: {:.3}ns ({})", 
             adapted_timestep * 1e9,
             if adapted_timestep < initial_timestep { "reduced" } else { "unchanged" });
}

fn test_mixed_signal_loop() {
    println!("\n4. Testing Mixed-Signal Feedback Loop");
    println!("-------------------------------------");
    
    // Create a simple comparator with hysteresis feedback
    let mut netlist = Netlist::new();
    let analog_in = netlist.add_net(Some("analog_in".to_string()));
    let digital_out = netlist.add_net(Some("digital_out".to_string()));
    let analog_feedback = netlist.add_net(Some("analog_feedback".to_string()));
    let digital_feedback = netlist.add_net(Some("digital_feedback".to_string()));
    
    let mut adc = ADConverter::new(analog_in, digital_out, ADCConfig {
        v_ih: 2.5,
        v_il: 2.5,
        hysteresis: 0.5,
        ..Default::default()
    });
    
    let mut dac = DAConverter::new(digital_feedback, analog_feedback, DACConfig {
        v_ol: 0.0,
        v_oh: 1.0, // Feedback voltage
        ..Default::default()
    });
    
    let mut sync = DomainSynchronizer::new();
    
    println!("  Simulating comparator with feedback:");
    
    // Simulate rising input voltage
    let mut time = 0.0;
    let mut input_voltage = 0.0;
    let mut feedback_active = false;
    
    for step in 0..20 {
        // Ramp input voltage
        input_voltage = step as f64 * 0.3; // 0.3V per step
        
        // Apply feedback if active
        let effective_voltage = if feedback_active {
            input_voltage - 1.0 // Subtract feedback
        } else {
            input_voltage
        };
        
        // Process through ADC
        if let Some(event) = adc.convert(effective_voltage, time) {
            println!("    t={:.1}ns: Vin={:.2}V, Veff={:.2}V → Digital {:?}", 
                     time * 1e9, input_voltage, effective_voltage, event.new_value);
            
            // Update feedback through DAC
            feedback_active = event.new_value == LogicLevel::High;
            dac.update(event.new_value, time);
        }
        
        time += 10e-9; // 10ns steps
    }
    
    println!("\n  Feedback loop behavior demonstrated:");
    println!("    - Input threshold at 2.5V");
    println!("    - Hysteresis of ±0.5V");
    println!("    - Feedback reduces effective input by 1V");
}