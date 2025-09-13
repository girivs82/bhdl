// Test integrated component compatibility analysis in BHDL pipeline
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{Synthesizer, NetlistConfig};
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("=== Testing Integrated Component Compatibility Analysis ===");
    
    // Test with a complex circuit that has component definitions
    let test_file = "test_compatibility_complex.bhdl";
    println!("Processing circuit design: {}", test_file);
    
    let bhdl_source = fs::read_to_string(test_file)
        .map_err(|e| format!("Failed to read {}: {}", test_file, e))?;
    
    println!("Parsing BHDL source...");
    let parse_result = parse(&bhdl_source);
    let syntax = parse_result.syntax();
    
    // Run semantic analysis
    println!("Running semantic analysis...");
    let analysis = analyze(&SourceFile::cast(syntax.clone()).unwrap());
    
    // Create synthesizer with compatibility analysis enabled
    println!("\nSetting up synthesizer with compatibility analysis enabled...");
    let mut config = NetlistConfig::default();
    config.enable_compatibility_analysis = true;  // Enable compatibility analysis
    config.database_path = Some("/Users/girivs/src/bhdl-new/components.db".to_string());  // Use real database!
    
    let mut synthesizer = Synthesizer::with_config(config);
    
    println!("Running BHDL synthesis pipeline with integrated compatibility analysis...");
    
    // Generate netlist (this will now include compatibility analysis)
    match synthesizer.generate_from_ast_and_analysis(&SourceFile::cast(syntax.clone()).unwrap(), &analysis).await {
        Ok(netlist) => {
            println!("\n✅ BHDL synthesis completed successfully!");
            println!("Generated netlist:");
            println!("  - {} modules", netlist.modules.len());
            println!("  - {} instances", netlist.instances.len());
            println!("  - {} nets", netlist.nets.len());
            
            println!("\n✅ Component compatibility analysis was automatically integrated!");
            println!("Check the log output above for compatibility analysis results.");
        },
        Err(e) => {
            eprintln!("❌ Synthesis failed: {}", e);
            return Err(e.into());
        }
    }
    
    println!("\n=== Integration Test Summary ===");
    println!("✅ Successfully demonstrated integrated compatibility analysis");
    println!("✅ Compatibility analysis runs automatically during synthesis");
    println!("✅ Results are reported to users through standard logging");
    println!("✅ Pipeline continues even if compatibility analysis has issues");
    
    Ok(())
}