use bhdl_parser::parse;

fn main() {
    println!("Testing specific nested object issues...");
    
    // Test cases to isolate parsing problems
    let test_cases = vec![
        // Simple array with decimal units
        "const values: values = [1k, 2.2k, 4.7k, 10k];",
        
        // Object with tuple field
        "const range: range = { voltage_range: (3.3V, 5V) };",
        
        // Object with negative temperatures
        "const temps: temps = { min: -40C, max: 85C };",
        
        // Mixed successful case
        "const config: config = {
            components: [
                { type: \"resistor\", value: 10k },
                { type: \"capacitor\", value: 100uF }
            ]
        };",
    ];
    
    for (i, input) in test_cases.iter().enumerate() {
        println!("\n--- Test Case {}: ---", i + 1);
        println!("{}", input);
        
        // Parse the const declaration
        let result = parse(input);
        
        if result.errors().is_empty() {
            println!("✅ Parsed successfully");
        } else {
            println!("❌ Parse errors:");
            for (i, error) in result.errors().iter().enumerate() {
                if i < 5 { // Show only first 5 errors for readability
                    println!("  - {}", error.message);
                }
            }
            if result.errors().len() > 5 {
                println!("  ... and {} more errors", result.errors().len() - 5);
            }
        }
    }
}