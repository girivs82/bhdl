use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use tokio;

#[tokio::main]
async fn main() {
    println!("=== Testing Module Variants and Deduplication ===\n");
    
    // Test case: Same module with different parameters should create variants
    let code = r#"
module RC_Filter(r_value: resistance = 1k, c_value: capacitance = 1uF) {
    pin IN: signal in;
    pin OUT: signal out;
    pin GND: ground in;
}

board FilterChain {
    ground GND;
    
    // Low-pass filter with 1kHz cutoff
    lpf_1k: RC_Filter(r_value=1.6k, c_value=100nF) {
        IN <- input;
        OUT -> stage1;
        GND <- GND;
    }
    
    // Another identical low-pass filter (should reuse variant)
    lpf_1k_2: RC_Filter(r_value=1.6k, c_value=100nF) {
        IN <- stage1;
        OUT -> stage2;
        GND <- GND;
    }
    
    // Different filter with 10kHz cutoff (should create new variant)
    lpf_10k: RC_Filter(r_value=1.6k, c_value=10nF) {
        IN <- stage2;
        OUT -> output;
        GND <- GND;
    }
    
    // Default filter (should create another variant)
    default_filter: RC_Filter() {
        IN <- test_in;
        OUT -> test_out;
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
    
    println!("\n3. Analyzing module variants needed:");
    println!("Expected variants:");
    println!("  - RC_Filter_1k6_100n (for 1kHz filters)");
    println!("  - RC_Filter_1k6_10n (for 10kHz filter)");
    println!("  - RC_Filter_1k_1u (for default filter)");
    
    println!("\n4. Generating netlist...");
    
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
            
            println!("\nModule definitions:");
            for (id, module) in &netlist.modules {
                println!("  - {:?}: {} ({:?})", id, module.name, module.kind);
                // Check if module has attributes for parameters
                if !module.attributes.is_empty() {
                    println!("    Attributes:");
                    for (key, value) in &module.attributes {
                        println!("      * {}: {}", key, value);
                    }
                }
            }
            
            println!("\nModule instances:");
            for (_, instance) in &netlist.instances {
                if let Some(module) = netlist.modules.get(instance.definition) {
                    println!("  - {} : {} ({:?})", instance.name, module.name, module.kind);
                    if !instance.attributes.is_empty() {
                        println!("    Instance attributes:");
                        for (key, value) in &instance.attributes {
                            println!("      * {}: {}", key, value);
                        }
                    }
                }
            }
            
            // Generate SPICE to show variants
            println!("\n5. SPICE output:");
            match bhdl_synthesizer::hierarchical_connectivity::generate_spice_subcircuits(&netlist, &analysis_result) {
                Ok(spice) => {
                    println!("{}", spice);
                    
                    // Check if SPICE has proper variants
                    let lines: Vec<&str> = spice.lines().collect();
                    let subckt_count = lines.iter().filter(|l| l.starts_with(".SUBCKT")).count();
                    println!("\nSPICE subcircuits defined: {}", subckt_count);
                    println!("Expected: 3 variants of RC_Filter");
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