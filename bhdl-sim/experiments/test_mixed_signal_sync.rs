//! Test program for mixed-signal synchronization

use bhdl_sim::integration::synchronizer::{
    MixedSignalSynchronizer, SyncStrategy, SyncConfig, SyncReason
};
use bhdl_netlist::NetId;
use std::collections::HashMap;

fn main() {
    println!("Testing Mixed-Signal Synchronization Logic");
    println!("=========================================\n");
    
    // Test 1: Basic synchronization setup
    test_basic_sync();
    
    // Test 2: Event-driven synchronization
    test_event_driven_sync();
    
    // Test 3: Adaptive synchronization
    test_adaptive_sync();
    
    println!("\n✓ All synchronization tests completed!");
}

fn test_basic_sync() {
    println!("1. Testing Basic Lock-Step Synchronization:");
    println!("   --------------------------------------");
    
    // Create interface nets
    let interface_nets = vec![
        NetId::default(), // Let's call this VCC
        // NetId::default(), // And this one DATA
    ];
    
    let config = SyncConfig {
        max_sync_interval: 1e-6,  // 1 microsecond
        min_sync_interval: 1e-9,  // 1 nanosecond
        analog_change_threshold: 0.1,
        sync_all_digital_events: true,
        max_lookahead: 1e-3,
    };
    
    let mut sync = MixedSignalSynchronizer::new(SyncStrategy::LockStep, interface_nets)
        .with_config(config);
    
    println!("   ✓ Created lock-step synchronizer");
    println!("   - Max sync interval: 1µs");
    println!("   - Min sync interval: 1ns");
    
    // Simulate some time steps
    let mut current_time = 0.0;
    let mut last_sync = 0.0;
    
    for i in 0..5 {
        current_time = i as f64 * 1e-6;
        
        if sync.needs_sync(current_time, last_sync) {
            let analog_values = create_test_analog_values(current_time);
            let digital_values = create_test_digital_values(current_time);
            
            let result = sync.synchronize(current_time, &analog_values, &digital_values).unwrap();
            println!("   ✓ Sync at t={:.1}µs: {} nets updated", 
                    current_time * 1e6, result.nets_updated.len());
            
            last_sync = current_time;
        }
    }
}

fn test_event_driven_sync() {
    println!("\n2. Testing Event-Driven Synchronization:");
    println!("   ------------------------------------");
    
    let interface_nets = vec![NetId::default()];
    let mut sync = MixedSignalSynchronizer::new(SyncStrategy::EventDriven, interface_nets);
    
    // Register some digital events
    let event_times = vec![1e-6, 2.5e-6, 3.7e-6, 5e-6];
    for &time in &event_times {
        sync.add_digital_event(time);
        println!("   ✓ Registered digital event at t={:.1}µs", time * 1e6);
    }
    
    // Check next sync times
    let mut current = 0.0;
    while let Some(next_time) = sync.next_sync_time(current) {
        if next_time > 6e-6 {
            break;
        }
        println!("   → Next sync scheduled at t={:.1}µs", next_time * 1e6);
        current = next_time + 1e-9;
    }
    
    // Add analog threshold event
    sync.add_analog_event(1.8e-6, NetId::default());
    println!("   ✓ Added analog threshold event at t=1.8µs");
    
    if let Some(next) = sync.next_sync_time(0.0) {
        println!("   → Next sync now at t={:.1}µs (analog event takes priority)", next * 1e6);
    }
}

fn test_adaptive_sync() {
    println!("\n3. Testing Adaptive Synchronization:");
    println!("   ---------------------------------");
    
    let interface_nets = vec![NetId::default()];
    let mut sync = MixedSignalSynchronizer::new(SyncStrategy::Adaptive, interface_nets);
    
    // Simulate a scenario with varying activity
    let mut current_time = 0.0;
    let mut last_sync = 0.0;
    
    // Low activity period
    println!("   Low activity period (0-10µs):");
    for i in 0..10 {
        current_time = i as f64 * 1e-6;
        
        // Few digital events
        if i % 4 == 0 {
            sync.add_digital_event(current_time + 0.5e-6);
        }
        
        if sync.needs_sync(current_time, last_sync) {
            let analog_values = create_test_analog_values(current_time);
            let digital_values = create_test_digital_values(current_time);
            
            let result = sync.synchronize(current_time, &analog_values, &digital_values).unwrap();
            println!("     - Sync at t={:.1}µs", current_time * 1e6);
            last_sync = current_time;
        }
    }
    
    // High activity period
    println!("\n   High activity period (10-15µs):");
    for i in 10..15 {
        current_time = i as f64 * 1e-6;
        
        // Many digital events
        sync.add_digital_event(current_time + 0.1e-6);
        sync.add_digital_event(current_time + 0.2e-6);
        
        if sync.needs_sync(current_time, last_sync) {
            let analog_values = create_test_analog_values(current_time);
            let digital_values = create_test_digital_values(current_time);
            
            let result = sync.synchronize(current_time, &analog_values, &digital_values).unwrap();
            println!("     - Sync at t={:.1}µs", current_time * 1e6);
            last_sync = current_time;
        }
    }
    
    // Print metrics
    println!("\n{}", sync.metrics());
}

// Helper functions to create test values
fn create_test_analog_values(time: f64) -> HashMap<NetId, f64> {
    let mut values = HashMap::new();
    // Simulate a sine wave on the interface net
    values.insert(NetId::default(), 2.5 + 2.5 * (2.0 * std::f64::consts::PI * 1e6 * time).sin());
    values
}

fn create_test_digital_values(time: f64) -> HashMap<NetId, bool> {
    let mut values = HashMap::new();
    // Simulate a square wave
    values.insert(NetId::default(), (time * 1e6) as i32 % 2 == 0);
    values
}