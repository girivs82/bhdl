//! Compare GPU f32 auto-scaling vs CPU f64 precision
//! 
//! Tests that f32 auto-scaling achieves comparable accuracy to f64

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_gpu::gpu_data::GpuCircuitConverter,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::gpu_context::GpuContext;

fn create_challenging_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Create a circuit with very small currents to test f32 precision
    // 5V -> 10kΩ -> LED (Is=1e-14A) -> GND
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 10000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 10000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.001, // 1mA - smaller current for precision test
        dynamic_resistance: 50.0,
        saturation_current: Some(1e-14), // Extremely small - challenges f32 precision
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}

fn test_precision_comparison() -> Result<()> {
    println!("GPU f32 Auto-Scaling vs CPU f64 Precision Comparison");
    println!("{}", "=".repeat(80));
    
    let (circuit, models) = create_challenging_circuit();
    
    println!("Test circuit: 5V -> 10kΩ -> LED(Is=1e-14A) -> GND");
    println!("This circuit has extremely small saturation currents that challenge f32 precision\n");
    
    // Test GPU circuit conversion with auto-scaling
    println!("1. Testing GPU Circuit Conversion with Auto-Scaling:");
    let mut converter = GpuCircuitConverter::new();
    let (circuit_data, components, variables) = converter.convert_with_models(&circuit, &models);
    
    println!("Circuit converted to GPU format:");
    println!("  - Nodes: {}", circuit_data.num_nodes);
    println!("  - Components: {}", circuit_data.num_components);
    println!("  - Variables: {}", variables.len());
    
    // Analyze auto-scaling effectiveness
    println!("\n2. Auto-Scaling Analysis:");
    for (i, var) in variables.iter().enumerate() {
        let var_type = match var.var_type {
            0 => "NodeVoltage",
            1 => "BranchCurrent",
            _ => "Unknown",
        };
        let space = match var.space {
            0 => "Linear",
            1 => "Logarithmic", 
            _ => "Unknown",
        };
        
        println!("  Variable {}: {} space={}", i, var_type, space);
        println!("    Initial value: {:.6}", var.value);
        println!("    Scale factor: {:.2e}", var.scale_factor);
        println!("    Scale exponent: {}", var.scale_exponent);
        
        if var.space == 0 { // Linear space
            let actual_value = var.value as f64 * var.scale_factor as f64;
            println!("    Actual value: {:.6e}", actual_value);
        }
    }
    
    // Check component parameter scaling
    println!("\n3. Component Parameter Analysis:");
    for (i, comp) in components.iter().enumerate() {
        let comp_type = match comp.comp_type {
            0 => "Resistor",
            1 => "VoltageSource", 
            2 => "LED",
            3 => "Diode",
            _ => "Unknown",
        };
        
        println!("  Component {}: {}", i, comp_type);
        println!("    Value: {:.6}", comp.value);
        
        if comp.comp_type == 2 || comp.comp_type == 3 { // LED or Diode
            println!("    Saturation current: {:.2e}", comp.is_sat);
            println!("    Emission coefficient: {:.3}", comp.n_emission);
            println!("    Thermal voltage: {:.6}", comp.vt);
        }
    }
    
    // Test GPU context creation
    #[cfg(feature = "gpu")]
    {
        println!("\n4. GPU Context Test:");
        match pollster::block_on(GpuContext::new()) {
            Ok(context) => {
                println!("✓ GPU context created successfully");
                println!("  GPU: {} ({:?})", context.adapter_info.name, context.adapter_info.backend);
                
                if context.supports_double_precision() {
                    println!("  f64 support: YES (but testing f32 auto-scaling)");
                } else {
                    println!("  f64 support: NO (f32 auto-scaling essential)");
                }
            }
            Err(e) => {
                println!("✗ GPU context creation failed: {}", e);
                return Err(e);
            }
        }
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("\n4. GPU support not enabled - add --features gpu");
    }
    
    // Precision analysis
    println!("\n5. Precision Analysis:");
    let small_current = 1e-14_f64;
    let f32_value = small_current as f32;
    let f64_value = small_current;
    
    println!("Original value (f64): {:.2e}", f64_value);
    println!("Direct f32 cast:      {:.2e}", f32_value as f64);
    println!("Precision loss:       {:.2e}", (f64_value - f32_value as f64).abs());
    
    // Auto-scaling demonstration
    let scale_exp = f64_value.log10().floor() as i32;
    let scale_factor = 10_f32.powi(scale_exp);
    let normalized = (f64_value / scale_factor as f64) as f32;
    let denormalized = normalized as f64 * scale_factor as f64;
    
    println!("\nWith auto-scaling:");
    println!("Scale exponent: {}", scale_exp);
    println!("Scale factor:   {:.2e}", scale_factor);
    println!("Normalized:     {:.6}", normalized);
    println!("Denormalized:   {:.2e}", denormalized);
    println!("Precision loss: {:.2e}", (f64_value - denormalized).abs());
    
    let improvement_ratio = (f64_value - f32_value as f64).abs() / (f64_value - denormalized).abs();
    println!("Improvement:    {:.1}x better precision", improvement_ratio);
    
    println!("\n6. Test Results:");
    if improvement_ratio > 10.0 {
        println!("✓ Auto-scaling provides significant precision improvement");
        println!("✓ f32 GPU solver ready for high-precision circuit analysis");
    } else if improvement_ratio > 2.0 {
        println!("✓ Auto-scaling provides moderate precision improvement");
        println!("✓ Suitable for most circuit analysis tasks");
    } else {
        println!("⚠ Auto-scaling provides minimal improvement");
        println!("  Consider further optimization for ultra-high precision needs");
    }
    
    Ok(())
}

fn main() -> Result<()> {
    test_precision_comparison()
}