//! Debug simulation to track signal flow

use anyhow::Result;
use std::path::Path;

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, HasName};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_testbench::{TestbenchRunner, WaveformFormat};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Starting Debug Simulation ===");
    
    // Read the testbench file
    let testbench_content = std::fs::read_to_string("tests/circuits/testbenches/simple_led_testbench_basic.bhdl")?;
    println!("Testbench content loaded: {} bytes", testbench_content.len());
    
    // Parse the testbench
    let parse_result = parse(&testbench_content);
    if !parse_result.errors().is_empty() {
        for error in parse_result.errors() {
            eprintln!("Parse error: {:?}", error);
        }
        anyhow::bail!("Failed to parse testbench due to errors");
    }
    println!("Parse successful");
    
    // Convert to AST
    let ast = SourceFile::cast(parse_result.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to get SourceFile from parse result"))?;
    
    // Find the board definition for synthesis
    let board_def = ast.boards()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No board found in file"))?;
    let board_name = board_def.name().map(|n| n.text().to_string());
    println!("Board found: {:?}", board_name);
    
    // Find the testbench definition
    let testbench_def = ast.testbenches()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No testbench found in file"))?;
    let testbench_name = testbench_def.name().map(|n| n.text().to_string());
    println!("Testbench found: {:?}", testbench_name);
    
    // Analyze the board
    let analysis_result = analyze(&ast);
    println!("Analysis complete. {} diagnostics", analysis_result.diagnostics.len());
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&ast, &analysis_result).await?;
    println!("Netlist generated: {} instances, {} nets", 
             netlist.instances.len(), netlist.nets.len());
    
    // Compile testbench to runtime structure
    let testbench = bhdl_testbench::compiler::compile_testbench(&testbench_def)?;
    println!("Testbench compiled: {} scopes, {} stimuli", 
             testbench.scopes.len(), testbench.stimuli.len());
    
    // Create and configure testbench runner
    let mut runner = TestbenchRunner::new(
        testbench,
        netlist,
        None, // No flow tracker needed for SPICE
    )?;
    println!("TestbenchRunner created");
    
    // Add CSV output
    runner.add_waveform_output(WaveformFormat::CSV, Path::new("tests/outputs/simulation/debug_simple_led.csv"))?;
    println!("Waveform output configured");
    
    // Run simulation
    println!("Starting simulation...");
    let results = runner.run()?;
    
    println!("=== Simulation Complete ===");
    println!("Passed: {}", results.passed);
    println!("Violations: {}", results.violations.len());
    println!("Measurements: {:?}", results.measurements);
    println!("Simulation time: {:.6}s", results.simulation_time);
    
    Ok(())
}