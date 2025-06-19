/// End-to-end test for safety analysis using real BHDL files
/// 
/// This test loads BHDL files from disk and runs them through the complete
/// pipeline: parse -> analyze -> synthesize -> safety analysis

use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("=== End-to-End Safety Analysis Test ===\n");
    
    // Test dangerous LED circuit
    println!("Test 1: Dangerous LED Circuit");
    println!("==============================");
    test_file("tests/circuits/safety/dangerous_led.bhdl").await?;
    
    println!("\n\nTest 2: Safe LED Circuit");
    println!("========================");
    test_file("tests/circuits/safety/safe_led.bhdl").await?;
    
    Ok(())
}

async fn test_file(file_path: &str) -> Result<()> {
    // Load BHDL file
    println!("\n1. Loading file: {}", file_path);
    let source = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read {}", file_path))?;
    
    println!("   Source:\n{}", source.lines()
        .map(|l| format!("   {}", l))
        .collect::<Vec<_>>()
        .join("\n"));
    
    // Parse
    println!("\n2. Parsing...");
    let parse_result = parse(&source);
    if !parse_result.errors().is_empty() {
        for error in parse_result.errors() {
            eprintln!("   Parse error: {}", error.message);
        }
        return Err(anyhow::anyhow!("Parsing failed"));
    }
    
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    println!("   ✓ Parse successful");
    
    // Analyze (passes 1-7)
    println!("\n3. Running analyzer (passes 1-7)...");
    let analysis = analyze(&source_file);
    println!("   ✓ Analysis complete");
    println!("   - Diagnostics: {}", analysis.diagnostics.len());
    for diag in &analysis.diagnostics {
        println!("     • {}", diag.message);
    }
    
    // Synthesize netlist
    println!("\n4. Synthesizing netlist...");
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        database_path: None,
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    println!("   ✓ Netlist generated");
    println!("   - Modules: {}", netlist.modules.len());
    println!("   - Instances: {}", netlist.instances.len());
    println!("   - Nets: {}", netlist.nets.len());
    
    // Debug netlist
    println!("\n   Netlist details:");
    for (_id, module) in &netlist.modules {
        println!("     Module: {}", module.name);
    }
    for (_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            println!("     Instance: {} (type: {})", instance.name, module.name);
        }
    }
    for (_id, net) in &netlist.nets {
        if let Some(name) = &net.name {
            println!("     Net: {} ({} connections)", name, net.connections.len());
        }
    }
    
    // Run safety analysis using bhdl-safety crate
    println!("\n5. Running safety analysis...");
    let safety_analyzer = bhdl_safety::SafetyAnalyzer::default();
    let safety_report = safety_analyzer.analyze(&netlist)?;
    
    println!("   ✓ Safety analysis complete");
    println!("   - Circuit status: {:?}", safety_report.circuit_status);
    println!("   - Total violations: {}", safety_report.summary.total_violations);
    
    // Show violations
    if !safety_report.violations.is_empty() {
        println!("\n   Safety Violations:");
        for (i, violation) in safety_report.violations.iter().enumerate() {
            println!("   [{}] {} - {:?}", 
                     i + 1, 
                     violation.severity.as_str(),
                     violation.violation_type);
            println!("       Message: {}", violation.message);
            
            if let Some(fix) = &violation.suggested_fix {
                println!("       Suggested fix: {}", fix);
            }
        }
    } else {
        println!("   ✓ No safety violations detected!");
    }
    
    // Show component risks
    if !safety_report.component_risks.is_empty() {
        println!("\n   Component Risks:");
        for (component, risk) in &safety_report.component_risks {
            println!("     {} - Risk Level: {:?}", component, risk.risk_level);
            for issue in &risk.issues {
                println!("       • {}", issue);
            }
        }
    }
    
    // Summary
    println!("\n   Summary:");
    println!("   - Circuit Status: {:?}", safety_report.circuit_status);
    println!("   - Risk Components: {}", safety_report.component_risks.len());
    
    Ok(())
}