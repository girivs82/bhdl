//! Test testbench parsing

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, TestbenchDef, HasName};

fn main() {
    println!("=== BHDL Testbench Parsing Test ===\n");
    
    let input = r#"
// Simple LED circuit
board SimpleLED {
    power VCC = 5V @ 100mA;
    ground GND;
    
    @VCC -> R1: Resistor(330).1 -> R1.2 -> LED1: LED(red).A;
    LED1.K -> @GND;
}

// Testbench definition
testbench TB_SimpleLED for SimpleLED {
    simulation {
        duration: 10ms;
        timestep: 10us;
        solver: spice;
        temperature: 25;
    }
    
    scope "main" {
        signals: @VCC, @GND, R1.current;
        capture: continuous;
    }
    
    stimulus {
        @VCC: ramp(from: 0V, to: 5V, duration: 1ms);
    }
    
    verify {
        assert R1.current < 20mA always
            message "Current too high";
    }
    
    measure {
        avg_current = average(R1.current);
    }
}
    "#;
    
    // Parse the input
    let result = parse(input);
    
    // Check for errors
    if !result.errors().is_empty() {
        println!("Parse errors:");
        for error in result.errors() {
            println!("  - {}", error.message);
        }
    }
    
    // Get the syntax tree
    let root = result.syntax();
    println!("Syntax tree parsed successfully\n");
    
    // Cast to SourceFile
    if let Some(source_file) = SourceFile::cast(root.clone()) {
        // Find boards
        for board in source_file.boards() {
            if let Some(name) = board.name() {
                println!("Found board: {}", name.text());
            }
        }
        
        // Find testbenches
        for testbench in source_file.testbenches() {
            if let Some(name) = testbench.name() {
                println!("Found testbench: {}", name.text());
            }
            
            if let Some(target) = testbench.target_board() {
                println!("  Target board: {}", target.text());
            }
            
            // Check simulation block
            if let Some(sim_block) = testbench.simulation_block() {
                println!("  Has simulation configuration");
                
                if let Some(duration) = sim_block.duration() {
                    if let Some(num) = duration.number() {
                        println!("    Duration: {}", num.text());
                    }
                }
            }
            
            // Count scopes
            let scope_count = testbench.scopes().count();
            println!("  {} scopes defined", scope_count);
            
            // Check other blocks
            if testbench.stimulus_block().is_some() {
                println!("  Has stimulus block");
            }
            
            if testbench.verify_block().is_some() {
                println!("  Has verify block");
            }
            
            if testbench.measure_block().is_some() {
                println!("  Has measure block");
            }
        }
    } else {
        println!("Failed to cast to SourceFile");
    }
    
    println!("\n=== Test Complete ===");
}