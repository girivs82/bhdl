//! Test fault injection capabilities with LED circuit
//! Demonstrates how R1 short causes overcurrent and safety analysis detects LED damage

use anyhow::Result;
use bhdl_testbench::{
    TestbenchRunner, compile_testbench,
    fault_injection::{FaultScenario, FaultType, FaultValue, ExpectedBehavior, StressLimit},
};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Fault Injection Test: LED Circuit ===\n");
    
    // Define test circuit with LED and current limiting resistor
    let circuit_bhdl = r#"
    board TestBoard {
        // Power source
        power VCC = 5V @ 1A;
        ground GND;
        
        // LED circuit with current limiting resistor
        net led_circuit: @VCC -> R1: Res(330).1 -> R1.2 -> LED1: LED(red).anode -> LED1.cathode -> @GND;
    }
    "#;
    
    // Define testbench with fault scenarios
    let testbench_bhdl = r#"
    testbench TB_LED_Fault for TestBoard {
        simulation {
            duration: 100ms;
            timestep: 0.1ms;
            solver: spice_adaptive;
        }
        
        verify {
            // Normal operation checks
            assert @VCC == 5V message "VCC voltage stable";
            assert R1.current < 20mA message "Current within safe limits";
            assert LED1.current < 30mA message "LED current safe";
        }
        
        measure {
            nominal_current = R1.current;
            led_voltage = LED1.voltage;
        }
    }
    "#;
    
    // Parse the circuit
    println!("Parsing circuit...");
    let parse_result = parse(circuit_bhdl);
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parse_result.errors() {
            eprintln!("  {:?}", error);
        }
        return Err(anyhow::anyhow!("Failed to parse circuit"));
    }
    
    let source_file = SourceFile::cast(parse_result.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to get AST root"))?;
    
    // Analyze the circuit
    println!("Analyzing circuit...");
    let analysis_result = analyze(&source_file);
    
    // Check for analysis errors
    if !analysis_result.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  {}", diag.message);
        }
    }
    
    // Synthesize netlist
    println!("Synthesizing netlist...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    // Parse testbench
    println!("Parsing testbench...");
    let testbench_parse_result = parse(testbench_bhdl);
    if !testbench_parse_result.errors().is_empty() {
        eprintln!("Testbench parse errors:");
        for error in testbench_parse_result.errors() {
            eprintln!("  {:?}", error);
        }
        return Err(anyhow::anyhow!("Failed to parse testbench"));
    }
    
    // Find the testbench definition in the AST
    let testbench_source = SourceFile::cast(testbench_parse_result.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to get testbench AST root"))?;
    
    let testbench_def = testbench_source.testbenches().next()
        .ok_or_else(|| anyhow::anyhow!("No testbench definition found"))?;
    
    // Compile testbench
    println!("Compiling testbench...");
    let testbench = compile_testbench(&testbench_def)?;
    
    // Create testbench runner
    println!("Creating testbench runner...");
    let mut runner = TestbenchRunner::new_with_analysis(
        testbench, 
        netlist,
        None,  // No flow tracker needed for this test
        Some(analysis_result),  // Pass analysis result for power domains
    )?;
    
    // Add custom fault scenarios beyond the standard ones
    println!("\nAdding custom fault scenarios...");
    
    // Scenario 1: R1 partial short (more realistic)
    let r1_short_scenario = FaultScenario {
        name: "r1_partial_short".to_string(),
        description: "R1 fails as partial short circuit (10 ohms)".to_string(),
        faults: {
            let mut faults = HashMap::new();
            faults.insert("R1".to_string(), FaultType::ParameterOverride {
                parameter: "resistance".to_string(),
                value: FaultValue::Absolute(10.0), // 10 ohms - more realistic
            });
            faults
        },
        trigger_time: Some(0.01), // Trigger at 10ms
        expected_behavior: Some(ExpectedBehavior {
            should_fail_safe: false, // No protection in this simple circuit
            max_stress: {
                let mut stress = HashMap::new();
                stress.insert("LED1".to_string(), StressLimit {
                    max_current: Some(0.030), // 30mA absolute max for LED
                    max_voltage: None,
                    max_power: Some(0.1),     // 100mW max
                    max_temperature: None,
                });
                stress
            },
            protection_should_trigger: vec![],
        }),
    };
    runner.add_fault_scenario(r1_short_scenario);
    
    // Scenario 2: R1 drift +5% (within tolerance)
    let r1_drift_scenario = FaultScenario {
        name: "r1_drift_5pct".to_string(),
        description: "R1 drifts +5% (within typical tolerance)".to_string(),
        faults: {
            let mut faults = HashMap::new();
            faults.insert("R1".to_string(), FaultType::ParameterOverride {
                parameter: "resistance".to_string(),
                value: FaultValue::Drift(5.0),
            });
            faults
        },
        trigger_time: None,
        expected_behavior: None,
    };
    runner.add_fault_scenario(r1_drift_scenario);
    
    // Scenario 3: R1 drift +20% (out of tolerance)
    let r1_drift_high_scenario = FaultScenario {
        name: "r1_drift_20pct".to_string(),
        description: "R1 drifts +20% (exceeds typical 5% tolerance)".to_string(),
        faults: {
            let mut faults = HashMap::new();
            faults.insert("R1".to_string(), FaultType::ParameterOverride {
                parameter: "resistance".to_string(),
                value: FaultValue::Drift(20.0),
            });
            faults
        },
        trigger_time: None,
        expected_behavior: None,
    };
    runner.add_fault_scenario(r1_drift_high_scenario);
    
    // Run all fault scenarios
    println!("\n=== Running Fault Campaign ===\n");
    let fault_results = runner.run_fault_campaign(vec![
        "resistor_short",      // Standard scenario from FaultInjector
        "r1_partial_short",    // Our custom partial short
        "r1_drift_5pct",       // Within tolerance
        "r1_drift_20pct",      // Out of tolerance
        "led_open",           // Standard LED failure
        "aging_simulation",   // Standard aging scenario
    ])?;
    
    // Summary report
    println!("\n=== Fault Campaign Summary ===");
    println!("Total scenarios run: {}", fault_results.len());
    
    let passed = fault_results.iter().filter(|r| r.safety_passed).count();
    let failed = fault_results.len() - passed;
    
    println!("Safety passed: {}", passed);
    println!("Safety failed: {}", failed);
    
    // Detailed analysis of R1 short scenario
    if let Some(r1_short_result) = fault_results.iter()
        .find(|r| r.scenario_name == "r1_partial_short") {
        
        println!("\n=== Detailed Analysis: R1 Partial Short (10Ω) ===");
        
        // Debug: Show all signals
        println!("\nAvailable signals in results:");
        for (signal, value) in &r1_short_result.baseline_values {
            println!("  Baseline - {:?}: {:.6}", signal, value);
        }
        for (signal, value) in &r1_short_result.faulted_values {
            println!("  Faulted - {:?}: {:.6}", signal, value);
        }
        
        // Compare baseline vs faulted currents for R1
        let baseline_r1_current = r1_short_result.baseline_values.get(&bhdl_testbench::SignalRef::Current("R1".to_string()))
            .map(|v| v.abs() * 1000.0) // Convert to mA
            .unwrap_or(0.0);
        
        let faulted_r1_current = r1_short_result.faulted_values.get(&bhdl_testbench::SignalRef::Current("R1".to_string()))
            .map(|v| v.abs() * 1000.0) // Convert to mA
            .unwrap_or(0.0);
            
        // Also check LED current
        let baseline_led_current = r1_short_result.baseline_values.get(&bhdl_testbench::SignalRef::Current("LED1".to_string()))
            .map(|v| v.abs() * 1000.0) // Convert to mA
            .unwrap_or(0.0);
        
        let faulted_led_current = r1_short_result.faulted_values.get(&bhdl_testbench::SignalRef::Current("LED1".to_string()))
            .map(|v| v.abs() * 1000.0) // Convert to mA
            .unwrap_or(0.0);
        
        println!("\nCurrent Analysis:");
        println!("Baseline R1 current: {:.2} mA", baseline_r1_current);
        println!("Faulted R1 current: {:.2} mA", faulted_r1_current);
        println!("R1 current increase: {:.1}x", if baseline_r1_current > 0.0 { faulted_r1_current / baseline_r1_current } else { 0.0 });
        
        println!("\nBaseline LED current: {:.2} mA", baseline_led_current);
        println!("Faulted LED current: {:.2} mA", faulted_led_current);
        println!("LED current increase: {:.1}x", if baseline_led_current > 0.0 { faulted_led_current / baseline_led_current } else { 0.0 });
        
        // Show stress violations
        if !r1_short_result.stress_violations.is_empty() {
            println!("\nStress Violations Detected:");
            for violation in &r1_short_result.stress_violations {
                println!("  [{}] {} {}: {:.3} (limit: {:.3})",
                    violation.severity,
                    violation.component,
                    violation.stress_type,
                    violation.actual_value,
                    violation.limit_value
                );
            }
            
            println!("\nSAFETY CRITICAL: LED will be damaged due to overcurrent!");
            println!("Recommended fixes:");
            println!("  1. Add redundant current limiting");
            println!("  2. Use constant current LED driver");
            println!("  3. Add overcurrent protection (fuse/PTC)");
            println!("  4. Implement fault detection circuit");
        }
    }
    
    // Show how cascade failures would be detected (if implemented)
    println!("\n=== Cascade Failure Analysis (Future) ===");
    println!("When fully integrated with bhdl-safety:");
    println!("  1. R1 shorts -> Overcurrent through LED");
    println!("  2. LED junction temperature rises rapidly");
    println!("  3. LED fails open or degrades (Vf drops)");
    println!("  4. If LED shorts, full VCC appears across remnant");
    println!("  5. Potential PCB trace damage from overcurrent");
    
    Ok(())
}