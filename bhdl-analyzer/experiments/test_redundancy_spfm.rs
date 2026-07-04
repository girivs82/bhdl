use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::passes::{
    analyze_requirement_hierarchy,
    analyze_fmea,
    ASILLevel,
    RedundancyAnalyzer,
};

fn main() {
    // BCM with proper redundancy for ASIL-B compliance
    let bcm_redundant = r#"
board BCM_PowerSupply {
    // Triple redundancy for voltage monitoring (2oo3 voting)
    voltage_monitor_1: VoltageMonitor();
    voltage_monitor_2: VoltageMonitor();  
    voltage_monitor_3: VoltageMonitor();
    
    // Dual redundancy for current monitoring
    current_monitor_1: CurrentSensor(100mA);
    current_monitor_2: CurrentSensor(100mA);
    
    // Dual protection circuits
    input_protection: TVSDiode(15V);
    reverse_protection: SchottkyDiode();
    
    // Dual power supplies with monitoring
    mcu_supply: LM7805();
    backup_supply: LM7805();
    
    // Diagnostic components
    watchdog: WatchdogTimer(100ms);
    diagnostic_led: LED(green);
    
    // Safety compliance with redundancy
    satisfies {
        // Technical requirements with multiple implementations
        TSR_PWR_MONITOR_001: via voltage_monitor_1, voltage_monitor_2, voltage_monitor_3;
        TSR_PWR_MONITOR_002: via current_monitor_1, current_monitor_2;
        TSR_PWR_PROTECT_001: via input_protection, reverse_protection;
        TSR_PWR_SUPPLY_001: via mcu_supply, backup_supply;
        TSR_DIAG_001: via watchdog, diagnostic_led;
    }
}
"#;

    println!("=== REDUNDANCY-AWARE SPFM ANALYSIS ===\n");
    
    // Parse the BHDL
    let parsed = parse(bcm_redundant);
    
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  {:?}", error);
        }
        return;
    }
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    // STEP 1: Analyze requirement hierarchy
    println!("STEP 1: Requirement Analysis");
    println!("=============================\n");
    
    let hierarchy = analyze_requirement_hierarchy(&source_file);
    
    println!("Requirements with implementations:");
    for (req_id, req) in &hierarchy.requirements {
        if let bhdl_analyzer::passes::ImplementationDetails::ByComponents(comps) = &req.implemented_by {
            if !comps.is_empty() {
                let redundancy = if comps.len() > 1 {
                    format!(" [{}oo{} redundancy]", comps.len() - 1, comps.len())
                } else {
                    String::new()
                };
                println!("  {} → {}{}", req_id, comps.join(", "), redundancy);
            }
        }
    }
    println!();
    
    // STEP 2: Baseline FMEA (without redundancy consideration)
    println!("STEP 2: Baseline FMEA Analysis");
    println!("================================\n");
    
    let mut baseline_fmea = analyze_fmea(&source_file, &hierarchy);
    
    // Add some diagnostic coverage
    for (_, component) in &mut baseline_fmea.components {
        if component.component_type.contains("Monitor") {
            component.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
                id: format!("SM_{}_MON", component.component_id.to_uppercase()),
                description: "Built-in self-test".to_string(),
                mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::Diagnostic,
                coverage: 0.90,
                targets: vec!["stuck_at".to_string()],
            });
        }
    }
    
    let baseline_spfm = baseline_fmea.calculate_spfm();
    let baseline_lfm = baseline_fmea.calculate_lfm();
    let baseline_pmhf = baseline_fmea.calculate_pmhf();
    
    println!("Baseline Metrics (without redundancy analysis):");
    println!("  SPFM: {:.1}%", baseline_spfm * 100.0);
    println!("  LFM:  {:.1}%", baseline_lfm * 100.0);
    println!("  PMHF: {:.1} FIT\n", baseline_pmhf);
    
    // STEP 3: Apply redundancy analysis
    println!("STEP 3: Redundancy Analysis");
    println!("============================\n");
    
    let mut redundancy_analyzer = RedundancyAnalyzer::new();
    redundancy_analyzer.analyze_from_hierarchy(&hierarchy);
    
    println!("Detected Redundancy Configurations:");
    for (req_id, config) in &redundancy_analyzer.redundancy_configs {
        println!("  {} → {}", req_id, config.redundancy_type.description());
        println!("    Components: {}", config.components.join(", "));
        
        // Calculate effective failure rate for demonstration
        let base_rate = 100.0;  // Example 100 FIT base rate
        let effective = config.redundancy_type.calculate_effective_failure_rate(base_rate, config.common_cause_factor);
        println!("    Failure rate reduction: {:.1} FIT → {:.1} FIT ({:.1}% improvement)",
            base_rate, effective, (1.0 - effective/base_rate) * 100.0);
    }
    println!();
    
    // STEP 4: Apply redundancy to FMEA
    println!("STEP 4: Redundancy-Adjusted FMEA");
    println!("==================================\n");
    
    let mut adjusted_fmea = baseline_fmea.clone();
    redundancy_analyzer.apply_to_fmea(&mut adjusted_fmea);
    
    // Calculate adjusted metrics
    let adjusted_spfm = redundancy_analyzer.calculate_adjusted_spfm(&adjusted_fmea);
    let adjusted_lfm = adjusted_fmea.calculate_lfm();
    let adjusted_pmhf = adjusted_fmea.calculate_pmhf();
    
    println!("Redundancy-Adjusted Metrics:");
    println!("  SPFM: {:.1}% (was {:.1}%)", adjusted_spfm * 100.0, baseline_spfm * 100.0);
    println!("  LFM:  {:.1}% (was {:.1}%)", adjusted_lfm * 100.0, baseline_lfm * 100.0);
    println!("  PMHF: {:.1} FIT (was {:.1} FIT)\n", adjusted_pmhf, baseline_pmhf);
    
    // STEP 5: Check ASIL compliance
    println!("STEP 5: ASIL Compliance Check");
    println!("==============================\n");
    
    println!("┌─────────┬────────┬────────┬─────────────┬──────────┬──────────┐");
    println!("│ ASIL    │ SPFM   │ LFM    │ PMHF (FIT)  │ Baseline │ Adjusted │");
    println!("├─────────┼────────┼────────┼─────────────┼──────────┼──────────┤");
    
    for level in [ASILLevel::ASIL_A, ASILLevel::ASIL_B, ASILLevel::ASIL_C, ASILLevel::ASIL_D] {
        let targets = bhdl_analyzer::passes::fmea_analysis::ASILTargets::for_level(level.clone());
        
        let baseline_meets = baseline_spfm >= targets.spfm_target && 
                           baseline_lfm >= targets.lfm_target && 
                           baseline_pmhf <= targets.pmhf_target;
        
        let adjusted_meets = adjusted_spfm >= targets.spfm_target && 
                           adjusted_lfm >= targets.lfm_target && 
                           adjusted_pmhf <= targets.pmhf_target;
        
        let baseline_status = if baseline_meets { "✅" } else { "❌" };
        let adjusted_status = if adjusted_meets { "✅" } else { "❌" };
        
        println!("│ {:7} │ ≥{:4.0}% │ ≥{:4.0}% │ ≤{:10.0} │    {}    │    {}    │",
            format!("{:?}", level),
            targets.spfm_target * 100.0,
            targets.lfm_target * 100.0,
            targets.pmhf_target,
            baseline_status,
            adjusted_status
        );
    }
    println!("└─────────┴────────┴────────┴─────────────┴──────────┴──────────┘\n");
    
    // STEP 6: Generate redundancy report
    let report = redundancy_analyzer.generate_report();
    
    println!("STEP 6: Redundancy Summary");
    println!("===========================\n");
    
    println!("Total functions: {}", report.total_functions);
    println!("Redundant functions: {} ({:.0}%)", 
        report.redundant_functions, 
        (report.redundant_functions as f64 / report.total_functions as f64) * 100.0);
    println!("Single-channel functions: {}", report.single_channel_functions);
    println!("Highest redundancy: {}", report.highest_redundancy.description());
    
    // Count SPF to MPF conversions
    let mut spf_count = 0;
    let mut mpf_count = 0;
    for component in adjusted_fmea.components.values() {
        for mode in &component.failure_modes {
            match mode.failure_type {
                bhdl_analyzer::passes::FailureType::SinglePointFault => spf_count += 1,
                bhdl_analyzer::passes::FailureType::MultiPointFault => mpf_count += 1,
                _ => {}
            }
        }
    }
    
    println!("\nFailure Mode Classification:");
    println!("  Single-Point Faults: {} (should be minimal with redundancy)", spf_count);
    println!("  Multi-Point Faults: {} (converted from SPF due to redundancy)", mpf_count);
    
    println!("\n{}", "=".repeat(60));
    
    // Final assessment
    let meets_asil_b = adjusted_spfm >= 0.90 && adjusted_lfm >= 0.60 && adjusted_pmhf <= 100.0;
    
    if meets_asil_b {
        println!("\n✅ SYSTEM ACHIEVES ASIL-B COMPLIANCE");
        println!("   • SPFM ≥ 90% achieved through redundancy");
        println!("   • LFM ≥ 60% maintained");
        println!("   • PMHF ≤ 100 FIT achieved");
        println!("   • Redundancy effectively mitigates single-point faults");
    } else {
        println!("\n⚠️ SYSTEM NEEDS FURTHER IMPROVEMENTS FOR ASIL-B");
        if adjusted_spfm < 0.90 {
            println!("   • SPFM still below 90% - add more redundancy or diagnostics");
        }
        if adjusted_lfm < 0.60 {
            println!("   • LFM below 60% - improve latent fault detection");
        }
        if adjusted_pmhf > 100.0 {
            println!("   • PMHF above 100 FIT - reduce failure rates or add mitigation");
        }
    }
    
    println!("\n{}", "=".repeat(60));
    println!("\nThis demonstrates how redundancy analysis improves SPFM:");
    println!("• Single-point faults become multi-point faults with redundancy");
    println!("• Effective failure rates are calculated considering redundancy type");
    println!("• 2oo3 voting provides better coverage than 1oo2 redundancy");
    println!("• Common cause failures (CCF) are considered in calculations");
}