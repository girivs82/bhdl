use bhdl_parser::parse;
use std::fs;

fn main() {
    let files_to_test = vec![
        "bhdl-stdlib/passives/capacitor_simple.bhdl",
        "bhdl-stdlib/passives/inductor_simple.bhdl",
        "bhdl-stdlib/passives/resistor_simple.bhdl",
        "tests/circuits/realistic/buck_converter_tps54302.bhdl",
    ];
    
    let mut all_passed = true;
    
    for file_path in files_to_test {
        println!("\n=== Testing: {} ===", file_path);
        
        match fs::read_to_string(file_path) {
            Ok(content) => {
                let result = parse(&content);
                
                if !result.errors().is_empty() {
                    println!("❌ FAILED - Parse errors:");
                    for (i, err) in result.errors().iter().enumerate() {
                        if i < 10 { // Show first 10 errors
                            println!("  {:?}", err);
                        }
                    }
                    if result.errors().len() > 10 {
                        println!("  ... and {} more errors", result.errors().len() - 10);
                    }
                    all_passed = false;
                } else {
                    println!("✓ Parsed successfully");
                }
            }
            Err(e) => {
                println!("❌ FAILED - Could not read file: {}", e);
                all_passed = false;
            }
        }
    }
    
    if all_passed {
        println!("\n✅ All library files parsed successfully!");
    } else {
        println!("\n❌ Some library files failed to parse");
        std::process::exit(1);
    }
}