use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::passes::{
    analyze_requirement_hierarchy,
    analyze_fmea,
    ASILLevel,
};

fn main() {
    // Enhanced BCM safety example with multiple redundant components and diagnostic coverage
    let bcm_enhanced = r#"
board BCM_PowerSupply {
    // Triple redundancy for voltage monitoring (2oo3 voting)
    voltage_monitor_1: VoltageMonitor();      // Primary voltage monitoring
    voltage_monitor_2: VoltageMonitor();      // Secondary voltage monitoring  
    voltage_monitor_3: VoltageMonitor();      // Tertiary voltage monitoring
    
    // Dual redundancy for current monitoring
    current_monitor_1: CurrentSensor(100mA);  // Primary current monitoring
    current_monitor_2: CurrentSensor(100mA);  // Redundant current monitoring
    
    // Protection circuits with diagnostics
    input_protection: TVSDiode(15V);          // Primary overvoltage protection
    reverse_protection: SchottkyDiode();      // Reverse polarity protection
    
    // Power supplies with monitoring
    mcu_supply: LM7805();                     // MCU power supply (5V)
    backup_supply: LM7805();                  // Backup MCU supply
    
    // Diagnostic components for self-testing
    watchdog: WatchdogTimer(100ms);           // System health monitoring
    diagnostic_led: LED(green);               // Visual diagnostic indicator
    fault_led: LED(red);                      // Fault indication
    
    // Communication and reporting
    can_transceiver: MCP2551();               // CAN bus for safety reporting
    
    // Hierarchical safety compliance with enhanced redundancy
    satisfies {
        // Safety Goals (top level)
        SG_BCM_001: via FSR_BCM_POWER_001, FSR_BCM_POWER_002;
        SG_BCM_002: via FSR_BCM_DIAG_001;
        
        // Functional Safety Requirements 
        FSR_BCM_POWER_001: via TSR_PWR_MONITOR_001, TSR_PWR_MONITOR_002;
        FSR_BCM_POWER_002: via TSR_PWR_PROTECT_001, TSR_PWR_PROTECT_002;
        FSR_BCM_DIAG_001: via TSR_DIAG_001, TSR_DIAG_002;
        
        // Technical Safety Requirements with multiple implementations
        TSR_PWR_MONITOR_001: via voltage_monitor_1, voltage_monitor_2, voltage_monitor_3;
        TSR_PWR_MONITOR_002: via current_monitor_1, current_monitor_2;
        TSR_PWR_PROTECT_001: via input_protection, reverse_protection;
        TSR_PWR_PROTECT_002: via mcu_supply, backup_supply;
        TSR_DIAG_001: via watchdog;
        TSR_DIAG_002: via diagnostic_led, fault_led, can_transceiver;
    }
}
"#;

    println!("=== ENHANCED ISO 26262 SAFETY ANALYSIS ===\n");
    println!("Testing with redundancy and diagnostic coverage...\n");
    
    // Parse the BHDL
    let parsed = parse(bcm_enhanced);
    
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
    
    // Show requirement tree with multiple implementations
    println!("Requirement Decomposition (with redundancy):");
    for (parent, children) in &hierarchy.decomposition_tree {
        println!("  {} →", parent);
        for child in children {
            print!("    └─ {}", child);
            
            // Show implementation details for technical requirements
            if let Some(req) = hierarchy.requirements.get(child) {
                match &req.implemented_by {
                    bhdl_analyzer::passes::ImplementationDetails::ByComponents(comps) if !comps.is_empty() => {
                        println!(" → [{}]", comps.join(", "));
                    }
                    _ => println!(),
                }
            }
        }
    }
    println!();
    
    // Show complete traceability paths
    println!("Complete Traceability Paths:");
    for path in &hierarchy.traceability_paths {
        println!("\n  {} (Safety Goal)", path.safety_goal);
        
        for fr in &path.functional_reqs {
            println!("    ├─ {} (Functional)", fr);
        }
        
        for tr in &path.technical_reqs {
            print!("    └─ {} (Technical)", tr);
            
            if let Some(req) = hierarchy.requirements.get(tr) {
                match &req.implemented_by {
                    bhdl_analyzer::passes::ImplementationDetails::ByComponents(comps) => {
                        let redundancy = if comps.len() > 1 {
                            format!(" ({}oo{} redundancy)", comps.len() - 1, comps.len())
                        } else {
                            String::new()
                        };
                        println!(" → [{}]{}", comps.join(", "), redundancy);
                    }
                    _ => println!(),
                }
            }
        }
    }
    println!();
    
    // STEP 2: Enhanced FMEA/FMEDA Analysis with diagnostic coverage
    println!("\nSTEP 2: Enhanced FMEA/FMEDA Analysis");
    println!("====================================\n");
    
    // Create enhanced FMEA with better diagnostic coverage
    let mut enhanced_fmea = analyze_fmea(&source_file, &hierarchy);
    
    // Add diagnostic coverage to monitoring components
    if let Some(vm1) = enhanced_fmea.components.get_mut("voltage_monitor_1") {
        vm1.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_VM1_VOTING".to_string(),
            description: "2oo3 voting logic".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::HardwareRedundancy,
            coverage: 0.99,
            targets: vec!["voltage_drift".to_string(), "stuck_at".to_string()],
        });
    }
    if let Some(vm2) = enhanced_fmea.components.get_mut("voltage_monitor_2") {
        vm2.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_VM2_VOTING".to_string(),
            description: "2oo3 voting logic".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::HardwareRedundancy,
            coverage: 0.99,
            targets: vec!["voltage_drift".to_string(), "stuck_at".to_string()],
        });
    }
    if let Some(vm3) = enhanced_fmea.components.get_mut("voltage_monitor_3") {
        vm3.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_VM3_VOTING".to_string(),
            description: "2oo3 voting logic".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::HardwareRedundancy,
            coverage: 0.99,
            targets: vec!["voltage_drift".to_string(), "stuck_at".to_string()],
        });
    }
    
    // Add diagnostic coverage to current monitors
    if let Some(cm1) = enhanced_fmea.components.get_mut("current_monitor_1") {
        cm1.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_CM1_REDUNDANCY".to_string(),
            description: "Dual redundancy comparison".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::HardwareRedundancy,
            coverage: 0.95,
            targets: vec!["current_drift".to_string(), "open_circuit".to_string()],
        });
    }
    if let Some(cm2) = enhanced_fmea.components.get_mut("current_monitor_2") {
        cm2.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_CM2_REDUNDANCY".to_string(),
            description: "Dual redundancy comparison".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::HardwareRedundancy,
            coverage: 0.95,
            targets: vec!["current_drift".to_string(), "open_circuit".to_string()],
        });
    }
    
    // Add diagnostic coverage to power supplies
    if let Some(ps) = enhanced_fmea.components.get_mut("mcu_supply") {
        ps.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_PS_MONITOR".to_string(),
            description: "Output voltage monitoring".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::Monitoring,
            coverage: 0.90,
            targets: vec!["output_low".to_string(), "output_high".to_string()],
        });
        ps.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_PS_THERMAL".to_string(),
            description: "Thermal shutdown".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::TemperatureMonitoring,
            coverage: 0.85,
            targets: vec!["thermal_runaway".to_string()],
        });
    }
    if let Some(bs) = enhanced_fmea.components.get_mut("backup_supply") {
        bs.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_BS_MONITOR".to_string(),
            description: "Output voltage monitoring".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::Monitoring,
            coverage: 0.90,
            targets: vec!["output_low".to_string(), "output_high".to_string()],
        });
        bs.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_BS_SWITCH".to_string(),
            description: "Automatic switchover".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::HardwareRedundancy,
            coverage: 0.95,
            targets: vec!["primary_failure".to_string()],
        });
    }
    
    // Add watchdog as safety mechanism
    if let Some(wd) = enhanced_fmea.components.get_mut("watchdog") {
        wd.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_WD_RESET".to_string(),
            description: "Periodic system reset".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::Monitoring,
            coverage: 0.98,
            targets: vec!["system_hang".to_string(), "software_fault".to_string()],
        });
        wd.safety_mechanisms.push(bhdl_analyzer::passes::fmea_analysis::SafetyMechanism {
            id: "SM_WD_REPORT".to_string(),
            description: "Fault detection and reporting".to_string(),
            mechanism_type: bhdl_analyzer::passes::fmea_analysis::SafetyMechanismType::Diagnostic,
            coverage: 0.95,
            targets: vec!["undetected_fault".to_string()],
        });
    }
    
    // Recalculate metrics with enhanced diagnostic coverage
    enhanced_fmea.metrics.spfm = enhanced_fmea.calculate_spfm();
    enhanced_fmea.metrics.lfm = enhanced_fmea.calculate_lfm();
    enhanced_fmea.metrics.pmhf = enhanced_fmea.calculate_pmhf();
    
    // Calculate diagnostic coverage from safety mechanisms
    let mut total_coverage = 0.0;
    let mut coverage_count = 0;
    for component in enhanced_fmea.components.values() {
        for mechanism in &component.safety_mechanisms {
            total_coverage += mechanism.coverage;
            coverage_count += 1;
        }
    }
    enhanced_fmea.metrics.diagnostic_coverage = if coverage_count > 0 {
        total_coverage / coverage_count as f64
    } else {
        0.0
    };
    
    // Check ASIL compliance for all levels
    for level in [ASILLevel::ASIL_A, ASILLevel::ASIL_B, ASILLevel::ASIL_C, ASILLevel::ASIL_D] {
        enhanced_fmea.check_asil_compliance(level);
    }
    
    // Show enhanced safety metrics
    println!("Enhanced Safety Metrics:");
    println!("  SPFM: {:.1}% (Single Point Fault Metric)", enhanced_fmea.metrics.spfm * 100.0);
    println!("  LFM:  {:.1}% (Latent Fault Metric)", enhanced_fmea.metrics.lfm * 100.0);
    println!("  PMHF: {:.1} FIT (Probabilistic Metric for HW Failures)", enhanced_fmea.metrics.pmhf);
    println!("  DC:   {:.1}% (Diagnostic Coverage)\n", enhanced_fmea.metrics.diagnostic_coverage * 100.0);
    
    // Show ASIL compliance with enhanced coverage
    println!("ASIL Compliance Check (Enhanced):");
    println!("┌─────────┬────────┬────────┬─────────────┬────────┐");
    println!("│ ASIL    │ SPFM   │ LFM    │ PMHF (FIT)  │ Status │");
    println!("├─────────┼────────┼────────┼─────────────┼────────┤");
    
    for level in [ASILLevel::ASIL_A, ASILLevel::ASIL_B, ASILLevel::ASIL_C, ASILLevel::ASIL_D] {
        let targets = bhdl_analyzer::passes::fmea_analysis::ASILTargets::for_level(level.clone());
        let compliant = enhanced_fmea.asil_compliance.get(&level).unwrap_or(&false);
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
    
    // Show redundancy analysis
    println!("Redundancy Analysis:");
    println!("┌─────────────────────┬────────────┬──────────────────────────┐");
    println!("│ Function            │ Redundancy │ Components               │");
    println!("├─────────────────────┼────────────┼──────────────────────────┤");
    
    // Analyze redundancy from requirements
    for (req_id, req) in &hierarchy.requirements {
        if req_id.starts_with("TSR_") {
            if let bhdl_analyzer::passes::ImplementationDetails::ByComponents(comps) = &req.implemented_by {
                if comps.len() > 1 {
                    let redundancy = format!("{}oo{}", comps.len() - 1, comps.len());
                    let comp_list = if comps.join(", ").len() > 24 {
                        format!("{}...", &comps.join(", ")[..21])
                    } else {
                        comps.join(", ")
                    };
                    println!("│ {:19} │ {:10} │ {:24} │", 
                        &req_id[4..], // Remove TSR_ prefix
                        redundancy,
                        comp_list
                    );
                }
            }
        }
    }
    println!("└─────────────────────┴────────────┴──────────────────────────┘\n");
    
    // STEP 3: Final Safety Assessment
    println!("STEP 3: Final Safety Assessment");
    println!("================================\n");
    
    let all_requirements_covered = hierarchy.requirements.values()
        .filter(|r| matches!(r.level, bhdl_analyzer::passes::RequirementLevel::Technical))
        .all(|r| matches!(r.implemented_by, bhdl_analyzer::passes::ImplementationDetails::ByComponents(ref c) if !c.is_empty()));
    
    let has_sufficient_redundancy = hierarchy.requirements.values()
        .filter(|r| r.id.starts_with("TSR_PWR_MONITOR"))
        .any(|r| {
            if let bhdl_analyzer::passes::ImplementationDetails::ByComponents(ref comps) = r.implemented_by {
                comps.len() >= 2
            } else {
                false
            }
        });
    
    let meets_asil_b = enhanced_fmea.asil_compliance.get(&ASILLevel::ASIL_B).unwrap_or(&false);
    let meets_asil_c = enhanced_fmea.asil_compliance.get(&ASILLevel::ASIL_C).unwrap_or(&false);
    
    println!("{}", "═".repeat(60));
    
    if all_requirements_covered && has_sufficient_redundancy && *meets_asil_b {
        println!("\n✅ ENHANCED SAFETY ANALYSIS PASSED");
        println!("   ✓ All requirements traced to implementations");
        println!("   ✓ Critical functions have redundancy");
        println!("   ✓ Safety metrics meet ASIL-B targets");
        if *meets_asil_c {
            println!("   ✓ System also meets ASIL-C requirements!");
        }
        println!("   ✓ Diagnostic coverage exceeds {:.0}%", enhanced_fmea.metrics.diagnostic_coverage * 100.0);
    } else {
        println!("\n⚠️ SAFETY ANALYSIS NEEDS IMPROVEMENT");
        if !all_requirements_covered {
            println!("   ✗ Some requirements lack implementation");
        }
        if !has_sufficient_redundancy {
            println!("   ✗ Critical monitoring lacks redundancy");
        }
        if !meets_asil_b {
            println!("   ✗ Safety metrics below ASIL-B targets");
        }
    }
    
    println!("\n{}", "═".repeat(60));
    println!("\nThis demonstrates enhanced ISO 26262 safety analysis with:");
    println!("• Multiple component implementations via comma-separated lists");
    println!("• Triple redundancy (2oo3) for critical voltage monitoring");
    println!("• Dual redundancy for current monitoring and power supplies");
    println!("• Diagnostic coverage through watchdog and self-test");
    println!("• Improved SPFM/LFM through redundancy and diagnostics");
    println!("• Complete traceability with redundancy annotations");
}