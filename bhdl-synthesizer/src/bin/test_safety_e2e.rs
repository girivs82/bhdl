/// End-to-end test for safety analysis using real BHDL files
/// 
/// This test loads BHDL files from disk and runs them through the complete
/// pipeline: parse -> analyze -> synthesize -> safety analysis

use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::{analyze, analyze_with_netlist};
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
        use_database_components: false,
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
    
    // Re-analyze with netlist for safety
    println!("\n5. Running safety analysis...");
    let final_analysis = analyze_with_netlist(&source_file, netlist, None);
    
    if let Some(ref safety) = final_analysis.safety_analysis {
        println!("   ✓ Safety analysis complete");
        println!("   - Violations: {}", safety.violations.len());
        println!("   - Suggested fixes: {}", safety.suggested_fixes.len());
        println!("   - Warnings: {}", safety.warnings.len());
        
        // Show violations
        if !safety.violations.is_empty() {
            println!("\n   Safety Violations:");
            for (i, violation) in safety.violations.iter().enumerate() {
                println!("   [{}] {:?} - {}", 
                         i + 1, 
                         violation.severity,
                         violation.component);
                println!("       Message: {}", violation.message);
                println!("       Technical: {}", violation.technical_details);
                
                if let Some(fix) = safety.suggested_fixes.iter()
                    .find(|f| f.violation_index == i) {
                    println!("       Suggested fix: {}", fix.description);
                }
            }
        } else if safety.warnings.is_empty() {
            println!("   ✓ No safety violations detected!");
        }
        
        // Show warnings
        if !safety.warnings.is_empty() {
            println!("\n   Warnings:");
            for warning in &safety.warnings {
                println!("   - {}", warning);
            }
        }
    } else {
        println!("   ⚠ Safety analysis was not performed");
    }
    
    // Summary
    let safety_diagnostics = final_analysis.diagnostics.iter()
        .filter(|d| d.message.contains("[CRITICAL]") || 
                    d.message.contains("[ERROR]") || 
                    d.message.contains("[WARNING]"))
        .count();
    
    println!("\n   Summary:");
    println!("   - Total diagnostics: {}", final_analysis.diagnostics.len());
    println!("   - Safety-related: {}", safety_diagnostics);
    
    Ok(())
}