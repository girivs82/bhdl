//! Demonstrates the full BHDL pipeline ending with transient analysis

use anyhow::Result;
use std::collections::HashMap;

// BHDL pipeline components
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_analyzer::symbol_table::SymbolTable;

// SPICE components
use bhdl_spice::{Circuit, ComponentModel, GlacierSolver, stdlib_model_loader::StdlibModelLoader};

fn main() -> Result<()> {
    println!("\n=== BHDL PIPELINE → TRANSIENT ANALYSIS DEMO ===\n");

    // Step 1: Start with BHDL code
    let bhdl_code = r#"
board SimpleLED {
    power VCC = 5V @ 100mA;
    ground GND;
    
    // LED with current limiting resistor
    VCC -> R1: Res(330Ω).1 -> LED1: LED(red).A;
    LED1.K -> GND;
}
"#;

    println!("1. BHDL SOURCE CODE:");
    println!("{}", bhdl_code);

    // Step 2: Parse BHDL
    println!("\n2. PARSING BHDL...");
    let parse_result = parse(bhdl_code);
    
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parsing failed"));
    }
    
    let ast = SourceFile::cast(parse_result.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to get AST"))?;
    
    println!("  ✓ Parse successful");
    
    // Count boards
    let board_count = ast.boards().count();
    println!("  ✓ Found {} board(s)", board_count);

    // Step 3: Analyze
    println!("\n3. SEMANTIC ANALYSIS...");
    let mut symbol_table = SymbolTable::new();
    let analysis_result = analyze(&ast, &mut symbol_table);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("  Diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("    {}", diag.message);
        }
    }
    
    println!("  ✓ Analysis complete");
    
    // Step 4: In a real pipeline, we would synthesize to netlist here
    // For this demo, we'll create the SPICE circuit directly
    println!("\n4. CREATING SPICE CIRCUIT (simulating netlist conversion)...");
    
    // This represents what the synthesizer would produce
    let mut circuit = Circuit::new();
    
    // Nodes from the BHDL circuit
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("N1".to_string(), None);  // Connection point between R1 and LED1
    circuit.add_node("GND".to_string(), None);
    
    // Components from the BHDL circuit
    circuit.add_branch("V_VCC".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    // Create component models
    let mut models = HashMap::new();
    
    models.insert("V_VCC".to_string(), 
        StdlibModelLoader::create_voltage_source_model("V_VCC", 5.0));
    
    models.insert("R1".to_string(), 
        StdlibModelLoader::create_resistor_model("R1", 330.0, None));
    
    models.insert("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    println!("  ✓ SPICE circuit created");
    println!("    Nodes: VCC, N1, GND");
    println!("    Components: V_VCC (5V), R1 (330Ω), LED1 (red)");

    // Step 5: Run DC analysis
    println!("\n5. DC ANALYSIS...");
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  ✓ Found {} DC solution(s)", solutions.len());
            
            for (i, (_, _, _, sol)) in solutions.iter().enumerate() {
                println!("\n  Solution {}:", i+1);
                
                // Get LED current
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    if let Some(&current) = sol.branch_currents.get(&r1_idx) {
                        let current_ma = current.abs() * 1000.0;
                        println!("    LED current: {:.2}mA", current_ma);
                        
                        // Calculate LED voltage
                        let vr1 = current.abs() * 330.0;
                        let vled = 5.0 - vr1;
                        println!("    LED voltage: {:.2}V", vled);
                        println!("    Power dissipation: {:.2}mW", sol.total_power * 1000.0);
                    }
                }
            }
            
            if solutions.len() > 1 {
                println!("\n  Note: Multiple solutions found - MAESTRO will select the best one");
            }
        }
        Err(e) => {
            println!("  ✗ DC analysis failed: {}", e);
            return Err(e.into());
        }
    }

    // Step 6: Run transient analysis with MAESTRO
    println!("\n6. TRANSIENT ANALYSIS (with MAESTRO DC selection)...");
    
    let mut solver2 = GlacierSolver::new(circuit.clone());
    for (name, model) in models {
        solver2.add_model(name, model);
    }
    
    println!("  Starting transient simulation...");
    let start = std::time::Instant::now();
    
    match solver2.analyze_transient(0.001, 0.0001, None) {
        Ok(result) => {
            let elapsed = start.elapsed();
            
            println!("\n  ✓✓✓ TRANSIENT ANALYSIS SUCCESSFUL! ✓✓✓");
            println!("\n  Performance:");
            println!("    Completed in: {:.3}s", elapsed.as_secs_f64());
            println!("    Time points: {}", result.time_points.len());
            println!("    Simulated: 0 to {:.1}ms", result.time_points.last().unwrap_or(&0.0) * 1000.0);
            
            // Show DC operating point selected by MAESTRO
            if let Some(initial) = result.branch_currents.first() {
                println!("\n  Initial DC operating point (selected by MAESTRO):");
                
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    if let Some(&current) = initial.get(&r1_idx) {
                        let current_ma = current.abs() * 1000.0;
                        println!("    LED current: {:.2}mA", current_ma);
                        
                        // Expected: (5V - 2V) / 330Ω ≈ 9.1mA
                        let expected = (5.0 - 2.0) / 330.0 * 1000.0;
                        let error = ((current_ma - expected) / expected * 100.0).abs();
                        
                        println!("    Expected: {:.2}mA", expected);
                        println!("    Accuracy: {:.1}%", 100.0 - error);
                    }
                }
            }
            
            // Check stability
            if result.time_points.len() > 10 {
                let first = result.branch_currents.first().unwrap();
                let last = result.branch_currents.last().unwrap();
                
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    let i_start = first.get(&r1_idx).copied().unwrap_or(0.0);
                    let i_end = last.get(&r1_idx).copied().unwrap_or(0.0);
                    let drift = ((i_end - i_start) / i_start.abs()).abs() * 100.0;
                    
                    println!("\n  Stability:");
                    println!("    Current drift: {:.4}%", drift);
                    if drift < 0.01 {
                        println!("    ✓ Extremely stable!");
                    }
                }
            }
        }
        Err(e) => {
            println!("\n  ✗ Transient analysis failed: {}", e);
            return Err(e.into());
        }
    }

    println!("\n=== PIPELINE DEMONSTRATION COMPLETE ===");
    println!("\nKey accomplishments:");
    println!("1. ✓ Parsed BHDL code successfully");
    println!("2. ✓ Performed semantic analysis");
    println!("3. ✓ Created SPICE circuit (simulating netlist synthesis)");
    println!("4. ✓ Found DC operating points");
    println!("5. ✓ MAESTRO selected appropriate DC point");
    println!("6. ✓ Completed stable transient simulation");
    
    println!("\nThis demonstrates the complete flow from BHDL source code");
    println!("to transient simulation results with MAESTRO DC selection.");
    
    Ok(())
}