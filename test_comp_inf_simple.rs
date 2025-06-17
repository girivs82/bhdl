// Simple test to check component inference voltage
use std::process::Command;

fn main() {
    println!("=== Testing Component Inference Voltage ===\n");
    
    // Create a test BHDL file
    let test_content = r#"
board SimpleTest {
    power VCC = 12V @ 1A;
    ground GND;
    
    // LED circuit at 12V
    VCC -> R1 -> LED1 -> GND;
    R1: Res(value);
    LED1: LED(red);
}
"#;
    
    // Write to file
    std::fs::write("test_voltage_inference.bhdl", test_content).unwrap();
    
    // Run the synthesizer end-to-end test
    println!("Running end-to-end test with 12V power domain...\n");
    
    let output = Command::new("cargo")
        .args(&["run", "-p", "bhdl-synthesizer", "--bin", "end_to_end_test"])
        .env("TEST_FILE", "test_voltage_inference.bhdl")
        .output()
        .expect("Failed to run test");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Look for component inference results
    if stdout.contains("Pass 6") && stdout.contains("Component Inference") {
        println!("Component inference pass found!");
        
        // Extract relevant lines
        for line in stdout.lines() {
            if line.contains("LED current limiting") || 
               line.contains("12V") ||
               line.contains("Res") ||
               line.contains("Reasoning:") {
                println!("  {}", line);
            }
        }
    }
    
    // Clean up
    std::fs::remove_file("test_voltage_inference.bhdl").ok();
    
    println!("\nTest completed!");
}