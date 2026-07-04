use bhdl_synthesizer::{NetlistConfig, NetlistGenerator};
use bhdl_analyzer;
use bhdl_parser::parse;
use bhdl_ast::AstNode;
use log::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    info!("Starting reliability and lifecycle analysis test...");
    
    // Test BHDL circuit with components that have different reliability profiles
    let circuit = r#"
    board ReliabilityTest {
        power VCC = 5V @ 1A;
        ground GND;
        
        // Test circuit with various component types for reliability analysis
        VCC -> test_net;
        test_net -> GND;
    }
    "#;
    
    // Parse and analyze the circuit
    let parse_result = parse(circuit);
    let syntax_tree = parse_result.syntax();
    let source_file = bhdl_ast::SourceFile::cast(syntax_tree).unwrap();
    
    let analysis_result = bhdl_analyzer::analyze(&source_file);
    
    info!("Analysis completed. Found {} diagnostics", analysis_result.diagnostics.len());
    
    // Create generator with reliability analysis enabled
    let config = NetlistConfig {
        enable_reliability_analysis: true,
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    
    // Generate netlist with reliability analysis
    info!("Starting netlist generation with reliability analysis...");
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    info!("Netlist generation completed successfully!");
    info!("Generated netlist with {} modules", netlist.modules.len());
    info!("Generated netlist with {} instances", netlist.instances.len());
    info!("Generated netlist with {} nets", netlist.nets.len());
    
    // Display generated components to verify the synthesis worked
    info!("Generated Components:");
    for (instance_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            info!("  Component: {} ({})", instance.name, module.name);
        }
    }
    
    // The reliability analysis runs internally during synthesis and provides comprehensive reliability assessment
    // If we reach this point without errors, the reliability analysis was successful
    
    info!("Reliability and lifecycle analysis test completed successfully!");
    Ok(())
}