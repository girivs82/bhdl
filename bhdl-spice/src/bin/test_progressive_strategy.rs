//! Test the progressive turn-on strategy directly

use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use bhdl_spice::intelligent_engine::patterns::{CircuitPattern, PatternMatcher};
use bhdl_spice::intelligent_engine::topology_analyzer::TopologyAnalyzer;
use bhdl_spice::intelligent_engine::strategies::{SolvingStrategy, progressive::ProgressiveTurnOnStrategy};

fn main() {
    println!("Testing Progressive Turn-On Strategy");
    
    // Create a test circuit with 2 LEDs in series
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    // Add components
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        330.0,
        None,
    );
    
    circuit.add_branch(
        "LED1".to_string(),
        "n1",
        "n2",
        "LED".to_string(),
        2.0,
        None,
    );
    
    circuit.add_branch(
        "LED2".to_string(),
        "n2",
        "gnd",
        "LED".to_string(),
        2.0,
        None,
    );
    
    // Create solver
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.01),
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits {
            max_current: Some(0.03),
            ..Default::default()
        },
    });
    
    solver.add_model("LED2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits {
            max_current: Some(0.03),
            ..Default::default()
        },
    });
    
    // First try standard solver
    println!("\n1. Testing standard Two-Phase solver:");
    match solver.analyze_simple() {
        Ok(results) => {
            println!("  ✓ Standard solver succeeded with {} solutions", results.len());
            if let Some(result) = results.first() {
                println!("    Power: {:.3} mW", result.total_power * 1000.0);
            }
        },
        Err(e) => {
            println!("  ✗ Standard solver failed: {}", e);
        }
    }
    
    // Now test progressive strategy
    println!("\n2. Testing Progressive Turn-On Strategy:");
    
    // Identify patterns
    let analyzer = TopologyAnalyzer::new();
    let patterns = analyzer.identify_patterns(solver.get_circuit());
    
    println!("  Found {} patterns:", patterns.len());
    for pattern in &patterns {
        println!("    - {}", pattern.name());
    }
    
    // Apply progressive strategy
    if let Some(series_pattern) = patterns.iter().find(|p| matches!(p, CircuitPattern::SeriesNonlinear { .. })) {
        let strategy = ProgressiveTurnOnStrategy::new();
        let context = bhdl_spice::intelligent_engine::strategies::SolverContext {
            previous_solutions: Vec::new(),
            temperature: 25.0,
            convergence_history: Vec::new(),
            user_hints: std::collections::HashMap::new(),
            synthesizer_context: None,
        };
        
        println!("\n  Applying progressive turn-on to: {}", series_pattern.name());
        
        match strategy.solve(&mut solver, series_pattern, &context) {
            Ok(results) => {
                println!("  ✓ Progressive strategy succeeded with {} solutions", results.len());
                if let Some(result) = results.first() {
                    println!("    Power: {:.3} mW", result.total_power * 1000.0);
                    println!("    Iterations: {}", result.iterations);
                }
            },
            Err(e) => {
                println!("  ✗ Progressive strategy failed: {}", e);
            }
        }
    } else {
        println!("  No series pattern found!");
    }
}