//! Single step debug to test SPICE solver

use anyhow::Result;
use std::path::Path;

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, HasName};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_testbench::{TestbenchRunner, WaveformFormat};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Single Step SPICE Debug ===");
    
    // Read the testbench file
    let testbench_content = std::fs::read_to_string("tests/circuits/testbenches/simple_led_testbench_basic.bhdl")?;
    
    // Parse and analyze
    let parse_result = parse(&testbench_content);
    let ast = SourceFile::cast(parse_result.syntax()).unwrap();
    let board_def = ast.boards().next().unwrap();
    let testbench_def = ast.testbenches().next().unwrap();
    
    let analysis_result = analyze(&ast);
    println!("Analysis complete");
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&ast, &analysis_result).await?;
    println!("Netlist generated: {} instances, {} nets", 
             netlist.instances.len(), netlist.nets.len());
    
    // Compile testbench
    let testbench = bhdl_testbench::compiler::compile_testbench(&testbench_def)?;
    println!("Testbench compiled");
    
    // Create testbench runner
    let mut runner = TestbenchRunner::new(testbench, netlist, None)?;
    runner.add_waveform_output(WaveformFormat::CSV, Path::new("tests/outputs/simulation/single_step_debug.csv"))?;
    
    println!("Starting single step simulation...");
    
    // Just run the first few steps manually
    let duration = 0.0001; // 100 microseconds  
    let timestep = 0.00001; // 10 microseconds
    let mut current_time = 0.0;
    
    for step in 0..3 {
        println!("=== STEP {} at time {:.6}s ===", step, current_time);
        
        // Apply stimuli
        let stimuli = runner.stimulus_gen.get_values(current_time);
        println!("Stimuli: {} entries", stimuli.len());
        for (signal, value) in &stimuli {
            println!("  {:?} = {:.3}", signal, value);
        }
        
        // This would normally call the private methods, so let's just print what we would do
        println!("Would step SPICE solver here...");
        
        current_time += timestep;
        
        if current_time > duration {
            break;
        }
    }
    
    println!("Single step debug complete");
    
    Ok(())
}