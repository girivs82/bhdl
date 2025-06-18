/// Test the complete BHDL pipeline with electrical safety analysis
/// 
/// This demonstrates the full flow:
/// 1. Parse BHDL source
/// 2. Run analyzer passes (1-7)
/// 3. Synthesize netlist externally
/// 4. Run safety analysis

use anyhow::Result;
use bhdl_parser::parse_source;
use bhdl_analyzer::{analyze, analyze_with_netlist};
use bhdl_synthesizer::Synthesizer;
use log::info;

fn main() -> Result<()> {
    env_logger::init();
    
    // Test case: LED without current limiting resistor
    let source = r#"
board PowerLED {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Dangerous: LED directly connected to 5V without resistor
    VCC -> LED(red).A;
    LED.K -> GND;
}
"#;
    
    println!("=== Testing BHDL Pipeline with Safety Analysis ===\n");
    println!("Source code:");
    println!("{}", source);
    println!();
    
    // Step 1: Parse
    println!("Step 1: Parsing...");
    let parsed = parse_source(source);
    if !parsed.errors.is_empty() {
        eprintln!("Parse errors:");
        for error in &parsed.errors {
            eprintln!("  {}", error);
        }
        return Err(anyhow::anyhow!("Parsing failed"));
    }
    println!("✓ Parsing successful\n");
    
    // Step 2: Initial analysis (without netlist)
    println!("Step 2: Running analyzer passes 1-7...");
    let mut analysis_result = analyze(&parsed.tree);
    println!("✓ Analysis complete. Diagnostics: {}\n", analysis_result.diagnostics.len());
    
    // Step 3: Synthesize netlist
    println!("Step 3: Synthesizing netlist...");
    let mut synthesizer = Synthesizer::new();
    let netlist = synthesizer.generate_from_ast(&parsed.tree)?;
    println!("✓ Netlist synthesized. Modules: {}, Instances: {}, Nets: {}\n", 
             netlist.modules.len(), netlist.instances.len(), netlist.nets.len());
    
    // Step 4: Re-analyze with netlist for safety
    println!("Step 4: Running safety analysis...");
    analysis_result = analyze_with_netlist(&parsed.tree, netlist, None);
    
    // Display results
    println!("\n=== Analysis Results ===\n");
    
    if let Some(ref safety) = analysis_result.safety_analysis {
        println!("Safety Violations: {}", safety.violations.len());
        for (i, violation) in safety.violations.iter().enumerate() {
            println!("\n[{}] {:?} - {} ({})", 
                     i + 1, 
                     violation.severity,
                     violation.message,
                     violation.component);
            println!("    Technical: {}", violation.technical_details);
            
            // Show suggested fix if available
            if let Some(fix) = safety.suggested_fixes.iter()
                .find(|f| f.violation_index == i) {
                println!("    Suggested Fix: {}", fix.description);
            }
        }
        
        if !safety.warnings.is_empty() {
            println!("\nSafety Warnings:");
            for warning in &safety.warnings {
                println!("  - {}", warning);
            }
        }
    } else {
        println!("No safety analysis performed");
    }
    
    println!("\nTotal Diagnostics: {}", analysis_result.diagnostics.len());
    for diag in &analysis_result.diagnostics {
        println!("  - {}", diag.message);
    }
    
    Ok(())
}