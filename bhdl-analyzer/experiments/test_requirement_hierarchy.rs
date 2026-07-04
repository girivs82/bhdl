use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::passes::analyze_requirement_hierarchy;

fn main() {
    // Test hierarchical requirements with proper decomposition
    let bcm_hierarchical = r#"
board BCM_PowerSupply {
    // Components for power monitoring
    voltage_monitor: VoltageMonitor();
    input_protection: TVSDiode(15V);
    current_monitor: CurrentSensor(100mA);
    mcu_supply: LM7805();
    backup_monitor: VoltageMonitor();  // Redundant monitoring
    
    // Hierarchical safety compliance
    satisfies {
        // Safety Goal (highest level)
        SG_BCM_001: via FSR_BCM_LIGHT_001;
        
        // Functional Safety Requirements
        FSR_BCM_LIGHT_001: via TSR_BCM_LIGHT_MON_001, TSR_BCM_LIGHT_MON_002;
        FSR_PWR_MCU: via TSR_PWR_MCU_001, TSR_PWR_MCU_002;
        
        // Technical Safety Requirements (implementation level)
        TSR_BCM_LIGHT_MON_001: via current_monitor;
        TSR_BCM_LIGHT_MON_002: via voltage_monitor, backup_monitor;
        TSR_PWR_MCU_001: via voltage_monitor;
        TSR_PWR_MCU_002: via input_protection;
        TSR_PWR_SUPPLY_001: via mcu_supply;
    }
}
"#;

    println!("Testing Hierarchical Requirement Analysis...\n");
    
    // Parse the BHDL
    let parsed = parse(bcm_hierarchical);
    
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  {:?}", error);
        }
        return;
    }
    
    println!("✓ Parsing successful\n");
    
    // Analyze requirement hierarchy
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let hierarchy = analyze_requirement_hierarchy(&source_file);
    
    println!("=== Requirement Hierarchy Analysis ===\n");
    
    // Summary statistics
    println!("Total Requirements: {}", hierarchy.requirements.len());
    println!("Overall Coverage: {:.1}%", hierarchy.coverage.overall);
    println!();
    
    // Show requirement levels
    let mut by_level = std::collections::HashMap::new();
    for req in hierarchy.requirements.values() {
        *by_level.entry(format!("{:?}", req.level)).or_insert(0) += 1;
    }
    
    println!("Requirements by Level:");
    for (level, count) in &by_level {
        println!("  {}: {}", level, count);
    }
    println!();
    
    // Show decomposition tree
    println!("Decomposition Tree:");
    for (parent, children) in &hierarchy.decomposition_tree {
        println!("  {} decomposes to:", parent);
        for child in children {
            println!("    └─> {}", child);
        }
    }
    println!();
    
    // Show implementation mappings
    println!("Implementation Mappings:");
    for (req_id, req) in &hierarchy.requirements {
        match &req.implemented_by {
            bhdl_analyzer::passes::ImplementationDetails::ByComponents(comps) => {
                println!("  {} → {}", req_id, comps.join(", "));
            }
            bhdl_analyzer::passes::ImplementationDetails::ByRequirements(reqs) => {
                println!("  {} → [{}]", req_id, reqs.join(", "));
            }
            _ => {}
        }
    }
    println!();
    
    // Show traceability paths
    if !hierarchy.traceability_paths.is_empty() {
        println!("Complete Traceability Paths:");
        for path in &hierarchy.traceability_paths {
            println!("\n  {} (Safety Goal)", path.safety_goal);
            for fr in &path.functional_reqs {
                println!("    └─> {} (Functional)", fr);
            }
            for tr in &path.technical_reqs {
                println!("        └─> {} (Technical)", tr);
            }
            for impl_req in &path.implementations {
                if let Some(req) = hierarchy.requirements.get(impl_req) {
                    match &req.implemented_by {
                        bhdl_analyzer::passes::ImplementationDetails::ByComponents(comps) => {
                            for comp in comps {
                                println!("            └─> {} (Component)", comp);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        println!();
    }
    
    // Show any validation issues
    if !hierarchy.diagnostics.is_empty() {
        println!("\n⚠️ Validation Issues:");
        for diag in &hierarchy.diagnostics {
            println!("  - {}", diag.message);
        }
    } else {
        println!("\n✅ No validation issues found!");
    }
    
    // Generate and display traceability report
    println!("\n=== Traceability Report (Markdown) ===\n");
    let report = hierarchy.generate_traceability_report();
    
    // Just show first part of report
    let lines: Vec<&str> = report.lines().take(50).collect();
    for line in lines {
        println!("{}", line);
    }
    if report.lines().count() > 50 {
        println!("\n... (report truncated, {} total lines)", report.lines().count());
    }
}