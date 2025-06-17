//! Test semantic visualizer with actual pipeline

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_visualizer::{render_semantic_circuit, generate_semantic_svg};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing semantic visualizer with real pipeline...");
    
    // Simple test circuit
    let bhdl_source = r#"
board TestRegulator {
    net VIN;
    net VOUT;
    net GND;
    
    VIN -> C1: Cap(10µF).1;
    C1.2 -> GND;
    
    VIN -> U1: LM7805().IN;
    U1.GND -> GND;
    U1.OUT -> VOUT;
    
    VOUT -> C2: Cap(10µF).1;
    C2.2 -> GND;
}
"#;

    // Parse
    let parsed = parse(bhdl_source);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    // Analyze
    let analysis = analyze(&source_file);
    
    // Generate netlist with database components
    let mut generator = NetlistGenerator::with_config(
        NetlistConfig {
            use_database_components: true,
            database_path: Some("/Users/girivs/src/bhdl-new/components.db".to_string()),
            ..Default::default()
        }
    );
    
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    let components = generator.get_component_instances().to_vec();
    
    println!("Generated netlist with {} components", components.len());
    
    // Print component info
    for comp in &components {
        println!("\nComponent {}: {} ({})", comp.instance_name, comp.component_name, comp.bhdl_type);
        println!("  SVG data length: {}", comp.svg_data.len());
        println!("  Has {} pins", comp.pins.len());
        for pin in &comp.pins {
            println!("    Pin {}: pos({:?}, {:?})", 
                pin.pin_number, 
                pin.x_position, 
                pin.y_position);
        }
    }
    
    // Try semantic visualization
    let layout = render_semantic_circuit(netlist, components)?;
    
    println!("\nLayout has {} components, {} nets", layout.components.len(), layout.nets.len());
    
    // Check component positions
    for comp in &layout.components {
        println!("Component at ({}, {})", comp.position.x, comp.position.y);
        println!("  Size: {}x{}", comp.size.x, comp.size.y);
        println!("  Pins: {:?}", comp.pins.keys().collect::<Vec<_>>());
    }
    
    // Generate SVG
    generate_semantic_svg(&layout, "test_semantic_output.svg")?;
    
    // Read and check the SVG
    let svg_content = std::fs::read_to_string("test_semantic_output.svg")?;
    
    // Check for key issues
    println!("\n=== SVG ANALYSIS ===");
    println!("SVG length: {} chars", svg_content.len());
    println!("Contains <svg>: {}", svg_content.contains("<svg"));
    println!("Contains viewBox: {}", svg_content.contains("viewBox"));
    println!("Contains transform: {}", svg_content.contains("transform"));
    println!("Contains <rect>: {} times", svg_content.matches("<rect").count());
    println!("Contains <path>: {} times", svg_content.matches("<path").count());
    println!("Contains <line>: {} times", svg_content.matches("<line").count());
    
    // Look for specific component markers
    println!("\nComponent SVG usage:");
    println!("Contains 'symbol-line': {}", svg_content.contains("symbol-line"));
    println!("Contains 'parallel plate': {}", svg_content.contains("plate"));
    
    // Save first 2000 chars for inspection
    println!("\n=== FIRST 2000 CHARS OF SVG ===");
    println!("{}", &svg_content[..svg_content.len().min(2000)]);
    
    Ok(())
}