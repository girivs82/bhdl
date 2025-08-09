//! Minimal test to debug fault injection issue

use anyhow::Result;
use bhdl_testbench::{TestbenchRunner, compile_testbench};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Minimal Fault Injection Test ===\n");
    
    // Simple circuit
    let circuit_bhdl = r#"
    board TestBoard {
        power VCC = 5V @ 1A;
        ground GND;
        net led_circuit: @VCC -> R1: Res(330).1 -> R1.2 -> LED1: LED(red).anode -> LED1.cathode -> @GND;
    }
    "#;
    
    // Simple testbench
    let testbench_bhdl = r#"
    testbench TB_Test for TestBoard {
        simulation {
            duration: 10ms;
            timestep: 0.1ms;
            solver: spice_adaptive;
        }
        
        verify {
            assert R1.current < 20mA message "Normal current";
        }
    }
    "#;
    
    // Parse and analyze circuit
    let parse_result = parse(circuit_bhdl);
    let source_file = SourceFile::cast(parse_result.syntax()).unwrap();
    let analysis_result = analyze(&source_file);
    
    // Synthesize netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    // Parse testbench
    let testbench_parse = parse(testbench_bhdl);
    let testbench_source = SourceFile::cast(testbench_parse.syntax()).unwrap();
    let testbench_def = testbench_source.testbenches().next().unwrap();
    let testbench = compile_testbench(&testbench_def)?;
    
    // Create runner
    let mut runner = TestbenchRunner::new_with_analysis(
        testbench, 
        netlist,
        None,
        Some(analysis_result),
    )?;
    
    // Run baseline
    println!("Running baseline...");
    let baseline_result = runner.run()?;
    println!("Baseline violations: {}", baseline_result.violations.len());
    
    // Get baseline R1 current
    let r1_baseline = runner.signal_values.iter()
        .find(|(s, _)| matches!(s, bhdl_testbench::SignalRef::Current(name) if name == "R1"))
        .map(|(_, v)| v.abs())
        .unwrap_or(0.0);
    
    println!("Baseline R1 current: {:.3}A ({:.1}mA)", r1_baseline, r1_baseline * 1000.0);
    
    // Now manually apply fault and re-run
    println!("\nApplying R1 short fault...");
    
    // Access SPICE solver and modify R1 resistance
    if let Some(spice) = &mut runner.spice_solver {
        if let Some(model) = spice.solver.get_model_mut("R1") {
            if let bhdl_spice::ComponentModel::Resistor { resistance, .. } = model {
                println!("  Changing R1 from {}Ω to 0.001Ω", resistance);
                *resistance = 0.001;
            }
        }
    }
    
    // Run with fault
    println!("\nRunning with fault...");
    let fault_result = runner.run()?;
    println!("Fault violations: {}", fault_result.violations.len());
    
    // Get fault R1 current
    let r1_fault = runner.signal_values.iter()
        .find(|(s, _)| matches!(s, bhdl_testbench::SignalRef::Current(name) if name == "R1"))
        .map(|(_, v)| v.abs())
        .unwrap_or(0.0);
    
    println!("Fault R1 current: {:.3}A ({:.1}mA)", r1_fault, r1_fault * 1000.0);
    
    // Analysis
    println!("\n=== Analysis ===");
    println!("Current ratio: {:.1}x", r1_fault / r1_baseline);
    
    if r1_fault > r1_baseline * 10.0 {
        println!("SUCCESS: Fault injection working - current increased significantly");
    } else {
        println!("PROBLEM: Fault injection not working - current didn't increase enough");
        println!("This suggests the SPICE solver is not properly updating after model change");
    }
    
    Ok(())
}