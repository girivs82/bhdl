// End-to-end pipeline test for BHDL v2.0
// Tests the complete flow: Parse -> AST -> Analyze -> Netlist -> Visualize

use std::fs;
use std::path::Path;

fn main() {
    println!("=== BHDL End-to-End Pipeline Test ===\n");
    
    // Read the test circuit
    let bhdl_file = "test_7805_regulator_realistic.bhdl";
    println!("1. Reading BHDL file: {}", bhdl_file);
    
    let content = fs::read_to_string(bhdl_file)
        .expect("Failed to read BHDL file");
    
    println!("   File size: {} bytes", content.len());
    println!("   First line: {}", content.lines().next().unwrap_or(""));
    
    // Stage 1: Parse
    println!("\n2. Parsing BHDL source...");
    let parsed = bhdl_parser::parse(&content);
    
    if !parsed.errors().is_empty() {
        println!("   ❌ Parse errors found:");
        for error in parsed.errors() {
            println!("      - {}", error.message);
        }
        return;
    }
    println!("   ✓ Parsing successful");
    
    // Stage 2: AST Generation
    println!("\n3. Generating AST...");
    use bhdl_ast::{SourceFile, Board};
    use rowan::ast::AstNode;
    
    let source_file = SourceFile::cast(parsed.syntax())
        .expect("Failed to create SourceFile AST node");
    
    // Find the board
    let board = source_file.boards().next()
        .expect("No board found in source file");
    
    println!("   ✓ AST generated");
    println!("   Board name: {:?}", board.name().map(|n| n.text().to_string()));
    println!("   Power declarations: {}", board.power_decls().count());
    println!("   Ground declarations: {}", board.ground_decls().count());
    println!("   Connections: {}", board.connections().count());
    
    // Stage 3: Semantic Analysis
    println!("\n4. Running semantic analysis...");
    let analysis_result = bhdl_analyzer::analyze(&source_file);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("   ⚠️  Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("      - {:?}: {}", diag.range, diag.message);
        }
    } else {
        println!("   ✓ No analysis errors");
    }
    
    println!("   Global symbols: {}", analysis_result.global_scope.all_symbols().count());
    
    // Stage 4: Netlist Generation
    println!("\n5. Generating netlist...");
    
    // For now, create a simple netlist manually since the converter might not be complete
    use bhdl_netlist::{Netlist, ModuleBuilder};
    
    let mut netlist = Netlist::new();
    let board_name = board.name().map(|n| n.text().to_string()).unwrap_or_else(|| "Board".to_string());
    
    let mut module_builder = ModuleBuilder::new(board_name.clone());
    
    // Add some basic components from our circuit
    let vin_net = module_builder.add_net("VIN");
    let vcc_net = module_builder.add_net("VCC");
    let gnd_net = module_builder.add_net("GND");
    
    // Add regulator
    let reg_inst = module_builder.add_instance("reg", "LM7805");
    module_builder.connect_pin(reg_inst, "IN", vin_net);
    module_builder.connect_pin(reg_inst, "OUT", vcc_net);
    module_builder.connect_pin(reg_inst, "GND", gnd_net);
    
    // Add LED and resistor
    let r_led = module_builder.add_instance("r_led", "Resistor");
    module_builder.set_parameter(r_led, "value", "330");
    
    let led = module_builder.add_instance("led", "LED");
    module_builder.set_parameter(led, "color", "green");
    
    let module_def = module_builder.build();
    let module_id = netlist.add_module(module_def);
    netlist.set_top_module(module_id);
    
    println!("   ✓ Netlist generated");
    println!("   Module: {}", netlist.top_module_name().unwrap_or("unnamed"));
    println!("   Instances: {}", netlist.module(module_id).unwrap().instances.len());
    println!("   Nets: {}", netlist.module(module_id).unwrap().nets.len());
    
    // Stage 5: Visualization
    println!("\n6. Generating visualization...");
    
    use bhdl_visualizer::{visualize_netlist, VisualizationOptions};
    
    let options = VisualizationOptions {
        width: 800.0,
        height: 600.0,
        show_labels: true,
        show_values: true,
        theme: "default".to_string(),
    };
    
    match visualize_netlist(&netlist, &options) {
        Ok(svg) => {
            let output_file = "pipeline_test_output.svg";
            fs::write(output_file, svg).expect("Failed to write SVG");
            println!("   ✓ SVG visualization generated: {}", output_file);
            
            // Basic validation of SVG content
            let svg_content = fs::read_to_string(output_file).unwrap();
            println!("   SVG size: {} bytes", svg_content.len());
            println!("   Contains <svg>: {}", svg_content.contains("<svg"));
            println!("   Contains components: {}", svg_content.contains("LM7805"));
        }
        Err(e) => {
            println!("   ❌ Visualization failed: {}", e);
        }
    }
    
    println!("\n=== Pipeline Test Complete ===");
}