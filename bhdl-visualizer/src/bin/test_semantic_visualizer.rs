//! Test the semantic visualizer with a linear regulator circuit

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_visualizer::{render_semantic_circuit, generate_semantic_svg};
use log::{info, error};

const TEST_CIRCUIT: &str = r#"
board LinearRegulatorTest {
    // Power input and output
    net VIN;
    net VOUT;
    net GND;
    
    // Input section
    VIN -> C1: Cap(10µF).1;
    C1.2 -> GND;
    
    // Regulator
    VIN -> U1: LM7805().IN;
    U1.GND -> GND;
    U1.OUT -> VOUT;
    
    // Output section
    VOUT -> C2: Cap(10µF).1;
    C2.2 -> GND;
    
    // LED indicator
    VOUT -> R1: Res(330Ω).1;
    R1.2 -> D1: LED(red).A;
    D1.K -> GND;
}
"#;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
    
    info!("🚀 Testing semantic visualizer with linear regulator circuit");
    
    // Parse the BHDL source
    info!("Parsing BHDL source...");
    let parsed = parse(TEST_CIRCUIT);
    let source_file = SourceFile::cast(parsed.syntax()).expect("Failed to parse source file");
    
    // Analyze the circuit
    info!("Analyzing circuit...");
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        error!("Analysis produced {} diagnostics:", analysis.diagnostics.len());
        for diag in &analysis.diagnostics {
            error!("  {}", diag.message);
        }
    }
    
    // Generate netlist with database components
    info!("Generating netlist with database components...");
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: true,
        database_path: Some("/Users/girivs/src/bhdl-new/components.db".to_string()),
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    let component_instances = generator.get_component_instances().to_vec();
    
    info!("Generated netlist with {} instances and {} database components", 
          netlist.instances.len(), component_instances.len());
    
    // Generate semantic layout
    info!("Generating semantic circuit layout...");
    let layout = render_semantic_circuit(netlist, component_instances)?;
    
    info!("Layout complete: {} components, {} nets", 
          layout.components.len(), layout.nets.len());
    
    // Generate SVG
    let output_path = "semantic_regulator_test.svg";
    generate_semantic_svg(&layout, output_path)?;
    
    info!("✅ Semantic visualization complete! SVG saved to: {}", output_path);
    
    // Print layout statistics
    info!("Layout bounds: {:.0}x{:.0}", 
          layout.bounding_box.width(), 
          layout.bounding_box.height());
    
    Ok(())
}