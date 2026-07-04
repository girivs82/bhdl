//! Simple LED circuit test to debug fault injection

use anyhow::Result;

fn main() -> Result<()> {
    println!("=== Simple LED Circuit Test ===\n");
    
    // Expected behavior:
    // Baseline: 5V -> 330Ω -> LED (2V drop) -> GND
    // Current = (5V - 2V) / 330Ω = 9mA
    
    // Fault: R1 shorts to 0.001Ω
    // Current = (5V - 2V) / 0.001Ω = 3000A (unrealistic, LED would burn)
    
    // The issue we're seeing:
    // 1. Baseline shows 300mA (0.3A) which is wrong
    // 2. Fault doesn't seem to change the current
    
    println!("Expected baseline current: ~9mA");
    println!("Expected fault current: >1A (LED would burn)\n");
    
    // Let's trace through the testbench runner to see what's happening
    println!("The problem appears to be:");
    println!("1. LED current calculation might be wrong");
    println!("2. Fault injection might not be updating the circuit properly");
    println!("3. SPICE solver convergence issues");
    
    Ok(())
}