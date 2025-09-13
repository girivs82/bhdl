use bhdl_synthesizer::{NetlistConfig, NetlistGenerator};
use bhdl_analyzer;
use bhdl_parser::parse;
use bhdl_ast::AstNode;
use log::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    info!("Starting EMI/EMC analysis test...");
    
    // Test BHDL circuit with basic components to verify EMI/EMC analysis runs
    let circuit = r#"
    board TestEMI {
        power VCC = 5V @ 1A;
        ground GND;
        
        // Simple test circuit - just verify analysis runs without component definition issues
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
    
    // Create generator with EMI/EMC analysis enabled
    let config = NetlistConfig {
        enable_emi_emc_analysis: true,
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    
    // Generate netlist with EMI/EMC analysis
    info!("Starting netlist generation with EMI/EMC analysis...");
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
    
    // The EMI/EMC analysis runs internally during synthesis and affects the netlist generation
    // If we reach this point without errors, the EMI/EMC analysis was successful
    
    info!("EMI/EMC analysis test completed successfully!");
    Ok(())
}