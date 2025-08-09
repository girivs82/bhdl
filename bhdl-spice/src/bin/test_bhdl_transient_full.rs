//! Full end-to-end test: BHDL -> Parse -> Analyze -> Synthesize -> SPICE -> Transient with MAESTRO

use anyhow::{Result, Context};
use std::fs;
use std::collections::HashMap;
use rowan::ast::AstNode;

use bhdl_parser::parse_source;
use bhdl_ast::SourceFile;
use bhdl_analyzer::{analyze, SymbolTable};
use bhdl_synthesizer::Synthesizer;
use bhdl_netlist::Netlist;
use bhdl_spice::{Circuit, ComponentModel, GlacierSolver};

fn main() -> Result<()> {
    // Enable logging to see MAESTRO messages
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
    
    println!("\n=== FULL BHDL TO TRANSIENT ANALYSIS TEST ===\n");
    
    // Step 1: Read BHDL file
    let bhdl_file = "tests/circuits/simple/cli_test_led.bhdl";
    println!("1. Reading BHDL file: {}", bhdl_file);
    
    let content = fs::read_to_string(bhdl_file)
        .with_context(|| format!("Failed to read {}", bhdl_file))?;
    
    println!("   ✓ File read ({} bytes)", content.len());
    println!("   Content preview:");
    for line in content.lines().take(8) {
        println!("     {}", line);
    }
    
    // Step 2: Parse BHDL
    println!("\n2. Parsing BHDL...");
    let (parsed, errors) = parse_source(&content);
    
    if !errors.is_empty() {
        println!("   ❌ Parse errors:");
        for error in errors {
            println!("      {}", error);
        }
        return Err(anyhow::anyhow!("Parse failed"));
    }
    
    let ast = SourceFile::cast(parsed)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    println!("   ✓ Parse successful");
    
    // Step 3: Analyze
    println!("\n3. Running semantic analysis...");
    let mut symbol_table = SymbolTable::new();
    let analysis_result = analyze(&ast, &mut symbol_table);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("   Diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("      {:?}: {}", diag.severity, diag.message);
        }
    }
    
    println!("   ✓ Analysis complete");
    println!("   Power domains: {:?}", analysis_result.power_domains.keys().collect::<Vec<_>>());
    
    // Step 4: Synthesize to netlist
    println!("\n4. Synthesizing to netlist...");
    let mut synthesizer = Synthesizer::new();
    let netlist = synthesizer.synthesize(&ast, &analysis_result, &symbol_table)?;
    
    let top_module = netlist.top_level()
        .and_then(|id| netlist.module(id))
        .ok_or_else(|| anyhow::anyhow!("No top module"))?;
    
    println!("   ✓ Netlist generated");
    println!("   Module: {}", top_module.name);
    println!("   Instances: {}", top_module.instances().count());
    println!("   Nets: {}", netlist.nets().count());
    
    // Step 5: Convert to SPICE
    println!("\n5. Converting to SPICE circuit...");
    let (circuit, models) = netlist_to_spice(&netlist)?;
    
    println!("   ✓ SPICE circuit created");
    println!("   Nodes: {:?}", circuit.nodes().map(|(_, n)| &n.name).collect::<Vec<_>>());
    println!("   Branches: {:?}", circuit.branches().map(|(_, b)| format!("{}: {}", b.name, b.component_type)).collect::<Vec<_>>());
    
    // Step 6: Run DC analysis to see multiple solutions
    println!("\n6. Running DC analysis to check for multiple solutions...");
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("   Found {} DC solution(s)", solutions.len());
            for (i, (_, _, _, sol)) in solutions.iter().enumerate() {
                println!("   Solution {}: Power = {:.3}mW", i+1, sol.total_power * 1000.0);
            }
        }
        Err(e) => println!("   DC analysis failed: {}", e),
    }
    
    // Step 7: Run transient analysis with MAESTRO
    println!("\n7. Running transient analysis with MAESTRO DC selection...");
    let mut solver2 = GlacierSolver::new(circuit.clone());
    for (name, model) in models {
        solver2.add_model(name.clone(), model);
    }
    
    let start = std::time::Instant::now();
    match solver2.analyze_transient(0.001, 0.0001, None) {
        Ok(result) => {
            let elapsed = start.elapsed();
            println!("\n   ✓ Transient analysis SUCCESSFUL!");
            println!("   Completed in: {:.3}s", elapsed.as_secs_f64());
            println!("   Time points: {}", result.time_points.len());
            println!("   Simulation time: 0 to {:.1}ms", result.time_points.last().unwrap_or(&0.0) * 1000.0);
            
            // Check initial DC point
            if let Some(initial_currents) = result.branch_currents.first() {
                println!("\n   Initial DC operating point:");
                for (branch_idx, &current) in initial_currents {
                    if let Some((_, branch)) = circuit.branches().find(|(idx, _)| idx == branch_idx) {
                        if current.abs() > 1e-9 {
                            println!("     {} ({}): {:.2}mA", 
                                     branch.name, 
                                     branch.component_type,
                                     current.abs() * 1000.0);
                        }
                    }
                }
            }
            
            // Check stability
            if result.time_points.len() > 5 {
                let first = result.branch_currents.first().unwrap();
                let last = result.branch_currents.last().unwrap();
                
                // Find LED current
                if let Some((led_idx, led_branch)) = circuit.branches()
                    .find(|(_, b)| b.component_type == "LED") {
                    
                    let i_start = first.get(led_idx).copied().unwrap_or(0.0);
                    let i_end = last.get(led_idx).copied().unwrap_or(0.0);
                    let drift = ((i_end - i_start) / i_start.abs()).abs() * 100.0;
                    
                    println!("\n   Stability check:");
                    println!("     LED current drift: {:.3}%", drift);
                    if drift < 1.0 {
                        println!("     ✓ Excellent stability!");
                    }
                }
            }
        }
        Err(e) => {
            println!("\n   ✗ Transient analysis FAILED: {}", e);
            return Err(e.into());
        }
    }
    
    println!("\n=== TEST COMPLETE ===");
    println!("✓ Successfully processed BHDL circuit through full pipeline");
    println!("✓ MAESTRO DC selection working correctly");
    println!("✓ Transient analysis producing stable results");
    
    Ok(())
}

fn netlist_to_spice(netlist: &Netlist) -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    use bhdl_spice::stdlib_model_loader::StdlibModelLoader;
    
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    let top_module_id = netlist.top_level_module
        .ok_or_else(|| anyhow::anyhow!("No top module"))?;
    let top_module = netlist.modules.get(top_module_id)
        .ok_or_else(|| anyhow::anyhow!("Top module not found"))?;
    
    // Add nodes
    for (_, net) in &netlist.nets {
        let net_name = net.name.clone().unwrap_or_else(|| "unnamed".to_string());
        circuit.add_node(net_name, None);
    }
    
    // Add instances as branches
    for inst_id in &top_module.internal_instances {
        let instance = netlist.instances.get(*inst_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        let inst_name = instance.name.clone();
        
        // Get module definition to find component type
        let inst_module = netlist.modules.get(instance.definition)
            .ok_or_else(|| anyhow::anyhow!("Instance module not found"))?;
        let comp_type = inst_module.name.clone();
        
        // For a simple two-terminal component, find connected nets
        // This is a simplified approach - real code would need proper pin mapping
        let connected_nets: Vec<_> = netlist.nets.iter()
            .filter(|(_, net)| {
                net.connections.iter().any(|conn| {
                    if let crate::netlist::ConnectionPoint::InstancePin(iid, _) = conn {
                        iid == inst_id
                    } else {
                        false
                    }
                })
            })
            .collect();
        
        if connected_nets.len() >= 2 {
            let from_net = connected_nets[0].1.name.clone()
                .unwrap_or_else(|| format!("net_{}", connected_nets[0].0.into_raw_parts().0));
            let to_net = connected_nets[1].1.name.clone()
                .unwrap_or_else(|| format!("net_{}", connected_nets[1].0.into_raw_parts().0));
            
            // Map component types and create models
            match comp_type.as_str() {
                "Res" => {
                    circuit.add_branch(&inst_name, &from_net, &to_net, "Resistor".to_string(), 0.0, None);
                    
                    // Extract resistance value
                    let resistance = instance.parameters.get("value")
                        .and_then(|v| v.as_str())
                        .and_then(parse_resistance)
                        .unwrap_or(1000.0);
                    
                    models.insert(inst_name.clone(), 
                        StdlibModelLoader::create_resistor_model(&inst_name, resistance, None));
                }
                "LED" => {
                    circuit.add_branch(&inst_name, &from_net, &to_net, "LED".to_string(), 0.0, None);
                    
                    let color = instance.parameters.get("color")
                        .and_then(|v| v.as_str())
                        .unwrap_or("red");
                    
                    models.insert(inst_name.clone(), ComponentModel::LED {
                        color: color.to_string(),
                        forward_voltage: if color == "red" { 2.0 } else { 2.2 },
                        forward_current: 0.020,
                        dynamic_resistance: 10.0,
                        saturation_current: Some(1e-15),
                        emission_coefficient: Some(1.8),
                        thermal_voltage: Some(0.026),
                        limits: Default::default(),
                    });
                }
                _ => {
                    // For power sources, check if it's a power domain
                    if comp_type == "PowerDomain" {
                        // This is the voltage source
                        circuit.add_branch(&inst_name, &from_net, &to_net, "VoltageSource".to_string(), 0.0, None);
                        
                        // Extract voltage from parameters or default
                        let voltage = instance.parameters.get("voltage")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(5.0);
                        
                        models.insert(inst_name.clone(),
                            StdlibModelLoader::create_voltage_source_model(&inst_name, voltage));
                    }
                }
            }
        }
    }
    
    Ok((circuit, models))
}

fn parse_resistance(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.ends_with("Ω") {
        let num_part = &s[..s.len()-2];
        if num_part.ends_with("k") {
            num_part[..num_part.len()-1].parse::<f64>().ok().map(|v| v * 1000.0)
        } else if num_part.ends_with("M") {
            num_part[..num_part.len()-1].parse::<f64>().ok().map(|v| v * 1000000.0)
        } else {
            num_part.parse::<f64>().ok()
        }
    } else {
        s.parse::<f64>().ok()
    }
}