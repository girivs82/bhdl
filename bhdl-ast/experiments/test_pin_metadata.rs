//! Test pin metadata parsing

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, HasName};

fn main() {
    let test_cases = vec![
        ("Basic metadata", r#"
entity LM7805() {
    pin IN: power in @metadata(function="PowerIn");
    pin OUT: power out @metadata(function="PowerOut");
    pin GND: ground @metadata(function="Ground");
}"#),
        ("Multiple attributes", r#"
entity BuckConverter() {
    pin SW: power inout @metadata(function="SwitchNode", max_voltage="30V", slew_rate="fast");
    pin FB: signal in @metadata(function="Feedback", impedance="high");
    pin COMP: signal out @metadata(function="Compensation");
}"#),
        ("Mixed quoted and unquoted", r#"
entity OpAmp() {
    pin IN+: signal in @metadata(function="Signal", impedance=10Mohm);
    pin IN-: signal in @metadata(function="Signal", impedance=10Mohm);
    pin OUT: signal out @metadata(function="Signal", drive_strength="50mA");
}"#),
        ("With conditional pins", r#"
entity ConfigurableRegulator(enable_mode: bool = true) {
    pin IN: power in @metadata(function="PowerIn");
    pin OUT: power out @metadata(function="PowerOut");
    pin EN: signal in when enable_mode @metadata(function="Enable", active="high");
    pin GND: ground @metadata(function="Ground");
}"#),
    ];
    
    for (name, source) in test_cases {
        println!("\n=== {} ===", name);
        
        let parse_result = parse(source);
        if !parse_result.errors().is_empty() {
            println!("Parse errors:");
            for error in parse_result.errors() {
                println!("  {}", error.message);
            }
        }
        
        let root = parse_result.syntax();
        let source_file = SourceFile::cast(root).expect("Expected SourceFile");
        
        // Find entity
        if let Some(entity) = source_file.entities().next() {
            println!("Entity: {}", entity.name().map(|n| n.text().to_string()).unwrap_or_default());

            // Check pins
            for pin in entity.pins() {
                let pin_name = pin.name().map(|n| n.text().to_string()).unwrap_or_default();
                print!("  Pin {}: ", pin_name);
                
                if let Some(metadata) = pin.metadata() {
                    println!("has metadata");
                    for pair in metadata.pairs() {
                        if let (Some(key), Some(value)) = (
                            pair.key().map(|k| k.text().to_string()),
                            pair.value()
                        ) {
                            println!("    {} = {}", key, value);
                        }
                    }
                } else {
                    println!("no metadata");
                }
            }
        }
    }
}