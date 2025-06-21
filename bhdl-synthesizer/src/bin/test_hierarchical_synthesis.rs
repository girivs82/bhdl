use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use tokio;

#[tokio::main]
async fn main() {
    println!("=== Testing Hierarchical Module Synthesis ===\n");
    
    // Test case: Hierarchical PWM regulator
    let code = r#"
module PWMController(frequency: frequency = 100kHz) {
    pin VCC: power in;
    pin GND: ground in;
    pin OUT: signal out;
    pin EN: signal in;
    pin FEEDBACK: signal in;
}

module PowerRegulator(vout: voltage = 3.3V) {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground in;
    pin EN: signal in;
    
    // PWM controller instance
    pwm: PWMController(frequency=500kHz) {
        VCC <- VIN;
        GND <- GND;
        OUT -> switch_node;
        EN <- EN;
        FEEDBACK <- feedback_node;
    }
    
    // Power path connections
    switch_node -> VOUT;
    
    // Feedback network
    VOUT -> feedback_node;
}

board DualRailSupply {
    power VIN_12V = 12V @ 5A;
    ground GND;
    
    // 5V rail regulator
    reg_5v: PowerRegulator(vout=5V) {
        VIN <- VIN_12V;
        VOUT -> RAIL_5V;
        GND <- GND;
        EN <- enable_5v;
    }
    
    // 3.3V rail regulator
    reg_3v3: PowerRegulator(vout=3.3V) {
        VIN <- VIN_12V;
        VOUT -> RAIL_3V3;
        GND <- GND;
        EN <- enable_3v3;
    }
    
    // Enable signals
    enable_5v -> VIN_12V;
    enable_3v3 -> VIN_12V;
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
    
    println!("\n3. Generating hierarchical netlist...");
    
    // Configure for hierarchical synthesis
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: false,  // Disable component inference to avoid database requirement
        flatten_hierarchy: false,  // Keep hierarchy
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
            
            println!("\nModule definitions:");
            for (_, module) in &netlist.modules {
                println!("  - {} ({:?})", module.name, module.kind);
                println!("    Ports: {}", module.ports.len());
                // Ports are stored as IDs, need to look them up
                for &port_id in &module.ports {
                    if let Some(port) = netlist.ports.get(port_id) {
                        println!("      * {} ({:?})", port.name, port.direction);
                    }
                }
            }
            
            println!("\nModule instances:");
            for (_, instance) in &netlist.instances {
                if let Some(module) = netlist.modules.get(instance.definition) {
                    println!("  - {} : {} ({:?})", instance.name, module.name, module.kind);
                }
            }
            
            println!("\nNets:");
            for (_, net) in &netlist.nets {
                println!("  - {}", net.name.as_deref().unwrap_or("<unnamed>"));
            }
            
            // Generate SPICE subcircuits
            println!("\n4. Generating SPICE subcircuits...");
            match bhdl_synthesizer::hierarchical_connectivity::generate_spice_subcircuits(&netlist, &analysis_result) {
                Ok(spice) => {
                    println!("✓ SPICE generation successful");
                    println!("\nSPICE output:");
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