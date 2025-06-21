use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use tokio;

#[tokio::main]
async fn main() {
    println!("=== Testing Hierarchical Reference Designator Generation ===\n");
    
    // Test case: Nested modules with components that need unique reference designators
    let code = r#"
module PowerFilter() {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground in;
    
    // Input capacitor
    VIN -> Cap(10uF).1;
    Cap(10uF).2 -> GND;
    
    // Series resistor  
    VIN -> Res(10).1;
    Res(10).2 -> VOUT;
    
    // Output capacitor
    VOUT -> Cap(1uF).1;
    Cap(1uF).2 -> GND;
}

module VoltageRegulator() {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground in;
    
    // Input filter
    input_filter: PowerFilter() {
        VIN <- VIN;
        VOUT -> filtered_input;
        GND <- GND;
    }
    
    // Regulator components
    filtered_input -> Res(1k).1;
    Res(1k).2 -> VOUT;
    
    // Feedback network
    VOUT -> Res(10k).1;
    Res(10k).2 -> feedback;
    feedback -> Res(4.7k).1;
    Res(4.7k).2 -> GND;
    
    // Output filter
    output_filter: PowerFilter() {
        VIN <- VOUT;
        VOUT -> VOUT;
        GND <- GND;
    }
}

board PowerSupply {
    power VIN_12V = 12V @ 2A;
    ground GND;
    
    // 5V regulator
    reg_5v: VoltageRegulator() {
        VIN <- VIN_12V;
        VOUT -> RAIL_5V;
        GND <- GND;
    }
    
    // 3.3V regulator
    reg_3v3: VoltageRegulator() {
        VIN <- RAIL_5V;
        VOUT -> RAIL_3V3;
        GND <- GND;
    }
    
    // Load capacitors
    RAIL_5V -> Cap(100uF).1;
    Cap(100uF).2 -> GND;
    
    RAIL_3V3 -> Cap(100uF).1;
    Cap(100uF).2 -> GND;
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
    
    println!("\n3. Expected reference designators:");
    println!("Board level:");
    println!("  - C1, C2 (load capacitors)");
    println!("\nreg_5v (VoltageRegulator):");
    println!("  - reg_5v.R1, reg_5v.R2, reg_5v.R3 (regulator resistors)");
    println!("  - reg_5v.input_filter.C1, reg_5v.input_filter.C2, reg_5v.input_filter.R1");
    println!("  - reg_5v.output_filter.C1, reg_5v.output_filter.C2, reg_5v.output_filter.R1");
    println!("\nreg_3v3 (VoltageRegulator):");
    println!("  - reg_3v3.R1, reg_3v3.R2, reg_3v3.R3 (regulator resistors)");
    println!("  - reg_3v3.input_filter.C1, reg_3v3.input_filter.C2, reg_3v3.input_filter.R1");
    println!("  - reg_3v3.output_filter.C1, reg_3v3.output_filter.C2, reg_3v3.output_filter.R1");
    
    println!("\n4. Generating netlist...");
    
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        database_path: Some("/tmp/test_components.db".to_string()),
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    match generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await {
        Ok(netlist) => {
            println!("✓ Netlist generation successful");
            
            println!("\nNetlist statistics:");
            println!("  - Modules: {}", netlist.modules.len());
            println!("  - Instances: {}", netlist.instances.len());
            
            println!("\nGenerated reference designators:");
            
            // Group instances by their hierarchical path
            let mut hierarchy_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
            
            for (_, instance) in &netlist.instances {
                if let Some(module) = netlist.modules.get(instance.definition) {
                    // Extract hierarchical path from instance name
                    let parts: Vec<&str> = instance.name.split('.').collect();
                    let path = if parts.len() > 1 {
                        parts[..parts.len()-1].join(".")
                    } else {
                        "Board".to_string()
                    };
                    
                    hierarchy_map.entry(path)
                        .or_insert_with(Vec::new)
                        .push(format!("{} ({})", instance.name, module.name));
                }
            }
            
            // Display instances by hierarchy
            for (path, instances) in &hierarchy_map {
                println!("\n{}:", path);
                for inst in instances {
                    println!("  - {}", inst);
                }
            }
            
            // Check if reference designators are hierarchical
            let has_hierarchical_refdes = netlist.instances.iter()
                .any(|(_, inst)| inst.name.contains('.'));
            
            if has_hierarchical_refdes {
                println!("\n✅ Hierarchical reference designators detected!");
            } else {
                println!("\n❌ No hierarchical reference designators found - implementation needed");
            }
            
            // Generate SPICE to see the final reference designators
            println!("\n5. SPICE netlist preview:");
            match bhdl_synthesizer::hierarchical_connectivity::generate_spice_subcircuits(&netlist, &analysis_result) {
                Ok(spice) => {
                    // Show first few lines of SPICE output
                    for (i, line) in spice.lines().enumerate() {
                        if i < 20 {
                            println!("{}", line);
                        }
                    }
                    if spice.lines().count() > 20 {
                        println!("... (truncated)");
                    }
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