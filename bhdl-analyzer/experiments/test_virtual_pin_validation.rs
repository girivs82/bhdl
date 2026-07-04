// Test virtual pin validation in analyzer Pass2
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

fn main() {
    // Test case with invalid virtual pins that should generate warnings
    let content_with_errors = r#"
entity TestModule() {
    // Invalid: virtual pin with 'in' direction
    pin INPUT_VIRTUAL: virtual power in;
    
    // Valid: virtual pin with 'out' direction
    pin VALID_OUT: virtual power out;
    
    // Valid: virtual pin with 'inout' direction  
    pin VALID_INOUT: virtual signal inout;
}
"#;

    println!("Testing virtual pin validation with error cases...");
    let parsed = parse(content_with_errors);
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        return;
    }

    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    println!("Analysis diagnostics (should find {} validation errors):", 1);
    for (i, diag) in analysis.diagnostics.iter().enumerate() {
        println!("  {}: {}", i + 1, diag.message);
    }
    
    let virtual_pin_errors = analysis.diagnostics.iter()
        .filter(|d| d.message.contains("Virtual pin"))
        .count();
        
    if virtual_pin_errors >= 1 {
        println!("✓ Virtual pin validation working correctly - found expected errors");
    } else {
        println!("✗ Expected at least 1 virtual pin validation error, found {}", virtual_pin_errors);
    }
    
    // Test valid virtual pins
    println!("\nTesting valid virtual pins...");
    let content_valid = r#"
entity ValidModule() {
    pin VOUT: virtual power out;
    pin CONTROL: virtual signal inout; 
    pin GND_REF: virtual ground out;
}
"#;

    let parsed_valid = parse(content_valid);
    if !parsed_valid.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed_valid.errors() {
            println!("  - {}", error.message);
        }
        return;
    }

    let source_file_valid = SourceFile::cast(parsed_valid.syntax()).unwrap();
    let analysis_valid = analyze(&source_file_valid);
    
    println!("Analysis diagnostics for valid case (should be 0 virtual pin errors):");
    let virtual_pin_errors = analysis_valid.diagnostics.iter()
        .filter(|d| d.message.contains("Virtual pin"))
        .count();
        
    if virtual_pin_errors == 0 {
        println!("✓ Valid virtual pins passed validation");
    } else {
        println!("✗ Found {} unexpected virtual pin validation errors:", virtual_pin_errors);
        for diag in analysis_valid.diagnostics.iter().filter(|d| d.message.contains("Virtual pin")) {
            println!("  - {}", diag.message);
        }
    }
}