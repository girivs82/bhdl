use bhdl_parser::parse;

fn main() {
    // Test parsing built-in math functions
    let test_cases = vec![
        // Basic math functions
        ("const x = sqrt(16);", "sqrt function"),
        ("const y = abs(-5);", "abs function"),
        ("const z = pow(2, 8);", "pow function"),
        ("const a = min(10, 20);", "min function"),
        ("const b = max(10, 20);", "max function"),
        
        // Math in expressions
        ("attribute esr = 0.01 / sqrt(value * 1e6);", "sqrt in expression"),
        ("attribute power = abs(voltage) * current;", "abs in expression"),
        
        // Component with math in attributes
        (r#"module TestComponent() {
            attribute value = sqrt(100);
            attribute tolerance = abs(-5) + 1%;
        }"#, "module with math functions"),
    ];
    
    let mut all_passed = true;
    
    for (code, description) in test_cases.iter() {
        println!("\n=== Testing: {} ===", description);
        println!("Code: {}", code);
        
        let result = parse(code);
        
        if !result.errors().is_empty() {
            println!("❌ FAILED - Errors:");
            for err in result.errors() {
                println!("  {:?}", err);
            }
            all_passed = false;
        } else {
            println!("✓ Parsed successfully");
        }
    }
    
    // Now test parsing the actual capacitor library file
    println!("\n=== Testing Capacitor Library ===");
    let capacitor_code = r#"
module Capacitor(value: capacitance, voltage: voltage = 50V) {
    pin 1: signal inout;
    pin 2: signal inout;
    
    attribute component_class = "capacitor";
    attribute capacitance = value;
    attribute voltage_rating = voltage;
    
    // ESR approximation using sqrt
    attribute esr = 0.01 / sqrt(value * 1e6);
}"#;
    
    let result = parse(capacitor_code);
    if !result.errors().is_empty() {
        println!("❌ FAILED - Capacitor library errors:");
        for err in result.errors() {
            println!("  {:?}", err);
        }
        all_passed = false;
    } else {
        println!("✓ Capacitor library parsed successfully");
    }
    
    if all_passed {
        println!("\n✅ All tests passed!");
    } else {
        println!("\n❌ Some tests failed");
        std::process::exit(1);
    }
}