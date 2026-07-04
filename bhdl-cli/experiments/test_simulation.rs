//! Test simulation functionality with a simple LED circuit

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, HasName};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_testbench::{compile_testbench, TestbenchRunner, WaveformFormat};
use std::fs;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    
    println!("=== BHDL Testbench Simulation Demo ===\n");
    
    // Read the test file from command line or use default
    let args: Vec<String> = std::env::args().collect();
    let test_file = if args.len() > 2 && args[1] == "--testbench" {
        args[2].clone()
    } else if args.len() > 1 {
        args[1].clone()
    } else {
        "tests/circuits/testbenches/simple_led_testbench_basic.bhdl".to_string()
    };
    
    println!("Loading circuit and testbench from: {}", test_file);
    
    let content = fs::read_to_string(&test_file)?;
    
    // Step 1: Parse
    println!("\n1. Parsing BHDL file...");
    let parse_result = parse(&content);
    
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors:");
        for error in parse_result.errors() {
            eprintln!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parsing failed"));
    }
    
    let root = parse_result.syntax();
    let source_file = SourceFile::cast(root.clone())
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Find board and testbench
    let board = source_file.boards().next()
        .ok_or_else(|| anyhow::anyhow!("No board found in file"))?;
    
    let testbench_ast = source_file.testbenches().next()
        .ok_or_else(|| anyhow::anyhow!("No testbench found in file"))?;
    
    let board_name = board.name().map(|n| n.text().to_string()).unwrap_or_else(|| "unnamed".to_string());
    let testbench_name = testbench_ast.name().map(|n| n.text().to_string()).unwrap_or_else(|| "unnamed".to_string());
    
    println!("  ✓ Found board: {}", board_name);
    println!("  ✓ Found testbench: {}", testbench_name);
    
    // Step 2: Analyze board
    println!("\n2. Analyzing circuit...");
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        println!("  Analysis diagnostics:");
        for diag in &analysis.diagnostics {
            println!("    - {}", diag.message);
        }
    }
    
    // Step 3: Synthesize netlist
    println!("\n3. Synthesizing netlist...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    println!("  ✓ Netlist generated:");
    println!("    - {} instances", netlist.instances.len());
    println!("    - {} nets", netlist.nets.len());
    
    // Step 4: Compile testbench
    println!("\n4. Compiling testbench...");
    let testbench = compile_testbench(&testbench_ast)?;
    
    println!("  ✓ Testbench compiled:");
    println!("    - Simulation duration: {}ms", 
        testbench.simulation_config.duration.value);
    println!("    - {} scopes defined", testbench.scopes.len());
    println!("    - {} stimuli", testbench.stimuli.len());
    println!("    - {} assertions", testbench.assertions.len());
    
    // Step 5: Create output directory
    let output_dir = PathBuf::from("tests/outputs/simulation");
    fs::create_dir_all(&output_dir)?;
    
    // Step 6: Run simulation
    println!("\n5. Running simulation...");
    
    // Create testbench runner
    let mut runner = TestbenchRunner::new(testbench, netlist, None)?;
    
    // Add VCD output
    let vcd_path = output_dir.join("simple_led.vcd");
    runner.add_waveform_output(WaveformFormat::VCD, &vcd_path)?;
    
    // Also add CSV for easy analysis
    let csv_path = output_dir.join("simple_led.csv");
    runner.add_waveform_output(WaveformFormat::CSV, &csv_path)?;
    
    // Run the simulation
    let results = runner.run()?;
    
    // Step 7: Report results
    println!("\n6. Simulation Results:");
    
    if results.passed {
        println!("  ✓ All assertions PASSED");
    } else {
        println!("  ✗ {} assertions FAILED", results.violations.len());
        for violation in &results.violations {
            println!("    - {} @ {:.3}ms: {}", 
                violation.assertion_name,
                violation.time * 1000.0,
                violation.message
            );
        }
    }
    
    if !results.measurements.is_empty() {
        println!("\n  Measurements:");
        for (name, value) in &results.measurements {
            println!("    - {}: {:.3}", name, value);
        }
    }
    
    println!("\n  Output files:");
    println!("    - VCD waveform: {}", vcd_path.display());
    println!("    - CSV data: {}", csv_path.display());
    
    println!("\n=== Simulation Complete ===");
    
    Ok(())
}