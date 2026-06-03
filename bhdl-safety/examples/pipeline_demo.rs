//! Demo of the complete BHDL pipeline with safety analysis
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
    
    // Load test file
    let source = r#"
board DangerousLED {
    power VCC = 5V @ 1A;
    ground GND;
    
    // DANGEROUS: LED directly connected to 5V
    VCC -> D1: LED(red).A;
    D1.K -> GND;
}
"#;
    
    println!("=== BHDL Pipeline Demo ===\n");
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
    
    // Step 2: Analyze (semantic analysis)
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
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        database_path: Some("/Users/girivs/src/bhdl-new/components.db".to_string()),
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    println!("   ✓ Netlist generated");
    println!("   - Modules: {}", netlist.modules.len());
    println!("   - Instances: {}", netlist.instances.len());
    println!("   - Nets: {}", netlist.nets.len());
    println!();
    
    // Step 4: Safety Analysis (NEW - separate step)
    println!("4. Running safety analysis...");
    let safety_analyzer = SafetyAnalyzer::default();
    let safety_report = safety_analyzer.analyze(&netlist)?;
    
    println!("   ✓ Safety analysis complete");
    println!("   - Circuit status: {:?}", safety_report.circuit_status);
    println!("   - Total violations: {}", safety_report.summary.total_violations);
    println!("     • Critical: {}", safety_report.summary.critical_count);
    println!("     • Errors: {}", safety_report.summary.error_count);
    println!("     • Warnings: {}", safety_report.summary.warning_count);
    
    if !safety_report.violations.is_empty() {
        println!("\n   Safety Violations:");
        for (i, violation) in safety_report.violations.iter().enumerate() {
            println!("\n   [{}] {} - {:?}", 
                     i + 1, 
                     violation.severity.as_str(),
                     violation.violation_type);
            println!("       {}", violation.message);
            if let Some(fix) = &violation.suggested_fix {
                println!("       Fix: {}", fix);
            }
        }
    }
    
    // Show component risks
    if !safety_report.component_risks.is_empty() {
        println!("\n   Component Risk Assessment:");
        for (component, risk) in &safety_report.component_risks {
            println!("     {} - Risk Level: {:?}", component, risk.risk_level);
            for issue in &risk.issues {
                println!("       • {}", issue);
            }
        }
    }
    
    println!("\n✨ Pipeline complete!");
    
    Ok(())
}