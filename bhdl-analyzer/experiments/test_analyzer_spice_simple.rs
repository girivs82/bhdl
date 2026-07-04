use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use log::info;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("=== Testing Analyzer-SPICE Integration ===\n");

    // Test circuit with potential issues for SPICE to detect
    let input = r#"
board TestSpiceIntegration {
    power VCC = 5V @ 1A;
    ground GND;
    
    // LED without resistor - should be detected
    VCC -> led1: LED("red").A;
    led1.K -> GND;
    
    // LED with resistor
    VCC -> r1: Res(220).1;
    r1.2 -> led2: LED("green").A; 
    led2.K -> GND;
    
    // Voltage divider
    VCC -> r2: Res(10k).1;
    r2.2 @VDIV-> r3: Res(10k).1;
    r3.2 -> GND;
}
"#;

    // Step 1: Parse
    info!("Step 1: Parsing BHDL...");
    let parse_result = parse(input);
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parse_result.errors() {
            eprintln!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parsing failed"));
    }
    info!("✅ Parsing successful!");

    // Step 2: Analyze
    let syntax_tree = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_tree)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    info!("\nStep 2: Running semantic analysis...");
    let analysis_result = analyze(&source_file);
    
    // Print diagnostics
    if !analysis_result.diagnostics.is_empty() {
        info!("\nDiagnostics:");
        for diag in &analysis_result.diagnostics {
            info!("  - {}", diag.message);
        }
    }
    
    // Print power domains
    info!("\nPower domains:");
    for (name, domain) in &analysis_result.power_analysis.domains {
        info!("  - {}: {}V @ {}A", name, domain.voltage, domain.max_current);
    }
    
    // Print component inference from analyzer
    info!("\nComponent inference from analyzer:");
    for suggestion in &analysis_result.component_inference.inferred_components {
        info!("  - {} ({}): {}", 
            suggestion.component_type,
            suggestion.instance_name.as_ref().unwrap_or(&"unnamed".to_string()),
            suggestion.reasoning
        );
        
        // Check if this is about LED current limiting
        if suggestion.component_type == "Resistor" && suggestion.reasoning.contains("LED") {
            info!("    ⚠️  SAFETY ISSUE DETECTED: LED needs current limiting!");
        }
        
        for param in &suggestion.parameters {
            info!("    - {}: {} (confidence: {:.0}%)", 
                param.name, param.value, param.confidence * 100.0);
        }
    }
    
    // Print power analysis details
    info!("\nPower flow analysis:");
    for (component, domain_name) in &analysis_result.power_analysis.component_domains {
        info!("  - {} connected to domain: {}", component, domain_name);
    }
    
    // Check for potential issues
    info!("\n=== Analysis Summary ===");
    let led_without_resistor = analysis_result.component_inference.inferred_components.iter()
        .any(|s| s.reasoning.contains("LED") && s.reasoning.contains("current"));
    
    if led_without_resistor {
        info!("⚠️  Circuit has safety issues that need to be addressed!");
        info!("   - LED without current limiting resistor detected");
        info!("   - Analyzer suggests adding appropriate resistor");
    } else {
        info!("✅ Circuit appears to be properly designed");
    }
    
    info!("\nThe analyzer's component inference uses SPICE-like electrical models");
    info!("to detect circuit issues and suggest improvements.");
    
    Ok(())
}