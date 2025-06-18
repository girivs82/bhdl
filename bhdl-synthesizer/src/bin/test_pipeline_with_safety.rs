/// Test the complete BHDL pipeline with electrical safety analysis
/// 
/// This demonstrates the full flow:
/// 1. Parse BHDL source
/// 2. Run analyzer passes (1-7)
/// 3. Synthesize netlist
/// 4. Run analyzer with netlist for safety analysis (Pass 8)

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_analyzer::{analyze, analyze_with_netlist};
use bhdl_synthesizer::NetlistGenerator;
use bhdl_ast::SourceFile;
use rowan::ast::AstNode;

fn main() -> Result<()> {
    env_logger::init();
    
    // Test case: LED without current limiting resistor
    let source = r#"
board PowerLED {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Dangerous: LED directly connected to 5V without resistor
    VCC -> D1: LED(red).A;
    D1.K -> GND;
}
"#;
    
    println!("=== Testing BHDL Pipeline with Safety Analysis ===\n");
    println!("Source code:");
    println!("{}", source);
    println!();
    
    // Step 1: Parse
    println!("Step 1: Parsing...");
    let parsed = parse(source);
    if !parsed.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parsed.errors() {
            eprintln!("  {}", error.message);
        }
        return Err(anyhow::anyhow!("Parsing failed"));
    }
    println!("✓ Parsing successful\n");
    
    // Step 2: Initial analysis (without netlist)
    println!("Step 2: Running analyzer passes 1-7...");
    // Convert to AST
    let syntax_node = rowan::SyntaxNode::<bhdl_parser::BhdlLanguage>::new_root(parsed.green_node);
    let source_file = SourceFile::cast(syntax_node).ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    let analysis_result = analyze(&source_file);
    println!("✓ Initial analysis complete. Diagnostics: {}\n", analysis_result.diagnostics.len());
    
    // Step 3: Synthesize netlist
    println!("Step 3: Synthesizing netlist...");
    let mut synthesizer = NetlistGenerator::new();
    let netlist = synthesizer.generate_from_ast(&source_file)?;
    println!("✓ Netlist synthesized. Modules: {}, Instances: {}, Nets: {}", 
             netlist.modules.len(), netlist.instances.len(), netlist.nets.len());
    
    // Debug: Print netlist details
    println!("\nNetlist Details:");
    for (id, module) in &netlist.modules {
        println!("  Module {:?}: {}", id, module.name);
    }
    for (id, instance) in &netlist.instances {
        println!("  Instance {:?}: {} (module: {:?})", id, instance.name, instance.definition);
    }
    println!();
    
    // Step 4: Re-analyze with netlist for safety
    println!("Step 4: Running analyzer with netlist for safety analysis...");
    let final_result = analyze_with_netlist(&source_file, netlist, None);
    
    // Display results
    println!("\n=== Analysis Results ===\n");
    
    if let Some(ref safety) = final_result.safety_analysis {
        println!("Safety Analysis Results:");
        println!("  Violations: {}", safety.violations.len());
        println!("  Suggested fixes: {}", safety.suggested_fixes.len());
        println!("  Warnings: {}", safety.warnings.len());
        
        if !safety.violations.is_empty() {
            println!("\nSafety Violations:");
            for (i, violation) in safety.violations.iter().enumerate() {
                println!("\n  [{}] {:?} - {}", 
                         i + 1, 
                         violation.severity,
                         violation.component);
                println!("      Message: {}", violation.message);
                println!("      Details: {}", violation.technical_details);
                
                // Show suggested fix if available
                if let Some(fix) = safety.suggested_fixes.iter()
                    .find(|f| f.violation_index == i) {
                    println!("      Fix: {}", fix.description);
                }
            }
        }
        
        if !safety.warnings.is_empty() {
            println!("\nSafety Warnings:");
            for warning in &safety.warnings {
                println!("  - {}", warning);
            }
        }
    } else {
        println!("No safety analysis was performed (unexpected)");
    }
    
    // All diagnostics including safety
    println!("\nAll Diagnostics: {}", final_result.diagnostics.len());
    for diag in &final_result.diagnostics {
        println!("  - {}", diag.message);
    }
    
    // Test with protected LED
    println!("\n\n=== Testing Protected LED Circuit ===\n");
    
    let safe_source = r#"
board SafeLED {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Safe: LED with current limiting resistor
    VCC -> R1: Res(220).1;
    R1.2 -> D1: LED(red).A;
    D1.K -> GND;
}
"#;
    
    let parsed_safe = parse(safe_source);
    if !parsed_safe.errors().is_empty() {
        return Err(anyhow::anyhow!("Parsing safe circuit failed"));
    }
    
    let syntax_node_safe = rowan::SyntaxNode::<bhdl_parser::BhdlLanguage>::new_root(parsed_safe.green_node);
    let source_file_safe = SourceFile::cast(syntax_node_safe).ok_or_else(|| anyhow::anyhow!("Failed to cast safe to SourceFile"))?;
    
    let analysis_safe = analyze(&source_file_safe);
    let mut synthesizer_safe = NetlistGenerator::new();
    let netlist_safe = synthesizer_safe.generate_from_ast(&source_file_safe)?;
    let final_safe = analyze_with_netlist(&source_file_safe, netlist_safe, None);
    
    if let Some(ref safety) = final_safe.safety_analysis {
        println!("Safety Analysis Results:");
        println!("  Violations: {}", safety.violations.len());
        
        if safety.violations.is_empty() {
            println!("  ✓ No safety violations!");
        } else {
            for violation in &safety.violations {
                println!("  - {:?}: {}", violation.severity, violation.message);
            }
        }
    }
    
    Ok(())
}