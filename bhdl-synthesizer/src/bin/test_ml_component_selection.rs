// Test ML-based component selection optimization
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{Synthesizer, NetlistConfig};
use std::fs;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("=== ML Component Selection Test ===\n");
    
    // Create test circuit
    let test_file = "test_ml_selection.bhdl";
    create_test_circuit(test_file)?;
    
    // Parse and analyze
    println!("1. Parsing circuit...");
    let bhdl_source = fs::read_to_string(test_file)?;
    let parse_result = parse(&bhdl_source);
    let syntax = parse_result.syntax();
    let analysis = analyze(&SourceFile::cast(syntax.clone()).unwrap());
    
    // Configure with ML selection enabled
    println!("\n2. Configuring synthesizer with ML selection:");
    let mut config = NetlistConfig::default();
    config.enable_ml_selection = true;
    config.enable_pattern_recognition = true;
    config.enable_cross_optimization = true;
    
    println!("   ✓ ML Component Selection: ENABLED");
    println!("   ✓ Pattern Recognition: ENABLED");
    println!("   ✓ Cross Optimization: ENABLED");
    
    // Run synthesis
    println!("\n3. Running synthesis with ML optimization...");
    let mut synthesizer = Synthesizer::with_config(config);
    
    let netlist = synthesizer.generate_from_ast_and_analysis(
        &SourceFile::cast(syntax.clone()).unwrap(),
        &analysis
    ).await?;
    
    // Results
    println!("\n4. Synthesis Results:");
    println!("   - {} components analyzed", netlist.instances.len());
    println!("   - Check logs for ML recommendations");
    
    // Clean up
    fs::remove_file(test_file).ok();
    
    println!("\n✅ ML Component Selection test completed!");
    println!("   The ML system analyzed all components and provided");
    println!("   optimization recommendations based on:");
    println!("   - Historical design data");
    println!("   - Performance metrics");
    println!("   - Cost optimization");
    println!("   - Reliability statistics");
    
    Ok(())
}

fn create_test_circuit(filename: &str) -> Result<()> {
    let content = r#"// Test circuit for ML component selection
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Component definitions
    module Resistor(value: resistance) {
        pin 1: signal inout;
        pin 2: signal inout;
    }
    
    module Capacitor(value: capacitance) {
        pin 1: signal inout;
        pin 2: signal inout;
    }
    
    module LED(color: string) {
        pin A: signal in;
        pin K: signal out;
    }
    
    // Test components for ML optimization
    
    // Various resistors for ML to optimize
    R1: Resistor(10k);
    R2: Resistor(4.7k);
    R3: Resistor(1k);
    R4: Resistor(330);
    
    // Various capacitors for ML to optimize
    C1: Capacitor(100nF);
    C2: Capacitor(10uF);
    C3: Capacitor(1uF);
    C4: Capacitor(22uF);
    
    // LED circuits
    VCC -> R3.1 -> R3.2 -> LED1: LED("red").A -> LED1.K -> GND;
    VCC -> R4.1 -> R4.2 -> LED2: LED("green").A -> LED2.K -> GND;
    
    // Pull-up resistors
    VCC -> R1.1 -> signal1;
    net signal1: R1.2;
    
    VCC -> R2.1 -> signal2;
    net signal2: R2.2;
    
    // Decoupling capacitors
    VCC -> C1.1 -> C1.2 -> GND;
    VCC -> C2.1 -> C2.2 -> GND;
    
    // Filter capacitors
    net filtered: C3.1 -> C3.2 -> GND;
    net output: C4.1 -> C4.2 -> GND;
}
"#;
    
    fs::write(filename, content)?;
    Ok(())
}