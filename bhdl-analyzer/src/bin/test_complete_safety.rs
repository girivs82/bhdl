use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::passes::{
    analyze_requirement_hierarchy,
    analyze_fmea,
    ASILLevel,
};

fn main() {
    // Comprehensive BCM safety example with hierarchical requirements
    let bcm_complete = r#"
board BCM_PowerSupply {
    // Safety-critical components with redundancy
    voltage_monitor: VoltageMonitor();      // Primary voltage monitoring
    backup_monitor: VoltageMonitor();       // Redundant voltage monitoring
    current_monitor: CurrentSensor(100mA);  // Current monitoring
    input_protection: TVSDiode(15V);        // Input protection
    mcu_supply: LM7805();                   // MCU power supply
    
    // Hierarchical safety compliance with full traceability
    satisfies {
        // Safety Goals (top level) - simplified syntax for now
        SG_BCM_001: via FSR_BCM_POWER_001;
        
        // Functional Safety Requirements (decomposed from safety goals)
        FSR_BCM_POWER_001: via TSR_PWR_MONITOR_001;
        FSR_BCM_POWER_002: via TSR_PWR_PROTECT_001;
        
        // Technical Safety Requirements (implementation level)
        TSR_PWR_MONITOR_001: via voltage_monitor;    // Primary monitoring
        TSR_PWR_MONITOR_002: via backup_monitor;     // Redundant monitoring
        TSR_PWR_PROTECT_001: via input_protection;   // Protection
        TSR_PWR_SUPPLY_001: via mcu_supply;          // MCU supply
        TSR_PWR_CURRENT_001: via current_monitor;    // Current monitoring
    }
}
"#;

    println!("=== COMPLETE ISO 26262 SAFETY ANALYSIS ===\n");
    println!("Testing hierarchical requirements with FMEA/FMEDA...\n");
    
    // Parse the BHDL
    let parsed = parse(bcm_complete);
    
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  {:?}", error);
        }
        return;
    }
    
    println!("✓ Parsing successful\n");
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    // STEP 1: Analyze requirement hierarchy
    println!("STEP 1: Requirement Hierarchy Analysis");
    println!("=====================================\n");
    
    let hierarchy = analyze_requirement_hierarchy(&source_file);
    
    println!("Requirements found: {}", hierarchy.requirements.len());
    println!("Coverage: {:.1}%\n", hierarchy.coverage.overall);
    
    // Show requirement tree
    println!("Requirement Decomposition:");
    for (parent, children) in &hierarchy.decomposition_tree {
        println!("  {} →", parent);
        for child in children {
            println!("    └─ {}", child);
        }
    }
    println!();
    
    // Show traceability paths
    if !hierarchy.traceability_paths.is_empty() {
        println!("Complete Traceability Paths:");
        for path in &hierarchy.traceability_paths {
            println!("\n  {} (Safety Goal)", path.safety_goal);
            
            // Show functional requirements
            for fr in &path.functional_reqs {
                println!("    ├─ {} (Functional)", fr);
            }
            
            // Show technical requirements and their implementations
            for tr in &path.technical_reqs {
                print!("    └─ {} (Technical)", tr);
                
                // Show what implements this technical requirement
                if let Some(req) = hierarchy.requirements.get(tr) {
                    match &req.implemented_by {
                        bhdl_analyzer::passes::ImplementationDetails::ByComponents(comps) => {
                            println!(" → [{}]", comps.join(", "));
                        }
                        _ => println!(),
                    }
                }
            }
        }
        println!();
    }
    
    // STEP 2: FMEA/FMEDA Analysis
    println!("\nSTEP 2: FMEA/FMEDA Analysis");
    println!("============================\n");
    
    let fmea = analyze_fmea(&source_file, &hierarchy);
    
    // Show safety metrics
    println!("Safety Metrics:");
    println!("  SPFM: {:.1}% (Single Point Fault Metric)", fmea.metrics.spfm * 100.0);
    println!("  LFM:  {:.1}% (Latent Fault Metric)", fmea.metrics.lfm * 100.0);
    println!("  PMHF: {:.1} FIT (Probabilistic Metric for HW Failures)", fmea.metrics.pmhf);
    println!("  DC:   {:.1}% (Diagnostic Coverage)\n", fmea.metrics.diagnostic_coverage * 100.0);
    
    // Show ASIL compliance
    println!("ASIL Compliance Check:");
    println!("┌─────────┬────────┬────────┬─────────────┬────────┐");
    println!("│ ASIL    │ SPFM   │ LFM    │ PMHF (FIT)  │ Status │");
    println!("├─────────┼────────┼────────┼─────────────┼────────┤");
    
    for level in [ASILLevel::ASIL_A, ASILLevel::ASIL_B, ASILLevel::ASIL_C, ASILLevel::ASIL_D] {
        let targets = bhdl_analyzer::passes::fmea_analysis::ASILTargets::for_level(level.clone());
        let compliant = fmea.asil_compliance.get(&level).unwrap_or(&false);
        let status = if *compliant { "✅" } else { "❌" };
        
        println!("│ {:7} │ ≥{:4.0}% │ ≥{:4.0}% │ ≤{:10.0} │   {}   │",
            format!("{:?}", level),
            targets.spfm_target * 100.0,
            targets.lfm_target * 100.0,
            targets.pmhf_target,
            status
        );
    }
    println!("└─────────┴────────┴────────┴─────────────┴────────┘\n");
    
    // Show top failure modes by RPN
    println!("Top 5 Failure Modes (by Risk Priority Number):");
    println!("┌────────────────┬─────────────────────────┬─────┬──────────┬────────┐");
    println!("│ Component      │ Failure Mode            │ RPN │ Severity │ FIT    │");
    println!("├────────────────┼─────────────────────────┼─────┼──────────┼────────┤");
    
    for entry in fmea.fmea_table.iter().take(5) {
        println!("│ {:14} │ {:23} │ {:3} │ {:8} │ {:6.1} │",
            entry.component,
            if entry.failure_mode.len() > 23 {
                format!("{}...", &entry.failure_mode[..20])
            } else {
                entry.failure_mode.clone()
            },
            entry.rpn,
            format!("{:?}", entry.severity),
            entry.failure_rate
        );
    }
    println!("└────────────────┴─────────────────────────┴─────┴──────────┴────────┘\n");
    
    // STEP 3: Combined Analysis Summary
    println!("STEP 3: Combined Safety Analysis Summary");
    println!("=========================================\n");
    
    // Check if requirements are covered by components with adequate safety
    println!("Requirement Safety Coverage:");
    
    let mut all_covered = true;
    for (req_id, req) in &hierarchy.requirements {
        if matches!(req.level, bhdl_analyzer::passes::RequirementLevel::Technical) {
            print!("  {} → ", req_id);
            
            match &req.implemented_by {
                bhdl_analyzer::passes::ImplementationDetails::ByComponents(comps) => {
                    // Check if these components have safety mechanisms
                    let mut has_safety = false;
                    for comp in comps {
                        if let Some(reliability) = fmea.components.get(comp) {
                            if !reliability.safety_mechanisms.is_empty() {
                                has_safety = true;
                            }
                        }
                    }
                    
                    if has_safety {
                        println!("✓ Implemented with safety mechanisms");
                    } else {
                        println!("⚠️ No safety mechanisms detected");
                        all_covered = false;
                    }
                }
                _ => {
                    println!("❌ Not implemented");
                    all_covered = false;
                }
            }
        }
    }
    
    println!("\n{}", "=".repeat(50));
    
    // Final verdict
    if all_covered && fmea.metrics.spfm >= 0.90 && fmea.metrics.lfm >= 0.60 {
        println!("\n✅ SAFETY ANALYSIS PASSED");
        println!("   - All requirements traced to implementation");
        println!("   - Safety metrics meet minimum ASIL-B targets");
        println!("   - Components have diagnostic coverage");
    } else {
        println!("\n⚠️ SAFETY ANALYSIS NEEDS ATTENTION");
        if !all_covered {
            println!("   - Some requirements lack safety mechanisms");
        }
        if fmea.metrics.spfm < 0.90 {
            println!("   - SPFM below target (need ≥90%)");
        }
        if fmea.metrics.lfm < 0.60 {
            println!("   - LFM below target (need ≥60%)");
        }
    }
    
    // Show any diagnostics
    if !hierarchy.diagnostics.is_empty() || !fmea.diagnostics.is_empty() {
        println!("\n⚠️ Issues Found:");
        for diag in &hierarchy.diagnostics {
            println!("  - {}", diag.message);
        }
        for diag in &fmea.diagnostics {
            println!("  - {}", diag.message);
        }
    }
    
    println!("\n{}", "=".repeat(50));
    println!("\nThis demonstrates complete ISO 26262 safety analysis:");
    println!("1. Hierarchical requirement decomposition (V-model)");
    println!("2. Full traceability from safety goals to components");
    println!("3. FMEA/FMEDA with failure mode analysis");
    println!("4. Safety metric calculation (SPFM, LFM, PMHF)");
    println!("5. ASIL compliance verification");
}