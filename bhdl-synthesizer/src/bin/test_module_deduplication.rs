use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use tokio;

#[tokio::main]
async fn main() {
    println!("=== Testing Module Deduplication ===\n");
    
    // Test case: Multiple instances of the same module with same parameters
    let code = r#"
module Buffer(gain: real = 1.0) {
    pin IN: signal in;
    pin OUT: signal out;
    pin VCC: power in;
    pin GND: ground in;
}

module Amplifier(stages: int = 2) {
    pin IN: signal in;
    pin OUT: signal out;
    pin VCC: power in;
    pin GND: ground in;
    
    // Two buffer stages with same gain
    buf1: Buffer(gain=10.0) {
        IN <- IN;
        OUT -> stage1_out;
        VCC <- VCC;
        GND <- GND;
    }
    
    buf2: Buffer(gain=10.0) {
        IN <- stage1_out;
        OUT -> OUT;
        VCC <- VCC;
        GND <- GND;
    }
}

board AudioSystem {
    power VCC = 5V @ 500mA;
    ground GND;
    
    // Multiple amplifiers with same configuration
    left_channel: Amplifier(stages=2) {
        IN <- left_input;
        OUT -> left_output;
        VCC <- VCC;
        GND <- GND;
    }
    
    right_channel: Amplifier(stages=2) {
        IN <- right_input;
        OUT -> right_output;
        VCC <- VCC;
        GND <- GND;
    }
    
    // Another amplifier with different stages
    monitor: Amplifier(stages=3) {
        IN <- monitor_input;
        OUT -> monitor_output;
        VCC <- VCC;
        GND <- GND;
    }
}
"#;

    println!("1. Parsing...");
    let parse_result = parse(code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    println!("✓ Parsing successful");
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    println!("\n2. Running analysis...");
    let analysis_result = analyze(&source_file);
    
    println!("Analysis complete:");
    println!("  - Diagnostics: {}", analysis_result.diagnostics.len());
    for diag in &analysis_result.diagnostics {
        println!("    * {}", diag.message);
    }
    
    println!("\n3. Generating netlist WITHOUT deduplication...");
    
    // Configure without deduplication
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: false,
        flatten_hierarchy: false,
        database_path: None,
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    match generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await {
        Ok(netlist) => {
            println!("✓ Netlist generation successful");
            
            println!("\nNetlist statistics:");
            println!("  - Modules: {}", netlist.modules.len());
            println!("  - Instances: {}", netlist.instances.len());
            println!("  - Nets: {}", netlist.nets.len());
            
            // Count module types
            let mut module_counts = std::collections::HashMap::new();
            for (_, module) in &netlist.modules {
                *module_counts.entry(module.name.clone()).or_insert(0) += 1;
            }
            
            println!("\nModule type counts (showing duplication):");
            for (name, count) in &module_counts {
                println!("  - {}: {} definitions", name, count);
                if *count > 1 {
                    println!("    ⚠️  Duplicate definitions detected!");
                }
            }
            
            println!("\nAll module definitions:");
            for (id, module) in &netlist.modules {
                println!("  - {:?}: {} ({:?})", id, module.name, module.kind);
            }
            
            // Generate SPICE to show duplication
            println!("\n4. SPICE output (showing duplicate subcircuits):");
            match bhdl_synthesizer::hierarchical_connectivity::generate_spice_subcircuits(&netlist, &analysis_result) {
                Ok(spice) => {
                    println!("{}", spice);
                }
                Err(e) => {
                    println!("✗ SPICE generation failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ Netlist generation failed: {}", e);
        }
    }
    
    println!("\n=== Test Complete ===");
}