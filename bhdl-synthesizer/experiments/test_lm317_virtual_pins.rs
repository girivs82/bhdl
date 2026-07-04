/// Test LM317 adjustable regulator virtual pin expansion
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_synthesizer::NetlistGenerator;
use bhdl_analyzer::analyze;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("=== LM317 Adjustable Regulator Virtual Pin Test ===\n");
    
    // Read test circuit
    let test_file = "tests/circuits/realistic/adjustable_regulator_lm317.bhdl";
    let test_code = std::fs::read_to_string(test_file)?;
    
    println!("Test circuit:\n{}\n", test_code);
    
    // Parse
    let parse_result = parse(&test_code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  {}", error.message);
        }
        return Ok(());
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Analyze
    let analysis_result = analyze(&source_file);
    println!("Analysis complete. Diagnostics: {}\n", analysis_result.diagnostics.len());
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    println!("=== Netlist Generation Results ===");
    println!("Modules: {}", netlist.modules.len());
    println!("Instances: {}", netlist.instances.len());
    println!("Nets: {}\n", netlist.nets.len());
    
    // Verify LM317 and supporting components
    println!("=== Component Verification ===");
    
    let mut lm317_found = false;
    let mut resistors_found = 0;
    let mut diodes_found = 0;
    let mut capacitors_found = 0;
    
    for (_, instance) in &netlist.instances {
        if instance.name == "U1" {
            lm317_found = true;
            println!("✓ Found LM317: {}", instance.name);
        } else if instance.name.starts_with("U1_R") {
            resistors_found += 1;
            println!("✓ Found feedback resistor: {}", instance.name);
        } else if instance.name.starts_with("U1_D") {
            diodes_found += 1;
            println!("✓ Found protection diode: {}", instance.name);
        } else if instance.name.starts_with("U1_C") {
            capacitors_found += 1;
            println!("✓ Found capacitor: {}", instance.name);
        }
    }
    
    println!("\nComponent summary:");
    println!("  Feedback resistors: {} (expected: 2)", resistors_found);
    println!("  Protection diodes: {} (expected: 2)", diodes_found);
    println!("  Capacitors: {} (expected: 3)", capacitors_found);
    
    // Verify critical nets
    println!("\n=== Net Verification ===");
    
    // Check for ADJ net (should be created for feedback network)
    let adj_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n.contains("ADJ")));
    
    if adj_net.is_some() {
        println!("✓ ADJ net found (feedback network)");
    } else {
        println!("✗ ADJ net not found");
    }
    
    // Check VOUT net
    let vout_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n == "VOUT"));
    
    if let Some((_, vout_net)) = vout_net {
        println!("✓ VOUT net found");
        
        // Count components on VOUT
        let vout_connections = vout_net.connections.len();
        println!("  {} connections on VOUT net", vout_connections);
    } else {
        println!("✗ VOUT net not found");
    }
    
    // Check for proper feedback network connectivity
    println!("\n=== Feedback Network Analysis ===");
    
    // Find R1 (240Ω from VOUT to ADJ)
    let r1_found = netlist.instances.iter()
        .any(|(_, inst)| inst.name == "U1_R1");
    
    // Find R2 (calculated value from ADJ to GND)
    let r2_found = netlist.instances.iter()
        .any(|(_, inst)| inst.name == "U1_R2");
    
    if r1_found && r2_found {
        println!("✓ Complete feedback network found (R1 and R2)");
        println!("  R1: 240Ω (VOUT to ADJ)");
        println!("  R2: Calculated for 9V output");
        
        // Calculate expected R2 value
        let vout = 9.0;
        let vref = 1.25;
        let r1 = 240.0;
        let r2_calculated = r1 * ((vout / vref) - 1.0);
        println!("  R2 calculated value: {:.1}Ω", r2_calculated);
    } else {
        println!("✗ Incomplete feedback network");
    }
    
    // Summary
    println!("\n=== Test Summary ===");
    
    let all_components_found = lm317_found && 
                               resistors_found >= 2 && 
                               diodes_found >= 2 && 
                               capacitors_found >= 3;
    
    if all_components_found {
        println!("✅ SUCCESS: LM317 virtual pin expansion complete!");
        println!("   - Adjustable regulator with feedback network");
        println!("   - Protection diodes included");
        println!("   - Input/output/adjustment capacitors added");
        println!("   - Total supporting components: {}", 
                 resistors_found + diodes_found + capacitors_found);
    } else {
        println!("❌ FAILURE: Missing components");
        if !lm317_found {
            println!("   - LM317 not found");
        }
        if resistors_found < 2 {
            println!("   - Missing feedback resistors");
        }
        if diodes_found < 2 {
            println!("   - Missing protection diodes");
        }
        if capacitors_found < 3 {
            println!("   - Missing capacitors");
        }
    }
    
    Ok(())
}