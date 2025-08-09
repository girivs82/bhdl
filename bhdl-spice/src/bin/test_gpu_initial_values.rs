//! Test GPU solver initial values and first iteration

use std::sync::Arc;
use std::collections::HashMap;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_gpu::{GpuContext, gpu_data::*},
};

fn main() {
    println!("\n=== GPU INITIAL VALUES TEST ===\n");
    
    // Create simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    // Convert circuit to GPU format
    let mut converter = GpuCircuitConverter::new();
    let (circuit_data, components, variables) = converter.convert_with_models(&circuit, &models);
    
    println!("Initial GPU variables:");
    for (i, var) in variables.iter().enumerate() {
        let var_type = match var.var_type {
            0 => "Voltage",
            1 => "Current",
            _ => "Unknown",
        };
        let space = match var.space {
            0 => "Linear",
            1 => "Log",
            _ => "Unknown",
        };
        
        // Calculate actual value
        let actual_value = if var.space == 1 {
            // Log space
            (var.value as f64).exp()
        } else {
            // Linear space - denormalize
            var.value as f64 * var.scale_factor as f64
        };
        
        println!("  [{}] {} {} idx={}", i, var_type, space, var.index);
        println!("      Raw value: {}", var.value);
        println!("      Scale: 10^{} = {}", var.scale_exponent, var.scale_factor);
        println!("      Actual value: {:.6e}", actual_value);
    }
    
    // Calculate what the initial residuals would be
    println!("\nInitial residuals (CPU calculation):");
    
    // Node voltages (denormalized)
    let v0 = variables[0].value * variables[0].scale_factor; // VCC
    let v1 = variables[1].value * variables[1].scale_factor; // LED_A
    let v2 = 0.0; // GND
    
    // Currents
    let i_vsource = variables[2].value; // Voltage source current
    let i_led_log = variables[3].value; // LED current in log space
    let i_led = i_led_log.exp();
    
    println!("  V(VCC) = {:.6} V", v0);
    println!("  V(LED_A) = {:.6} V", v1);
    println!("  V(GND) = {:.6} V", v2);
    println!("  I(V1) = {:.6e} A", i_vsource);
    println!("  I(LED) = {:.6e} A (log={:.6})", i_led, i_led_log);
    
    // Calculate residuals
    println!("\nResiduals:");
    
    // KCL at VCC: I_vsource - I_resistor = 0
    let i_resistor = (v0 - v1) / 330.0;
    let kcl_vcc = i_vsource - i_resistor;
    println!("  KCL(VCC): {:.6e} (I_vsource={:.6e}, I_res={:.6e})", kcl_vcc, i_vsource, i_resistor);
    
    // KCL at LED_A: I_resistor - I_LED = 0
    let kcl_led_a = i_resistor - i_led;
    println!("  KCL(LED_A): {:.6e}", kcl_led_a);
    
    // Voltage source constraint: V(VCC) - V(GND) - 5*ramp = 0
    let ramp = 0.0; // First test point
    let v_constraint = v0 - v2 - 5.0 * ramp;
    println!("  V_constraint: {:.6e} (ramp={})", v_constraint, ramp);
    
    // LED constraint
    let v_led = v1 - v2;
    let is_sat = 1e-12;
    let n = 2.0;
    let vt = 0.026;
    let exp_arg = v_led / (n * vt);
    let model_current = is_sat * (exp_arg.exp() - 1.0);
    
    println!("  LED voltage: {:.6} V", v_led);
    println!("  LED model current: {:.6e} A", model_current);
    println!("  LED residual (I - I_model): {:.6e}", i_led - model_current);
    
    // Check for potential issues
    println!("\nPotential issues:");
    if v0.abs() < 1e-10 {
        println!("  WARNING: VCC voltage is near zero!");
    }
    if exp_arg > 50.0 {
        println!("  WARNING: LED exp argument is very large: {}", exp_arg);
    }
    if exp_arg < -50.0 {
        println!("  WARNING: LED exp argument is very negative: {}", exp_arg);
    }
}

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}