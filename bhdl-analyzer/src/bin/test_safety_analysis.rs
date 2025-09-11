use bhdl_parser::parse;
use bhdl_analyzer::analyze;
use bhdl_analyzer::passes::{SafetyCompliance};
use bhdl_ast::{AstNode, SourceFile};

fn main() {
    // Test BCM board with safety compliance
    let bcm_board = r#"
board BCM_PowerSupply {
    // Components for power monitoring
    voltage_monitor: VoltageMonitor();
    input_protection: TVSDiode(15V);
    mcu_supply: LM7805();
    
    // Safety compliance declarations
    satisfies {
        TSR_PWR_MCU_001: via voltage_monitor;
        TSR_PWR_MCU_002: via input_protection;
        TSR_PWR_SUPPLY_001: via mcu_supply;
    }
}
"#;

    println!("Testing Safety Analysis Pass...\n");
    
    // Parse and analyze
    let parsed = parse(bcm_board);
    
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  {:?}", error);
        }
        return;
    }
    
    println!("✓ Parsing successful");
    
    // Cast to SourceFile and run full analysis
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let result = analyze(&source_file);
    
    println!("\n=== Safety Analysis Results ===");
    
    // Check safety analysis results
    let safety = &result.safety_analysis;
    
    println!("\nRequirements found: {}", safety.requirements.len());
    for (req_id, req) in &safety.requirements {
        print!("  {}: ", req_id);
        match &req.satisfaction {
            SafetyCompliance::ViaComponent { component } => {
                println!("satisfied via {}", component);
            }
            SafetyCompliance::WithDetails { details } => {
                println!("satisfied with {} detail fields", details.len());
            }
            SafetyCompliance::NotSatisfied => {
                println!("NOT SATISFIED");
            }
        }
    }
    
    println!("\nCoverage Metrics:");
    println!("  Total requirements: {}", safety.coverage.total_requirements);
    println!("  Satisfied: {}", safety.coverage.satisfied_requirements);
    println!("  Coverage: {:.1}%", safety.coverage.coverage_percentage);
    
    if !safety.coverage.unsatisfied_requirements.is_empty() {
        println!("  Unsatisfied: {:?}", safety.coverage.unsatisfied_requirements);
    }
    
    println!("\nTraceability Matrix:");
    for (req_id, components) in &safety.traceability {
        println!("  {} -> {:?}", req_id, components);
    }
    
    // Check for safety-related diagnostics
    let safety_diags: Vec<_> = result.diagnostics
        .iter()
        .filter(|d| d.message.contains("Safety") || d.message.contains("requirement"))
        .collect();
    
    if !safety_diags.is_empty() {
        println!("\nSafety-related diagnostics:");
        for diag in safety_diags {
            println!("  - {}", diag.message);
        }
    }
    
    // Test success criteria
    if safety.coverage.coverage_percentage == 100.0 {
        println!("\n✅ SUCCESS: All safety requirements satisfied!");
    } else {
        println!("\n⚠️  WARNING: Not all safety requirements satisfied");
    }
}