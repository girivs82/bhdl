use std::fs;
use std::path::Path;
use bhdl_analyzer::Analyzer;
use bhdl_analyzer::net_attributes::NetAttribute;
use bhdl_ast::SourceFile;
use bhdl_parser::Parser;
use bhdl_synthesizer::Synthesizer;
use colored::*;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "=== TPS54331 Virtual Pin Synthesis Test ===".bold().cyan());
    
    // Read the test file
    let test_file = "tests/circuits/realistic/buck_converter_tps54331.bhdl";
    println!("Reading: {}", test_file);
    
    let source = fs::read_to_string(test_file)?;
    
    // Parse the source
    println!("\n{}", "Parsing...".bold());
    let mut parser = Parser::new(&source);
    let (node, diagnostics) = parser.parse();
    
    if !diagnostics.is_empty() {
        println!("{}", "Parser diagnostics:".yellow());
        for diag in &diagnostics {
            println!("  {} at {:?}: {}", 
                if diag.is_error() { "ERROR".red() } else { "WARNING".yellow() },
                diag.range,
                diag.message
            );
        }
        if diagnostics.iter().any(|d| d.is_error()) {
            return Err("Parser errors found".into());
        }
    }
    
    // Create AST
    let ast = SourceFile::cast(node.clone()).ok_or("Failed to create AST")?;
    
    // Run analyzer
    println!("{}", "Analyzing...".bold());
    let mut analyzer = Analyzer::new();
    let analysis_result = analyzer.analyze(&ast);
    
    // Check for analyzer errors
    if !analysis_result.diagnostics.is_empty() {
        println!("{}", "Analyzer diagnostics:".yellow());
        for diag in &analysis_result.diagnostics {
            println!("  {}: {}", 
                diag.severity.to_string().color(
                    if diag.severity.to_string().contains("Error") { "red" } 
                    else { "yellow" }
                ),
                diag.message
            );
        }
    }
    
    // Generate netlist
    println!("\n{}", "Generating netlist...".bold());
    let mut synthesizer = Synthesizer::new();
    let netlist = synthesizer.synthesize(&ast, &analysis_result).await?;
    
    // Print netlist summary
    println!("\n{}", "=== Netlist Summary ===".bold().green());
    
    // Count modules
    let module_count = netlist.modules.len();
    println!("Modules defined: {}", module_count.to_string().cyan());
    for module in netlist.modules.values() {
        println!("  - {} ({} pins)", 
            module.name.yellow(), 
            module.pins.len().to_string().cyan()
        );
    }
    
    // Count instances
    let instance_count = netlist.instances.len();
    println!("\nInstances created: {}", instance_count.to_string().cyan());
    
    // Group instances by module type
    let mut instance_types = std::collections::HashMap::new();
    for instance in netlist.instances.values() {
        let module_name = &netlist.modules[instance.module].name;
        *instance_types.entry(module_name.clone()).or_insert(0) += 1;
    }
    
    println!("\n{}", "Component breakdown:".bold());
    for (module_name, count) in instance_types.iter() {
        println!("  {} × {}", count.to_string().cyan(), module_name.yellow());
        
        // Show instances of this type
        for instance in netlist.instances.values() {
            if &netlist.modules[instance.module].name == module_name {
                println!("    - {} ({})", 
                    instance.name.green(),
                    instance.reference.as_ref().unwrap_or(&"no ref".to_string()).bright_black()
                );
                
                // Check if this is the TPS54331 and look for virtual pin expansion
                if module_name.contains("TPS54331") {
                    println!("      {} TPS54331 instance found - checking for virtual pin expansion...", "→".cyan());
                }
            }
        }
    }
    
    // Count nets
    println!("\n{}", "Nets created:".bold());
    println!("  Total nets: {}", netlist.nets.len().to_string().cyan());
    
    // Show power nets
    let power_nets: Vec<_> = netlist.nets.values()
        .filter(|net| net.attributes.iter().any(|attr| 
            matches!(attr, NetAttribute::PowerDomain { .. })
        ))
        .collect();
    
    if !power_nets.is_empty() {
        println!("\n  Power nets:");
        for net in power_nets {
            if let Some(attr) = net.attributes.iter().find(|a| 
                matches!(a, NetAttribute::PowerDomain { .. })
            ) {
                if let NetAttribute::PowerDomain { voltage, max_current, .. } = attr {
                    println!("    - {} ({}V @ {}A)", 
                        net.name.green(),
                        voltage,
                        max_current
                    );
                }
            }
        }
    }
    
    // Check for virtual pin expansion
    println!("\n{}", "=== Virtual Pin Expansion Check ===".bold().magenta());
    
    // Look for components that should be created by virtual pin
    let expected_components = vec![
        ("Inductor", "L1"),
        ("Capacitor", "C_BOOT"),
        ("Capacitor", "C_OUT1"),
        ("Capacitor", "C_OUT2"),
        ("Resistor", "R_FB1"),
        ("Resistor", "R_FB2"),
        ("Resistor", "R_COMP"),
        ("Capacitor", "C_COMP1"),
        ("Capacitor", "C_COMP2"),
        ("Capacitor", "C_SS"),
    ];
    
    println!("Expected components from virtual pin expansion:");
    for (comp_type, ref_des) in &expected_components {
        let found = netlist.instances.values().any(|inst| 
            inst.reference.as_ref().map_or(false, |r| r == ref_des)
        );
        
        if found {
            println!("  {} {} - {}", "✓".green(), ref_des.yellow(), comp_type.bright_black());
        } else {
            println!("  {} {} - {} (NOT FOUND)", "✗".red(), ref_des.yellow(), comp_type.bright_black());
        }
    }
    
    // Write netlist to file
    let output_path = "tests/outputs/netlists/buck_converter_tps54331.json";
    fs::create_dir_all(Path::new(output_path).parent().unwrap())?;
    
    let json = serde_json::to_string_pretty(&netlist)?;
    fs::write(output_path, json)?;
    println!("\n{} {}", "Netlist written to:".green(), output_path.cyan());
    
    // Final status
    let virtual_pin_expanded = expected_components.iter().all(|(_, ref_des)| 
        netlist.instances.values().any(|inst| 
            inst.reference.as_ref().map_or(false, |r| r == ref_des)
        )
    );
    
    if virtual_pin_expanded {
        println!("\n{} Virtual pin expansion successful!", "✓".green().bold());
    } else {
        println!("\n{} Virtual pin expansion incomplete or not implemented", "⚠".yellow().bold());
        println!("The synthesizer may need to be updated to handle virtual pins.");
    }
    
    Ok(())
}