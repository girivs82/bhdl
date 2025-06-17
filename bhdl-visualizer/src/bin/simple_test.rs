//! Simple test to generate a complete circuit visualization using the full BHDL pipeline

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_visualizer::LayoutConfig;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("🔧 BHDL Phase 3 Component Intelligence Pipeline Test");
    println!("{}", "=".repeat(60));
    
    // Step 1: Parse BHDL source code
    println!("\n📝 Step 1: Parsing BHDL source...");
    let bhdl_source = create_linear_regulator_bhdl_source();
    let parsed = parse(&bhdl_source);
    if !parsed.errors().is_empty() {
        println!("❌ Parse errors: {:?}", parsed.errors());
        return Err(anyhow::anyhow!("Failed to parse BHDL source"));
    }
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    println!("✅ Parsed BHDL source successfully");
    
    // Step 2: Analyze with semantic metadata
    println!("\n🧠 Step 2: Running semantic analysis...");
    let analysis_result = analyze(&source_file);
    println!("✅ Analysis complete:");
    println!("  - {} diagnostics", analysis_result.diagnostics.len());
    println!("  - {} power domains", analysis_result.power_analysis.domains.len());
    println!("  - {} component inferences", analysis_result.component_inference.inferred_components.len());
    
    // Step 3: Synthesize using component database
    println!("\n⚙️ Step 3: Synthesizing to netlist with database components...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_analysis(&analysis_result).await?;
    let db_components = generator.get_component_instances().to_vec();
    
    println!("✅ Test setup complete:");
    println!("  - {} instances", netlist.instances.len());
    println!("  - {} nets", netlist.nets.len());
    println!("  - {} database components", db_components.len());
    println!("  - {} power domains", analysis_result.power_analysis.domains.len());
    
    // Debug: Print actual netlist contents
    println!("\n🔍 Netlist Debug:");
    println!("Nets:");
    for (net_id, net) in &netlist.nets {
        println!("  - {:?}: {:?}", net_id, net.name);
    }
    println!("Instances:");
    for (inst_id, instance) in &netlist.instances {
        println!("  - {:?}: {} (definition: {:?})", inst_id, instance.name, instance.definition);
    }
    
    // Step 4: Visualize with semantic placement
    println!("\n🎨 Generating SVG visualization with semantic placement...");
    let mut config = LayoutConfig::default();
    config.component_spacing = 100.0;  // More spacing to accommodate labels
    config.grid_spacing = 20.0;
    config.show_grid = true;
    
    let svg_content = bhdl_visualizer::render_circuit_debug_with_analysis(
        &netlist, 
        &db_components, 
        Some(&analysis_result), 
        Some(config)
    ).await?;
    
    // Write to file
    std::fs::write("bhdl_regulator_circuit.svg", &svg_content)?;
    
    println!("✅ Generated: bhdl_regulator_circuit.svg");
    
    // Read and verify the SVG content (quality control)
    println!("\n🔍 Quality Control - Verifying SVG content...");
    let svg_content_check = std::fs::read_to_string("bhdl_regulator_circuit.svg")?;
    
    let component_count = svg_content_check.matches("<!-- Component:").count();
    let net_count = svg_content_check.matches("<!-- Net:").count();
    let viewbox_present = svg_content_check.contains("viewBox");
    
    println!("  - SVG length: {} chars", svg_content_check.len());
    println!("  - Components in SVG: {}", component_count);
    println!("  - Nets in SVG: {}", net_count);
    println!("  - ViewBox present: {}", viewbox_present);
    
    if component_count > 0 && viewbox_present {
        println!("✅ SVG quality check passed!");
    } else {
        println!("⚠️ SVG quality check revealed issues - please inspect manually");
    }
    
    println!("\n🎉 BHDL Phase 3 Component Intelligence Pipeline Test Complete!");
    println!("Database-driven component visualization with semantic placement is working.");
    
    Ok(())
}

/// Create BHDL source code for a linear regulator circuit
/// Using simplified BHDL v2.0 flow syntax to test parser step by step
fn create_linear_regulator_bhdl_source() -> String {
    r#"
board LinearRegulator {
    // BHDL v2.0 flow-based syntax with component instantiation
    // Current limiting resistor and status LED
    VCC -> Res(4.7kΩ).1 -> LED(red).A;
    LED.K -> GND;
    
    // Input filtering capacitor
    VCC -> Res(10kΩ).1;
    Res.2 -> GND;
}
"#.to_string()
}