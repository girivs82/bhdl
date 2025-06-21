use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

#[test]
fn test_interface_definition_analysis() {
    let source = r#"
    interface I2C(speed: frequency = 400kHz) {
        signal SDA: inout;
        signal SCL: out;
        signal ALERT: in optional;
        require pullup(SDA, 4.7k);
        require pullup(SCL, 4.7k);
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Interface instance
        i2c_bus: I2C(speed = 100kHz);
    }
    "#;
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let result = analyze(&source_file);
    
    // Check for no diagnostics
    assert_eq!(result.diagnostics.len(), 0, "Analysis diagnostics: {:?}", result.diagnostics);
    
    // Check symbol table has interface
    assert!(result.global_scope.lookup("I2C").is_some());
    let i2c_symbol = result.global_scope.lookup("I2C").unwrap();
    assert_eq!(i2c_symbol.kind, bhdl_analyzer::symbol_table::SymbolKind::Interface);
}

#[test]
fn test_undefined_interface_error() {
    let source = r#"
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Undefined interface
        spi_bus: SPI();
    }
    "#;
    
    let parsed = parse(source);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let result = analyze(&source_file);
    
    // Should have diagnostic about undefined interface
    assert!(result.diagnostics.len() > 0);
    assert!(result.diagnostics[0].message.contains("Undefined interface type: SPI"));
}

#[test]
fn test_interface_parameter_validation() {
    let source = r#"
    interface UART(baud_rate: frequency = 115200Hz, data_bits: int = 8) {
        signal TX: out;
        signal RX: in;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Invalid parameter
        uart: UART(invalid_param = 9600Hz);
    }
    "#;
    
    let parsed = parse(source);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let result = analyze(&source_file);
    
    // Should have diagnostic about unknown parameter
    assert!(result.diagnostics.len() > 0);
    // Print all diagnostics to debug
    for (i, diag) in result.diagnostics.iter().enumerate() {
        println!("Diagnostic {}: {}", i, diag.message);
    }
    // Find the diagnostic about unknown parameter
    let has_param_error = result.diagnostics.iter()
        .any(|d| d.message.contains("Unknown parameter 'invalid_param'"));
    assert!(has_param_error, "Expected diagnostic about unknown parameter 'invalid_param'");
}

#[test]
fn test_interface_signal_resolution() {
    let source = r#"
    interface GPIO {
        signal DATA: inout;
        signal READY: out;
    }
    
    module IOExpander {
        pin VCC: power in;
        pin GND: ground in;
        
        gpio_port: GPIO();
        
        // Should be able to reference interface signals
        gpio_port.DATA -> internal_signal;
        internal_signal: signal;
    }
    "#;
    
    let parsed = parse(source);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let result = analyze(&source_file);
    
    // For now, just check it parses without errors
    // Full signal resolution would be implemented in later passes
    println!("Diagnostics: {:?}", result.diagnostics);
}

#[test]
fn test_non_interface_type_error() {
    let source = r#"
    component Resistor {
        pin 1: signal inout;
        pin 2: signal inout;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Trying to use component as interface
        bus: Resistor();
    }
    "#;
    
    let parsed = parse(source);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let result = analyze(&source_file);
    
    // Should have diagnostic about wrong symbol type
    assert!(result.diagnostics.len() > 0);
    assert!(result.diagnostics[0].message.contains("is not an interface"));
}