/// Complete pipeline test with safety analysis
/// 
/// This test demonstrates the full BHDL flow including electrical safety analysis

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("=== Complete BHDL Pipeline Test with Safety Analysis ===\n");
    
    // Test case 1: Dangerous LED circuit (no resistor)
    let dangerous_source = r#"
board DangerousLED {
    power VCC = 5V @ 1A;
    ground GND;
    
    // DANGEROUS: LED directly connected to 5V
    VCC -> D1: LED(red).A;
    D1.K -> GND;
}
"#;
    
    println!("Test 1: Dangerous LED Circuit");
    println!("------------------------------");
    test_circuit(dangerous_source, "dangerous").await?;
    
    // Test case 2: Safe LED circuit (with resistor)
    let safe_source = r#"
board SafeLED {
    power VCC = 5V @ 1A;
    ground GND;
    
    // SAFE: LED with current limiting resistor
    VCC -> R1: Res(220).1;
    R1.2 -> D1: LED(red).A;
    D1.K -> GND;
}
"#;
    
    println!("\n\nTest 2: Safe LED Circuit");
    println!("------------------------");
    test_circuit(safe_source, "safe").await?;
    
    Ok(())
}

async fn test_circuit(source: &str, name: &str) -> Result<()> {
    // Step 1: Parse
    println!("\n1. Parsing...");
    let parse_result = parse(source);
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parse_result.errors() {
            eprintln!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parsing failed"));
    }
    
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    println!("   ✓ Parse successful");
    
    // Step 2: Initial Analysis
    println!("\n2. Running analyzer (passes 1-7)...");
    let analysis = analyze(&source_file);
    println!("   ✓ Analysis complete");
    println!("   - Diagnostics: {}", analysis.diagnostics.len());
    println!("   - Inferred components: {}", analysis.component_inference.get_inferred_components().len());
    println!("   - Power domains: {}", analysis.power_analysis.domains.len());
    
    // Step 3: Synthesize Netlist
    println!("\n3. Synthesizing netlist...");
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        database_path: Some("/Users/girivs/src/bhdl-new/components.db".to_string()),
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    println!("   ✓ Netlist generated");
    println!("   - Modules: {}", netlist.modules.len());
    println!("   - Instances: {}", netlist.instances.len());
    println!("   - Nets: {}", netlist.nets.len());
    
    // Debug: Print netlist structure
    println!("\n   Netlist Debug:");
    for (id, net) in &netlist.nets {
        println!("     Net {:?}: {:?} ({} connections)", id, net.name, net.connections.len());
    }
    for (id, inst) in &netlist.instances {
        if let Some(module) = netlist.modules.get(inst.definition) {
            println!("     Instance {:?}: {} (type: {})", id, inst.name, module.name);
        }
    }
    
    // Step 4: Safety analysis using bhdl-safety crate
    println!("\n4. Running safety analysis...");
    let safety_analyzer = bhdl_safety::SafetyAnalyzer::default();
    let safety_report = safety_analyzer.analyze(&netlist)?;
    
    println!("   ✓ Safety analysis complete");
    println!("   - Circuit status: {:?}", safety_report.circuit_status);
    println!("   - Total violations: {}", safety_report.summary.total_violations);
    println!("   - Critical: {}", safety_report.summary.critical_count);
    println!("   - Errors: {}", safety_report.summary.error_count);
    println!("   - Warnings: {}", safety_report.summary.warning_count);
    
    // Display violations
    if !safety_report.violations.is_empty() {
        println!("\n   Safety Violations:");
        for (i, violation) in safety_report.violations.iter().enumerate() {
            println!("   [{}] {} - {:?}", 
                     i + 1, 
                     violation.severity.as_str(),
                     violation.violation_type);
            println!("       {}", violation.message);
            if let Some(fix) = &violation.suggested_fix {
                println!("       Fix: {}", fix);
            }
        }
    } else {
        println!("   ✓ No safety violations found!");
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
    
    println!("\n   Summary:");
    println!("   - Circuit Status: {:?}", safety_report.circuit_status);
    println!("   - Total violations: {}", safety_report.summary.total_violations);
    println!("   - Risk components: {}", safety_report.component_risks.len());
    
    Ok(())
}