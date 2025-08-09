//! Test Two-Phase solver on realistic BHDL circuits with available components

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use std::time::Instant;

#[derive(Debug)]
struct CircuitTestResult {
    name: String,
    component_count: usize,
    node_count: usize,
    converged: bool,
    time_ms: f64,
    error_msg: Option<String>,
}

/// Create a realistic LED driver circuit (simplified)
fn create_led_driver() -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    
    // Create a constant current LED driver with multiple LEDs
    circuit.add_node("VIN".to_string(), None);      // 12V input
    circuit.add_node("REG".to_string(), None);      // Regulated voltage
    circuit.add_node("SENSE".to_string(), None);    // Current sense
    circuit.add_node("LED1_A".to_string(), None);   
    circuit.add_node("LED2_A".to_string(), None);
    circuit.add_node("LED3_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Input
    circuit.add_branch("VIN".to_string(), "VIN", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("CIN".to_string(), "VIN", "GND", "Capacitor".to_string(), 100e-6, None);
    
    // Voltage regulator stage
    circuit.add_branch("REG1".to_string(), "VIN", "REG", "VoltageRegulator".to_string(), 10.0, None);
    circuit.add_branch("CREG".to_string(), "REG", "GND", "Capacitor".to_string(), 10e-6, None);
    
    // LED string with current limiting
    circuit.add_branch("R1".to_string(), "REG", "LED1_A", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("LED1".to_string(), "LED1_A", "SENSE", "LED".to_string(), 0.0, None);
    
    circuit.add_branch("R2".to_string(), "REG", "LED2_A", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("LED2".to_string(), "LED2_A", "SENSE", "LED".to_string(), 0.0, None);
    
    circuit.add_branch("R3".to_string(), "REG", "LED3_A", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("LED3".to_string(), "LED3_A", "SENSE", "LED".to_string(), 0.0, None);
    
    // Current sense resistor
    circuit.add_branch("RSENSE".to_string(), "SENSE", "GND", "Resistor".to_string(), 10.0, None);
    
    // Models
    let models = vec![
        ("VIN".to_string(), ComponentModel::VoltageSource {
            voltage: 12.0,
            internal_resistance: Some(0.1),
        }),
        ("CIN".to_string(), ComponentModel::Capacitor {
            capacitance: 100e-6,
            esr: Some(0.05),
            limits: ElectricalLimits::default(),
        }),
        ("REG1".to_string(), ComponentModel::VoltageRegulator {
            output_voltage: 10.0,
            dropout_voltage: 2.0,
            quiescent_current: 5e-3,
            limits: ElectricalLimits {
                max_current: Some(1.0),
                max_power: Some(10.0),
                ..Default::default()
            },
        }),
        ("CREG".to_string(), ComponentModel::Capacitor {
            capacitance: 10e-6,
            esr: Some(0.1),
            limits: ElectricalLimits::default(),
        }),
        ("R1".to_string(), ComponentModel::Resistor {
            resistance: 100.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }),
        ("R2".to_string(), ComponentModel::Resistor {
            resistance: 100.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }),
        ("R3".to_string(), ComponentModel::Resistor {
            resistance: 100.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }),
        ("LED1".to_string(), ComponentModel::LED {
            color: "white".to_string(),
            forward_voltage: 3.2,
            forward_current: 20e-3,
            dynamic_resistance: 5.0,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(2.5),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
        ("LED2".to_string(), ComponentModel::LED {
            color: "white".to_string(),
            forward_voltage: 3.2,
            forward_current: 20e-3,
            dynamic_resistance: 5.0,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(2.5),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
        ("LED3".to_string(), ComponentModel::LED {
            color: "white".to_string(),
            forward_voltage: 3.2,
            forward_current: 20e-3,
            dynamic_resistance: 5.0,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(2.5),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
        ("RSENSE".to_string(), ComponentModel::Resistor {
            resistance: 10.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }),
    ];
    
    (circuit, models)
}

/// Create a power supply with multiple regulation stages
fn create_power_supply() -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    
    // Multi-output power supply
    circuit.add_node("VIN".to_string(), None);      // 24V input
    circuit.add_node("V12".to_string(), None);      // 12V rail
    circuit.add_node("V5".to_string(), None);       // 5V rail
    circuit.add_node("V3V3".to_string(), None);     // 3.3V rail
    circuit.add_node("LED_A".to_string(), None);    // Power LED
    circuit.add_node("GND".to_string(), None);
    
    // Input
    circuit.add_branch("VIN".to_string(), "VIN", "GND", "VoltageSource".to_string(), 24.0, None);
    circuit.add_branch("CIN".to_string(), "VIN", "GND", "Capacitor".to_string(), 220e-6, None);
    
    // 12V regulation stage
    circuit.add_branch("REG12".to_string(), "VIN", "V12", "VoltageRegulator".to_string(), 12.0, None);
    circuit.add_branch("C12".to_string(), "V12", "GND", "Capacitor".to_string(), 100e-6, None);
    circuit.add_branch("R12".to_string(), "V12", "GND", "Resistor".to_string(), 1000.0, None); // Load
    
    // 5V regulation stage (from 12V)
    circuit.add_branch("REG5".to_string(), "V12", "V5", "VoltageRegulator".to_string(), 5.0, None);
    circuit.add_branch("C5".to_string(), "V5", "GND", "Capacitor".to_string(), 100e-6, None);
    circuit.add_branch("R5".to_string(), "V5", "GND", "Resistor".to_string(), 100.0, None); // Load
    
    // 3.3V regulation stage (from 5V)
    circuit.add_branch("REG3V3".to_string(), "V5", "V3V3", "VoltageRegulator".to_string(), 3.3, None);
    circuit.add_branch("C3V3".to_string(), "V3V3", "GND", "Capacitor".to_string(), 47e-6, None);
    circuit.add_branch("R3V3".to_string(), "V3V3", "GND", "Resistor".to_string(), 330.0, None); // Load
    
    // Power indicator LED on 5V
    circuit.add_branch("RLED".to_string(), "V5", "LED_A", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("LED1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    
    // Models
    let models = vec![
        ("VIN".to_string(), ComponentModel::VoltageSource {
            voltage: 24.0,
            internal_resistance: Some(0.1),
        }),
        ("CIN".to_string(), ComponentModel::Capacitor {
            capacitance: 220e-6,
            esr: Some(0.05),
            limits: ElectricalLimits::default(),
        }),
        ("REG12".to_string(), ComponentModel::VoltageRegulator {
            output_voltage: 12.0,
            dropout_voltage: 2.0,
            quiescent_current: 5e-3,
            limits: ElectricalLimits::default(),
        }),
        ("REG5".to_string(), ComponentModel::VoltageRegulator {
            output_voltage: 5.0,
            dropout_voltage: 2.0,
            quiescent_current: 5e-3,
            limits: ElectricalLimits::default(),
        }),
        ("REG3V3".to_string(), ComponentModel::VoltageRegulator {
            output_voltage: 3.3,
            dropout_voltage: 1.7,
            quiescent_current: 3e-3,
            limits: ElectricalLimits::default(),
        }),
        ("C12".to_string(), ComponentModel::Capacitor {
            capacitance: 100e-6,
            esr: Some(0.1),
            limits: ElectricalLimits::default(),
        }),
        ("C5".to_string(), ComponentModel::Capacitor {
            capacitance: 100e-6,
            esr: Some(0.1),
            limits: ElectricalLimits::default(),
        }),
        ("C3V3".to_string(), ComponentModel::Capacitor {
            capacitance: 47e-6,
            esr: Some(0.1),
            limits: ElectricalLimits::default(),
        }),
        ("R12".to_string(), ComponentModel::Resistor {
            resistance: 1000.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        ("R5".to_string(), ComponentModel::Resistor {
            resistance: 100.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        ("R3V3".to_string(), ComponentModel::Resistor {
            resistance: 330.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        ("RLED".to_string(), ComponentModel::Resistor {
            resistance: 220.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        ("LED1".to_string(), ComponentModel::LED {
            color: "green".to_string(),
            forward_voltage: 2.2,
            forward_current: 10e-3,
            dynamic_resistance: 15.0,
            saturation_current: Some(1e-13),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
    ];
    
    (circuit, models)
}

/// Create a sensor interface circuit
fn create_sensor_interface() -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    
    // Temperature sensor with LED indicators
    circuit.add_node("VCC".to_string(), None);      // 5V supply
    circuit.add_node("SENSOR".to_string(), None);   // Sensor output
    circuit.add_node("DIV1".to_string(), None);     // Divider points
    circuit.add_node("DIV2".to_string(), None);
    circuit.add_node("LED_OK".to_string(), None);   // Status LEDs
    circuit.add_node("LED_WARN".to_string(), None);
    circuit.add_node("LED_ERR".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Power supply
    circuit.add_branch("VCC".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("C1".to_string(), "VCC", "GND", "Capacitor".to_string(), 10e-6, None);
    
    // Sensor model (voltage divider simulating thermistor)
    circuit.add_branch("RPULL".to_string(), "VCC", "SENSOR", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("RTHERM".to_string(), "SENSOR", "GND", "Resistor".to_string(), 10000.0, None);
    
    // Reference voltage dividers for comparison
    circuit.add_branch("RREF1".to_string(), "VCC", "DIV1", "Resistor".to_string(), 6800.0, None);
    circuit.add_branch("RREF2".to_string(), "DIV1", "DIV2", "Resistor".to_string(), 2200.0, None);
    circuit.add_branch("RREF3".to_string(), "DIV2", "GND", "Resistor".to_string(), 1000.0, None);
    
    // LED indicators with protection diodes
    circuit.add_branch("ROK".to_string(), "VCC", "LED_OK", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("LED_G".to_string(), "LED_OK", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D1".to_string(), "GND", "LED_OK", "Diode".to_string(), 0.0, None); // Protection
    
    circuit.add_branch("RWARN".to_string(), "VCC", "LED_WARN", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("LED_Y".to_string(), "LED_WARN", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "GND", "LED_WARN", "Diode".to_string(), 0.0, None);
    
    circuit.add_branch("RERR".to_string(), "VCC", "LED_ERR", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("LED_R".to_string(), "LED_ERR", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "GND", "LED_ERR", "Diode".to_string(), 0.0, None);
    
    // Filtering capacitors
    circuit.add_branch("CFILT".to_string(), "SENSOR", "GND", "Capacitor".to_string(), 100e-9, None);
    
    // Models
    let models = vec![
        ("VCC".to_string(), ComponentModel::VoltageSource {
            voltage: 5.0,
            internal_resistance: Some(0.01),
        }),
        ("C1".to_string(), ComponentModel::Capacitor {
            capacitance: 10e-6,
            esr: Some(0.1),
            limits: ElectricalLimits::default(),
        }),
        ("CFILT".to_string(), ComponentModel::Capacitor {
            capacitance: 100e-9,
            esr: Some(1.0),
            limits: ElectricalLimits::default(),
        }),
        ("RPULL".to_string(), ComponentModel::Resistor {
            resistance: 10000.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }),
        ("RTHERM".to_string(), ComponentModel::Resistor {
            resistance: 10000.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        ("RREF1".to_string(), ComponentModel::Resistor {
            resistance: 6800.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }),
        ("RREF2".to_string(), ComponentModel::Resistor {
            resistance: 2200.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }),
        ("RREF3".to_string(), ComponentModel::Resistor {
            resistance: 1000.0,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }),
        ("ROK".to_string(), ComponentModel::Resistor {
            resistance: 470.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        ("RWARN".to_string(), ComponentModel::Resistor {
            resistance: 470.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        ("RERR".to_string(), ComponentModel::Resistor {
            resistance: 470.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        ("LED_G".to_string(), ComponentModel::LED {
            color: "green".to_string(),
            forward_voltage: 2.2,
            forward_current: 10e-3,
            dynamic_resistance: 15.0,
            saturation_current: Some(1e-13),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
        ("LED_Y".to_string(), ComponentModel::LED {
            color: "yellow".to_string(),
            forward_voltage: 2.0,
            forward_current: 10e-3,
            dynamic_resistance: 12.0,
            saturation_current: Some(1e-13),
            emission_coefficient: Some(1.9),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
        ("LED_R".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 1.8,
            forward_current: 10e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-13),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
        ("D1".to_string(), ComponentModel::Diode {
            forward_voltage: 0.7,
            forward_resistance: 1.0,
            reverse_current: 1e-12,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.0),
            limits: ElectricalLimits::default(),
        }),
        ("D2".to_string(), ComponentModel::Diode {
            forward_voltage: 0.7,
            forward_resistance: 1.0,
            reverse_current: 1e-12,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.0),
            limits: ElectricalLimits::default(),
        }),
        ("D3".to_string(), ComponentModel::Diode {
            forward_voltage: 0.7,
            forward_resistance: 1.0,
            reverse_current: 1e-12,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.0),
            limits: ElectricalLimits::default(),
        }),
    ];
    
    (circuit, models)
}

fn test_circuit(name: &str, circuit: Circuit, models: Vec<(String, ComponentModel)>) -> CircuitTestResult {
    println!("\nTesting: {}", name);
    println!("  Components: {}", models.len());
    println!("  Nodes: {}", circuit.nodes().count());
    
    let mut solver = GlacierSolver::new(circuit.clone());
    
    // Add models
    for (model_name, model) in &models {
        solver.add_model(model_name.clone(), model.clone());
    }
    
    let start = Instant::now();
    
    match solver.analyze() {
        Ok(_) => {
            let elapsed = start.elapsed();
            println!("  ✅ Converged in {:.1}ms", elapsed.as_secs_f64() * 1000.0);
            
            CircuitTestResult {
                name: name.to_string(),
                component_count: models.len(),
                node_count: circuit.nodes().count(),
                converged: true,
                time_ms: elapsed.as_secs_f64() * 1000.0,
                error_msg: None,
            }
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!("  ❌ Failed: {}", e);
            
            CircuitTestResult {
                name: name.to_string(),
                component_count: models.len(),
                node_count: circuit.nodes().count(),
                converged: false,
                time_ms: elapsed.as_secs_f64() * 1000.0,
                error_msg: Some(e.to_string()),
            }
        }
    }
}

fn main() -> Result<()> {
    println!("=== Realistic BHDL Circuit Convergence Test ===");
    println!("\nTesting Two-Phase solver on practical board-level circuits\n");
    
    let mut results = Vec::new();
    
    // Test each realistic circuit
    let test_circuits = vec![
        ("LED Driver (3 parallel)", create_led_driver()),
        ("Multi-rail Power Supply", create_power_supply()),
        ("Sensor Interface with LEDs", create_sensor_interface()),
    ];
    
    for (name, (circuit, models)) in test_circuits {
        results.push(test_circuit(name, circuit, models));
    }
    
    // Summary
    println!("\n\n=== SUMMARY ===");
    
    let total = results.len();
    let converged = results.iter().filter(|r| r.converged).count();
    let total_components: usize = results.iter().map(|r| r.component_count).sum();
    let avg_components = total_components as f64 / total as f64;
    let avg_time: f64 = results.iter().map(|r| r.time_ms).sum::<f64>() / total as f64;
    
    println!("\nCircuit Complexity:");
    println!("  Average components per circuit: {:.0}", avg_components);
    println!("  Average nodes per circuit: {:.0}", 
             results.iter().map(|r| r.node_count).sum::<usize>() as f64 / total as f64);
    
    println!("\nConvergence Results:");
    for result in &results {
        println!("  {}: {} ({} components, {} nodes, {:.1}ms)",
                 result.name,
                 if result.converged { "✅" } else { "❌" },
                 result.component_count,
                 result.node_count,
                 result.time_ms);
    }
    
    println!("\nOverall Performance:");
    println!("  Success rate: {}/{} ({:.0}%)", converged, total, 
             converged as f64 / total as f64 * 100.0);
    println!("  Average time: {:.1}ms", avg_time);
    
    println!("\n\n=== PRACTICAL ASSESSMENT ===");
    
    let our_rate = converged as f64 / total as f64 * 100.0;
    
    if our_rate >= 90.0 {
        println!("\n✅ EXCELLENT: Two-Phase solver handles realistic circuits well!");
    } else if our_rate >= 75.0 {
        println!("\n✓ GOOD: Two-Phase solver shows promise but needs improvement");
    } else {
        println!("\n⚠️  NEEDS WORK: Two-Phase solver struggles with realistic circuits");
    }
    
    println!("\nKey Observations:");
    println!("• Mixed linear/nonlinear circuits: {}", 
             if results.iter().any(|r| r.name.contains("Power") && r.converged) { "✓ Handled" } else { "✗ Issues" });
    println!("• Multiple LEDs in circuit: {}", 
             if results.iter().any(|r| r.name.contains("LED") && r.converged) { "✓ Handled" } else { "✗ Issues" });
    println!("• Complex node counts (>8): {}", 
             if results.iter().any(|r| r.node_count > 8 && r.converged) { "✓ Handled" } else { "✗ Issues" });
    
    println!("\n\nCOMPARISON TO INDUSTRY:");
    println!("Typical SPICE convergence rates for board-level:");
    println!("  • Simple mixed-signal: 95-99%");
    println!("  • Complex power supplies: 85-95%");
    println!("  • LED drivers: 80-90%");
    println!("  • Sensor interfaces: 90-95%");
    
    println!("\nOur Two-Phase solver: {:.0}%", our_rate);
    
    if our_rate >= 85.0 {
        println!("\n✅ VERDICT: Two-Phase solver is suitable for BHDL!");
        println!("The predictable performance (avg {:.1}ms) makes it ideal", avg_time);
        println!("for interactive circuit development workflows.");
    } else {
        println!("\n⚠️  VERDICT: Consider hybrid approach");
        println!("Use Two-Phase for interactive editing, but offer");
        println!("industry-standard solver as fallback option.");
    }
    
    Ok(())
}