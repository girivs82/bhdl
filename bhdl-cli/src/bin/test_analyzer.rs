// Test semantic analysis with bhdl-analyzer

use std::fs;
use bhdl_ast::{AstNode, SourceFile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL Semantic Analysis Test ===\n");
    
    // Step 1: Parse BHDL file
    println!("Step 1: Parsing BHDL file...");
    let bhdl_content = fs::read_to_string("examples/7805_regulator_v2.bhdl")?;
    let parse_result = bhdl_parser::parse(&bhdl_content);
    
    // Check for parse errors
    let errors = parse_result.errors();
    if !errors.is_empty() {
        eprintln!("❌ Parse errors found:");
        for error in errors {
            eprintln!("  - {}", error.message);
        }
        return Err("Parsing failed".into());
    }
    println!("✅ Parsing successful!");
    
    // Step 2: Convert to AST
    println!("\nStep 2: Converting to AST...");
    let syntax_tree = parse_result.syntax();
    let ast = SourceFile::cast(syntax_tree.clone())
        .ok_or("Failed to convert syntax tree to SourceFile")?;
    println!("✅ AST conversion successful!");
    
    // Step 3: Run semantic analysis
    println!("\nStep 3: Running semantic analysis...");
    let analysis_result = bhdl_analyzer::analyze(&ast);
    
    // Print analysis summary
    println!("\n📊 Analysis Results:");
    println!("  Global symbols: {}", analysis_result.global_scope.get_symbols().len());
    println!("  Definition scopes: {}", analysis_result.definition_scopes.len());
    println!("  Resolved constants: {}", analysis_result.resolved_constants.len());
    println!("  Power domains: {}", analysis_result.power_analysis.domains.len());
    println!("  Level shifters: {}", analysis_result.power_analysis.level_shifted_signals.len());
    println!("  Inferred components: {}", analysis_result.component_inference.get_inferred_components().len());
    
    // Show diagnostics
    if !analysis_result.diagnostics.is_empty() {
        println!("\n⚠️  Diagnostics found: {}", analysis_result.diagnostics.len());
        for (i, diag) in analysis_result.diagnostics.iter().enumerate() {
            println!("  {}. {}", i + 1, diag.message);
            if i >= 9 { 
                println!("  ... and {} more", analysis_result.diagnostics.len() - 10);
                break;
            }
        }
    } else {
        println!("\n✅ No diagnostics found!");
    }
    
    // Show power domains
    if !analysis_result.power_analysis.domains.is_empty() {
        println!("\n⚡ Power Domains:");
        for (name, domain) in &analysis_result.power_analysis.domains {
            println!("  - {}: {}V @ {}A", name, domain.voltage, domain.max_current);
        }
    }
    
    // Show inferred components
    let inferred = analysis_result.component_inference.get_inferred_components();
    if !inferred.is_empty() {
        println!("\n🔍 Inferred Components:");
        for (i, comp) in inferred.iter().enumerate() {
            if i >= 5 {
                println!("  ... and {} more", inferred.len() - 5);
                break;
            }
            // Find value parameter if it exists
            let value_str = comp.parameters.iter()
                .find(|p| p.name == "value")
                .map(|p| match &p.value {
                    bhdl_analyzer::component_inference::ParameterValue::Resistance(r) => format!("{}Ω", r),
                    bhdl_analyzer::component_inference::ParameterValue::Capacitance(c) => format!("{}F", c),
                    bhdl_analyzer::component_inference::ParameterValue::Inductance(l) => format!("{}H", l),
                    bhdl_analyzer::component_inference::ParameterValue::Voltage(v) => format!("{}V", v),
                    bhdl_analyzer::component_inference::ParameterValue::Current(i) => format!("{}A", i),
                    bhdl_analyzer::component_inference::ParameterValue::Frequency(f) => format!("{}Hz", f),
                    bhdl_analyzer::component_inference::ParameterValue::Power(p) => format!("{}W", p),
                    bhdl_analyzer::component_inference::ParameterValue::String(s) => s.clone(),
                    bhdl_analyzer::component_inference::ParameterValue::Integer(i) => i.to_string(),
                    bhdl_analyzer::component_inference::ParameterValue::Real(r) => r.to_string(),
                    bhdl_analyzer::component_inference::ParameterValue::Boolean(b) => b.to_string(),
                })
                .unwrap_or_else(|| "unknown".to_string());
            println!("  - {}: {} (confidence: {:.1})", 
                comp.component_type,
                value_str,
                comp.confidence);
        }
    }
    
    println!("\n🎉 Semantic analysis completed!");
    
    Ok(())
}