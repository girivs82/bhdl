//! Simple coordinator test to verify SPICE solver setup

use anyhow::Result;

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use bhdl_testbench::coordinator::TestbenchRunner;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Simple Coordinator Test ===");
    
    // Create a very simple testbench in code
    let testbench_content = r#"
board SimpleLEDBoard {
    power VCC = 5V @ 100mA;
    ground GND;
    
    // Try a simple flow without intermediate nets
    @VCC -> R1: Res(330).1;
    R1.2 -> LED1: LED(red).A;
    LED1.K -> @GND;
}

testbench TB_SimpleLED for SimpleLEDBoard {
    simulation {
        duration: 100us;
        timestep: 10us;
        solver: spice;
        temperature: 25;
    }
    
    scope "main" {
        signals: @VCC, @GND, R1.current;
        capture: continuous;
    }
    
    stimulus {
        @VCC: 5V;
    }
    
    measure {
        avg_current = R1.current;
    }
    
    verify {
        assert R1.current in 5mA..15mA always message "LED current in safe range";
        assert R1.current > 5mA always message "LED current sufficient";
        assert @VCC == 5V +/- 0.1V always message "VCC voltage stable";
    }
}
"#;
    
    let parse_result = parse(testbench_content);
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
    println!("Testbench compiled successfully");
    
    // Create testbench runner
    println!("Creating TestbenchRunner with SPICE solver...");
    let mut runner = TestbenchRunner::new_with_analysis(testbench, netlist, None, Some(analysis_result))?;
    
    println!("✓ TestbenchRunner created successfully");
    println!("✓ SPICE solver should be set up with component models");
    
    // Run simulation
    println!("Running simulation...");
    let results = runner.run()?;
    
    println!("=== Simulation Results ===");
    println!("Passed: {}", results.passed);
    println!("Violations: {}", results.violations.len());
    if !results.violations.is_empty() {
        println!("\nAssertion Violations:");
        for violation in &results.violations {
            println!("  [{:?}] {} at time {:.6}s", 
                violation.severity, violation.message, violation.time);
        }
    }
    println!("\nMeasurements: {:?}", results.measurements);
    println!("Simulation time: {:.6}s", results.simulation_time);
    
    Ok(())
}