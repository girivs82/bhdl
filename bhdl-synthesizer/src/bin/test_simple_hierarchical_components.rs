use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use tokio;

#[tokio::main]
async fn main() {
    println!("=== Testing Simple Hierarchical Component Generation ===\n");
    
    // Simple test case with components in modules
    let code = r#"
module Filter() {
    pin IN: signal in;
    pin OUT: signal out;
    pin GND: ground in;
    
    // Simple RC filter
    IN -> Res(1k).1;
    Res(1k).2 -> OUT;
    OUT -> Cap(100nF).1;
    Cap(100nF).2 -> GND;
}

board TestBoard {
    ground GND;
    
    // Two filter instances
    filter1: Filter() {
        IN <- input1;
        OUT -> output1;
        GND <- GND;
    }
    
    filter2: Filter() {
        IN <- input2;
        OUT -> output2;
        GND <- GND;
    }
    
    // Board-level components
    input1 -> Res(10k).1;
    Res(10k).2 -> GND;
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
    println!("  - Inferred components: {}", analysis_result.component_inference.inferred_components.len());
    
    println!("\nInferred components from analyzer:");
    for comp in &analysis_result.component_inference.inferred_components {
        if let Some(name) = &comp.instance_name {
            println!("  - {} ({})", name, comp.component_type);
        } else {
            println!("  - <unnamed> ({})", comp.component_type);
        }
    }
    
    println!("\n3. Generating netlist...");
    
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: false, // Disable database lookup for simplicity
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
            
            println!("\nExpected instances:");
            println!("  - filter1 (Filter module)");
            println!("  - filter2 (Filter module)");
            println!("  - filter1.R1 (1k resistor in filter1)");
            println!("  - filter1.C1 (100nF capacitor in filter1)");
            println!("  - filter2.R1 (1k resistor in filter2)");
            println!("  - filter2.C1 (100nF capacitor in filter2)");  
            println!("  - R1 (10k resistor at board level)");
            
            println!("\nActual instances:");
            for (_, instance) in &netlist.instances {
                if let Some(module) = netlist.modules.get(instance.definition) {
                    println!("  - {} ({})", instance.name, module.name);
                }
            }
            
            // Check for hierarchical instances
            let hierarchical_count = netlist.instances.iter()
                .filter(|(_, inst)| inst.name.contains('.'))
                .count();
            
            println!("\nHierarchical instances: {}", hierarchical_count);
            
            if hierarchical_count > 0 {
                println!("✅ Hierarchical component instances detected!");
            } else {
                println!("❌ No hierarchical component instances found");
            }
        }
        Err(e) => {
            println!("✗ Netlist generation failed: {}", e);
        }
    }
    
    println!("\n=== Test Complete ===");
}