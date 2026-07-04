//! Test the coordinator's SPICE solver setup specifically

use anyhow::Result;
use std::collections::HashMap;

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, HasName};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_testbench::coordinator::TestbenchRunner;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Testing Coordinator SPICE Setup ===");
    
    // Read and parse the simple LED testbench
    let testbench_content = std::fs::read_to_string("tests/circuits/testbenches/simple_led_testbench_basic.bhdl")?;
    let parse_result = parse(&testbench_content);
    let ast = SourceFile::cast(parse_result.syntax()).unwrap();
    let board_def = ast.boards().next().unwrap();
    let testbench_def = ast.testbenches().next().unwrap();
    
    // Analyze
    let analysis_result = analyze(&ast);
    println!("Analysis complete");
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&ast, &analysis_result).await?;
    println!("Netlist: {} instances, {} nets", netlist.instances.len(), netlist.nets.len());
    
    // Compile testbench
    let testbench = bhdl_testbench::compiler::compile_testbench(&testbench_def)?;
    
    // Create testbench runner - this will test our SPICE solver setup
    println!("Creating TestbenchRunner (testing SPICE solver setup)...");
    let mut runner = TestbenchRunner::new(testbench, netlist, None)?;
    
    println!("✓ TestbenchRunner created successfully");
    println!("✓ SPICE solver setup appears to be working");
    println!("✓ Component models should be loaded");
    
    // Try to run just a few simulation steps
    println!("Testing a short simulation run...");
    
    let results = runner.run()?;
    
    println!("=== Simulation Results ===");
    println!("Passed: {}", results.passed);
    println!("Violations: {}", results.violations.len());
    println!("Measurements: {:?}", results.measurements);
    println!("Simulation time: {:.6}s", results.simulation_time);
    
    Ok(())
}