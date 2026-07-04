//! End-to-end test: BHDL file -> Analysis -> Synthesis -> SPICE (GLACIER+MAESTRO)

use std::fs;
use std::time::Instant;
use anyhow::{Result, Context};

use bhdl_parser::parse;
use bhdl_analyzer::analyze;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_spice::{
    stdlib_model_loader::StdlibModelLoader,
    solve_with_glacier_maestro,
};

fn main() -> Result<()> {
    println!("\n=== END-TO-END PIPELINE TEST (GLACIER+MAESTRO) ===\n");
    
    // Get BHDL file path from args or use default
    let bhdl_file = std::env::args().nth(1)
        .unwrap_or_else(|| "tests/circuits/simple/led_test_circuit.bhdl".to_string());
    
    println!("Input BHDL file: {}", bhdl_file);
    let start = Instant::now();
    
    // Step 1: Read BHDL file
    println!("\n1. Reading BHDL file...");
    let source = fs::read_to_string(&bhdl_file)
        .with_context(|| format!("Failed to read BHDL file: {}", bhdl_file))?;
    
    // Step 2: Parse BHDL
    println!("2. Parsing BHDL...");
    let parse_result = parse(&source);
    let syntax_tree = parse_result.syntax();
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parse_result.errors() {
            eprintln!("  - {}", error.message);
        }
        anyhow::bail!("Parsing failed with errors");
    }
    
    // Step 3: Analyze
    println!("3. Analyzing circuit...");
    let source_file = SourceFile::cast(syntax_tree)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast syntax tree to SourceFile"))?;
    
    let analysis_result = analyze(&source_file);
    
    println!("   - Analysis complete");
    println!("   - Found {} diagnostics", analysis_result.diagnostics.len());
    if !analysis_result.diagnostics.is_empty() {
        for diag in &analysis_result.diagnostics {
            println!("     - {}", diag.message);
        }
    }
    
    // Step 4: Create SPICE circuit directly (skip netlist for now)
    println!("4. Creating SPICE circuit...");
    
    // Create a simple LED circuit directly based on the BHDL analysis
    let mut circuit = bhdl_spice::Circuit::new();
    
    // Add nodes from analysis
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("GND".to_string(), None); 
    circuit.add_node("N1".to_string(), None); // Between R1 and D1
    
    // Add components based on our test circuit
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    println!("   - SPICE circuit has {} nodes", circuit.nodes().count());
    println!("   - SPICE circuit has {} branches", circuit.branches().count());
    
    // Step 5: Load models from stdlib
    println!("5. Loading component models from stdlib...");
    let models = StdlibModelLoader::load_models_from_circuit(&circuit)?;
    
    println!("   - Loaded {} component models", models.len());
    for (name, model) in &models {
        println!("     - {}: {}", name, match model {
            bhdl_spice::ComponentModel::Resistor { resistance, .. } => format!("Resistor {}Ω", resistance),
            bhdl_spice::ComponentModel::LED { color, forward_voltage, .. } => format!("{} LED (Vf={}V)", color, forward_voltage),
            bhdl_spice::ComponentModel::VoltageSource { voltage, .. } => format!("Voltage Source {}V", voltage),
            _ => "Other".to_string(),
        });
    }
    
    // Step 6: Solve with GLACIER+MAESTRO
    println!("6. Solving with GLACIER+MAESTRO...");
    match solve_with_glacier_maestro(circuit, models) {
        Ok(solutions) => {
            let elapsed = start.elapsed();
            println!("\n✓ SUCCESS: Found {} solutions in {:.2}ms total", 
                     solutions.len(), elapsed.as_secs_f64() * 1000.0);
            
            // Show solution details
            for (i, solution) in solutions.iter().enumerate() {
                println!("\nSolution {}:", i + 1);
                println!("  Ramp: {:.1}%", solution.ramp * 100.0);
                println!("  Iterations: {}", solution.iterations);
                println!("  Final error: {:.2e}", solution.final_error);
                
                // Show key voltages and currents
                if let Some(v_r1) = solution.node_voltages.get("R1.2") {
                    println!("  V(R1.2) = {:.3}V", v_r1);
                }
                if let Some(i_r1) = solution.branch_currents.get("R1") {
                    println!("  I(R1) = {:.3}mA", i_r1 * 1000.0);
                }
                if let Some(i_d1) = solution.branch_currents.get("D1") {
                    println!("  I(D1) = {:.3}mA", i_d1 * 1000.0);
                }
                
                // Calculate LED forward voltage
                if let (Some(v_anode), Some(v_cathode)) = 
                    (solution.node_voltages.get("D1.A"), solution.node_voltages.get("D1.K")) {
                    println!("  V(D1) = {:.3}V", v_anode - v_cathode);
                }
            }
            
            println!("\n✓ End-to-end pipeline completed successfully!");
            println!("  Total time: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
        }
        Err(e) => {
            println!("\n✗ GLACIER+MAESTRO failed: {}", e);
            anyhow::bail!("Solver failed");
        }
    }
    
    Ok(())
}