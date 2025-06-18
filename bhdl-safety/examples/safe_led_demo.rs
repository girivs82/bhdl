//! Demo of safe LED circuit with current limiting resistor
//! 
//! Flow: Parse -> Analyze -> Synthesize -> Safety Analysis

use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_safety::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    // Safe LED circuit with proper current limiting
    let source = r#"
board SafeLED {
    power VCC = 5V @ 1A;
    ground GND;
    
    // SAFE: LED with current limiting resistor
    VCC -> R1: Res(220Ω).1;
    R1.2 -> D1: LED(red).A;
    D1.K -> GND;
}
"#;
    
    println!("=== Safe LED Circuit Demo ===\n");
    println!("Source:\n{}\n", source);
    
    // Step 1: Parse
    println!("1. Parsing...");
    let parse_result = parse(source);
    if !parse_result.errors().is_empty() {
        anyhow::bail!("Parse errors: {:?}", parse_result.errors());
    }
    
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .context("Failed to cast to SourceFile")?;
    println!("   ✓ Parse successful\n");
    
    // Step 2: Analyze
    println!("2. Analyzing...");
    let analysis = analyze(&source_file);
    println!("   ✓ Analysis complete");
    println!("   - Diagnostics: {}", analysis.diagnostics.len());
    for diag in &analysis.diagnostics {
        println!("     • {}", diag.message);
    }
    println!();
    
    // Step 3: Synthesize netlist
    println!("3. Synthesizing netlist...");
    let config = NetlistConfig::default();
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    println!("   ✓ Netlist generated");
    println!("   - Modules: {}", netlist.modules.len());
    println!("   - Instances: {}", netlist.instances.len());
    println!("   - Nets: {}", netlist.nets.len());
    println!();
    
    // Step 4: Safety Analysis
    println!("4. Running safety analysis...");
    let safety_analyzer = SafetyAnalyzer::default();
    let safety_report = safety_analyzer.analyze(&netlist)?;
    
    println!("   ✓ Safety analysis complete");
    println!("   - Circuit status: {:?}", safety_report.circuit_status);
    println!("   - Total violations: {}", safety_report.summary.total_violations);
    
    if safety_report.is_safe() {
        println!("\n🎉 Circuit is SAFE! LED is properly protected with current limiting resistor.");
    } else {
        println!("\n⚠️  Circuit has safety issues:");
        for violation in &safety_report.violations {
            println!("   - {}: {}", violation.severity.as_str(), violation.message);
        }
    }
    
    Ok(())
}