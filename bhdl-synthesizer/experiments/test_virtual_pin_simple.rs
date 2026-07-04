use anyhow::Result;
use bhdl_parser;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_synthesizer::Synthesizer;
use bhdl_analyzer::analyze_with_base_path;
use colored::*;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("{}", "=== Simple Virtual Pin Test ===".bold().cyan());
    
    // Create a simple test module with virtual pins
    let simple_module = r#"
entity SimpleRegulator(vout: voltage = 5V) {
    pin VIN: power in;
    pin VOUT: power out virtual;
    pin GND: ground inout;
    pin EN: signal in;
}

// Simple virtual pin expansion
const SimpleRegulator_VIRTUAL_PIN_EXPANSION = {
    VOUT: {
        components: {
            output_cap: {
                type: "Capacitor", 
                value: "10µF",
                reference: "C_OUT"
            },
            feedback_resistor: {
                type: "Resistor",
                value: "10kΩ", 
                reference: "R_FB"
            }
        }
    }
};

board TestBoard {
    power VIN = 12V @ 1A;
    ground GND;
    
    VIN -> reg: SimpleRegulator(vout=5V);
    reg.GND -> GND;
    reg.VOUT -> load: Res(10Ω).1;
    load.2 -> GND;
}
"#;
    
    println!("Testing simple BHDL with virtual pins...");
    
    // Parse the BHDL
    let parse_result = bhdl_parser::parse(simple_module);
    if !parse_result.errors().is_empty() {
        println!("{}", "Parser errors:".red());
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parser errors found"));
    }
    println!("✓ Parsing successful");
    
    // Create AST
    let ast = SourceFile::cast(parse_result.syntax()).ok_or_else(|| anyhow::anyhow!("Failed to create AST"))?;
    
    // Run analyzer
    let analysis_result = analyze_with_base_path(&ast, std::path::Path::new("."));
    
    if !analysis_result.diagnostics.is_empty() {
        println!("{}", "Analyzer diagnostics:".yellow());
        for diag in &analysis_result.diagnostics {
            println!("  {}: {}", 
                if diag.message.contains("Error") { "ERROR".red() }
                else { "WARNING".yellow() },
                diag.message
            );
        }
        
        // Check if there are critical errors
        let critical_errors = analysis_result.diagnostics.iter()
            .any(|d| d.message.contains("Error"));
        if critical_errors {
            return Err(anyhow::anyhow!("Analysis failed with critical errors"));
        }
    }
    println!("✓ Analysis completed");
    
    // Generate netlist
    let mut synthesizer = Synthesizer::new();
    let netlist = synthesizer.synthesize(&ast, &analysis_result).await?;
    println!("✓ Netlist generation successful");
    
    // Check for virtual pin expansion
    println!("\n{}", "=== Virtual Pin Expansion Results ===".bold().magenta());
    
    println!("Modules: {}", netlist.modules.len());
    for module in netlist.modules.values() {
        println!("  - {}", module.name.yellow());
    }
    
    println!("\nInstances: {}", netlist.instances.len());
    let mut virtual_components_found = 0;
    for instance in netlist.instances.values() {
        let module_name = &netlist.modules[instance.definition].name;
        let ref_des = instance.attributes.get("reference")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "no_ref".to_string());
            
        println!("  - {} ({}) [{}]", 
            instance.name.green(),
            module_name.cyan(),
            ref_des.bright_black()
        );
        
        // Check if this looks like a virtual pin component
        if ref_des == "C_OUT" || ref_des == "R_FB" {
            virtual_components_found += 1;
            println!("    {} Virtual pin component detected!", "→".green());
        }
    }
    
    println!("\nNets: {}", netlist.nets.len());
    for net in netlist.nets.values() {
        let default_name = "unnamed".to_string();
        let name = net.name.as_ref().unwrap_or(&default_name);
        println!("  - {}", name.cyan());
    }
    
    // Final assessment
    if virtual_components_found > 0 {
        println!("\n{} Virtual pin expansion detected! Found {} virtual components.", 
                 "✓".green().bold(), virtual_components_found);
    } else {
        println!("\n{} No virtual pin expansion detected. This may indicate the feature is not yet implemented.", 
                 "⚠".yellow().bold());
    }
    
    Ok(())
}