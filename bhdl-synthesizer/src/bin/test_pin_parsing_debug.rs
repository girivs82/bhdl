use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, HasName};

fn main() {
    println!("=== Testing Pin Parsing ===\n");
    
    // Test with a simple module with multiple pins
    let test_code = r#"
entity TestModule() {
    pin VIN: power in;
    pin SW: switch out;
    pin GND: ground inout;
    pin VOUT: power out virtual;
}
"#;
    
    println!("Test code:\n{}", test_code);
    
    let parse_result = parse(test_code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    println!("\nEntities found:");
    for entity in source_file.entities() {
        if let Some(name) = entity.name() {
            println!("Entity: {}", name.text());

            let pins: Vec<_> = entity.pins().collect();
            println!("  Pins found: {}", pins.len());

            for pin in pins {
                if let Some(pin_name) = pin.name() {
                    println!("    - {}", pin_name.text());
                }
            }
        }
    }
    
    println!("\n=== Now testing actual TPS54331 file ===\n");
    
    // Test with actual TPS54331 file content (first few pins)
    let tps_code = r#"
entity TPS54331(
    vout: voltage = 3.3V
) {
    pin VIN: power in {
        voltage_range: (3.5V, 28V),
        voltage_abs_max: 30V,
        description: "Input supply"
    };
    
    pin SW: switch out {
        current_max: 4A,
        voltage_max: 30V,
        switching_freq: 570kHz,
        description: "Switch node to inductor"
    };
    
    pin GND: ground inout {
        description: "Ground and thermal pad"
    };
    
    pin VOUT: power out virtual {
        voltage: vout,
        current_max: 3A,
        description: "Virtual output pin"
    };
}
"#;
    
    println!("TPS54331 test code:\n{}", tps_code);
    
    let parse_result2 = parse(tps_code);
    if !parse_result2.errors().is_empty() {
        println!("Parse errors in TPS54331:");
        for error in parse_result2.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    
    let syntax2 = parse_result2.syntax();
    let source_file2 = SourceFile::cast(syntax2).expect("Failed to cast to SourceFile");
    
    println!("\nTPS54331 entities found:");
    for entity in source_file2.entities() {
        if let Some(name) = entity.name() {
            println!("Entity: {}", name.text());

            let pins: Vec<_> = entity.pins().collect();
            println!("  Pins found: {}", pins.len());

            for pin in pins {
                if let Some(pin_name) = pin.name() {
                    println!("    - {}", pin_name.text());
                }
            }
        }
    }
}