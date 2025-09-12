use std::fs;
use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing synthesizer symbol table integration...\n");
    
    // Load test file with imports
    let source_content = fs::read_to_string("simple_test_main.bhdl")?;
    
    println!("=== Source File ===");
    println!("{}", source_content);
    
    // Parse
    let parse_result = parse(&source_content);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Analyze
    println!("\n=== Analysis Phase ===");
    let analysis = analyze(&source_file);
    
    println!("Analyzed with {} symbols in global scope", analysis.global_scope.get_symbols().len());
    println!("Found {} diagnostics", analysis.diagnostics.len());
    
    // Test synthesizer
    println!("\n=== Synthesizer Test ===");
    let mut generator = NetlistGenerator::with_config(NetlistConfig::default());
    
    // The key test: can the synthesizer access the imported modules?
    match generator.generate_from_ast_and_analysis(&source_file, &analysis).await {
        Ok(netlist) => {
            println!("✅ Synthesizer successfully generated netlist!");
            println!("Modules: {}", netlist.modules.len());
            println!("Instances: {}", netlist.instances.len());
            println!("Nets: {}", netlist.nets.len());
            
            // Show the synthesizer's debug output about symbol table usage
            for (module_id, module) in &netlist.modules {
                println!("Module: {} (ID: {:?})", module.name, module_id);
            }
        }
        Err(e) => {
            println!("❌ Synthesizer failed: {}", e);
        }
    }
    
    Ok(())
}