// Test Design Rule Checker (DRC) functionality
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{Synthesizer, NetlistConfig};
use bhdl_synthesizer::design_rule_checker::{DesignRuleChecker, IndustryStandard};
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("=== Design Rule Checker (DRC) Test ===");
    
    // Load and process the test circuit
    let test_file = "test_compatibility_complex.bhdl";
    println!("Loading circuit: {}", test_file);
    
    let bhdl_source = fs::read_to_string(test_file)
        .map_err(|e| format!("Failed to read {}: {}", test_file, e))?;
    
    println!("Parsing BHDL source...");
    let parse_result = parse(&bhdl_source);
    let syntax = parse_result.syntax();
    
    println!("Running semantic analysis...");
    let analysis = analyze(&SourceFile::cast(syntax.clone()).unwrap());
    
    // Generate netlist
    println!("Generating netlist...");
    let mut config = NetlistConfig::default();
    config.database_path = None;  // No database needed for DRC test
    
    let mut synthesizer = Synthesizer::with_config(config);
    let netlist = synthesizer.generate_from_ast_and_analysis(
        &SourceFile::cast(syntax.clone()).unwrap(), 
        &analysis
    ).await?;
    
    println!("\nNetlist generated: {} components, {} nets", 
             netlist.instances.len(), netlist.nets.len());
    
    // Test different industry standards
    test_drc_standard(&netlist, &analysis, IndustryStandard::IPC2221, "IPC-2221 (Generic PCB)")?;
    test_drc_standard(&netlist, &analysis, IndustryStandard::IPC2152, "IPC-2152 (Trace Current)")?;
    test_drc_standard(&netlist, &analysis, IndustryStandard::Automotive, "Automotive")?;
    test_drc_standard(&netlist, &analysis, IndustryStandard::Medical, "Medical Device")?;
    
    println!("\n✅ Design Rule Checking test completed successfully!");
    
    Ok(())
}

fn test_drc_standard(
    netlist: &bhdl_netlist::Netlist,
    analysis: &bhdl_analyzer::AnalysisResult,
    standard: IndustryStandard,
    standard_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n{}", "=".repeat(60));
    println!("Testing {} Standard", standard_name);
    println!("{}", "=".repeat(60));
    
    let mut checker = DesignRuleChecker::new(standard);
    let report = checker.run_checks(netlist, analysis);
    
    println!("\n📋 DRC Report Summary:");
    println!("  Rules checked: {}", report.rules_checked);
    println!("  Pass rate: {:.1}%", report.pass_rate);
    println!("\n  Violations by severity:");
    println!("    🔴 Critical: {}", report.critical_count);
    println!("    🟠 Errors: {}", report.error_count);
    println!("    🟡 Warnings: {}", report.warning_count);
    println!("    🔵 Info: {}", report.info_count);
    
    if report.manufacturing_ready {
        println!("\n  ✅ Design is MANUFACTURING READY");
    } else {
        println!("\n  ❌ Design is NOT manufacturing ready");
        println!("     Fix all critical and error violations before manufacturing");
    }
    
    // Show first few violations if any
    if !report.violations.is_empty() {
        println!("\n  Sample violations (showing first 3):");
        for (i, violation) in report.violations.iter().take(3).enumerate() {
            println!("\n  {}. [{}] {} - {}", 
                     i + 1, 
                     violation.rule_id,
                     violation.rule_name,
                     match violation.severity {
                         bhdl_synthesizer::design_rule_checker::ViolationSeverity::Critical => "CRITICAL",
                         bhdl_synthesizer::design_rule_checker::ViolationSeverity::Error => "ERROR",
                         bhdl_synthesizer::design_rule_checker::ViolationSeverity::Warning => "WARNING",
                         bhdl_synthesizer::design_rule_checker::ViolationSeverity::Info => "INFO",
                     });
            println!("     Description: {}", violation.description);
            println!("     Fix: {}", violation.fix_suggestion);
            if let Some(ref standard_ref) = violation.standard_reference {
                println!("     Reference: {}", standard_ref);
            }
        }
        
        if report.violations.len() > 3 {
            println!("\n  ... and {} more violations", report.violations.len() - 3);
        }
    } else {
        println!("\n  🎉 No violations found!");
    }
    
    Ok(())
}