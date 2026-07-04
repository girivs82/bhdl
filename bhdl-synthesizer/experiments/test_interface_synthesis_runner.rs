use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use futures_util::FutureExt;

async fn test_interface_synthesis_i2c() {
    println!("\n=== Running test_interface_synthesis_i2c ===");
    
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
    let analysis = analyze(&source_file);
    
    // Should have no errors (except component inference warning about I2C)
    let non_inference_errors: Vec<_> = analysis.diagnostics.iter()
        .filter(|d| !d.message.contains("Component Inference"))
        .collect();
    assert_eq!(non_inference_errors.len(), 0, "Analysis errors: {:?}", non_inference_errors);
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let mut netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await
        .expect("Failed to generate netlist");
    
    // The interface synthesis should have been done as part of generate_from_ast_and_analysis
    // The issue is that interface instances are being processed as components, not interfaces
    
    // Check that nets were created for the interface
    let nets: Vec<_> = netlist.nets.iter()
        .map(|(_, net)| net.name.clone())
        .collect();
    
    println!("Generated nets: {:?}", nets);
    
    // Should have created nets for I2C signals
    // Note: Instance names are generated as U1, U2, etc. by component inference
    assert!(nets.iter().any(|n| n.as_ref().map(|s| s.contains("_SDA")).unwrap_or(false)), 
            "Should create SDA net for interface");
    assert!(nets.iter().any(|n| n.as_ref().map(|s| s.contains("_SCL")).unwrap_or(false)), 
            "Should create SCL net for interface");
    
    println!("✓ test_interface_synthesis_i2c passed");
}

async fn test_interface_synthesis_uart() {
    println!("\n=== Running test_interface_synthesis_uart ===");
    
    let source = r#"
    interface UART(baud_rate: frequency = 115200Hz, data_bits: int = 8) {
        signal TX: out;
        signal RX: in;
        signal RTS: out optional;
        signal CTS: in optional;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // UART interface instance
        uart1: UART(baud_rate = 9600Hz);
    }
    "#;
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let mut netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await
        .expect("Failed to generate netlist");
    
    // The interface synthesis should have been done as part of generate_from_ast_and_analysis
    // The issue is that interface instances are being processed as components, not interfaces
    
    // Check that nets were created for the interface
    let nets: Vec<_> = netlist.nets.iter()
        .map(|(_, net)| net.name.clone())
        .collect();
    
    println!("Generated nets: {:?}", nets);
    
    // Should have created nets for UART signals
    // Note: Instance names are generated as U1, U2, etc. by component inference
    assert!(nets.iter().any(|n| n.as_ref().map(|s| s.contains("_TX")).unwrap_or(false)), 
            "Should create TX net for interface");
    assert!(nets.iter().any(|n| n.as_ref().map(|s| s.contains("_RX")).unwrap_or(false)), 
            "Should create RX net for interface");
    
    println!("✓ test_interface_synthesis_uart passed");
}

async fn test_multiple_interface_instances() {
    println!("\n=== Running test_multiple_interface_instances ===");
    
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Multiple interface instances
        i2c1: I2C();
        i2c2: I2C();
    }
    "#;
    
    let parsed = parse(source);
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    let mut generator = NetlistGenerator::new();
    let mut netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await
        .expect("Failed to generate netlist");
    
    // The interface synthesis should have been done as part of generate_from_ast_and_analysis
    // The issue is that interface instances are being processed as components, not interfaces
    
    let nets: Vec<_> = netlist.nets.iter()
        .map(|(_, net)| net.name.clone())
        .collect();
    
    println!("Generated nets: {:?}", nets);
    
    // Should have separate nets for each interface instance
    // Note: Instance names are generated as U1, U2, etc. by component inference
    let sda_nets: Vec<_> = nets.iter().filter(|n| n.as_ref().map(|s| s.contains("_SDA")).unwrap_or(false)).collect();
    let scl_nets: Vec<_> = nets.iter().filter(|n| n.as_ref().map(|s| s.contains("_SCL")).unwrap_or(false)).collect();
    
    assert_eq!(sda_nets.len(), 2, "Should create two SDA nets");
    assert_eq!(scl_nets.len(), 2, "Should create two SCL nets");
    
    println!("✓ test_multiple_interface_instances passed");
}

#[tokio::main]
async fn main() {
    println!("Running interface synthesis tests...\n");
    
    // Run all tests directly
    let mut failed = false;
    
    // Test 1: I2C interface synthesis
    match std::panic::AssertUnwindSafe(test_interface_synthesis_i2c()).catch_unwind().await {
        Ok(_) => {},
        Err(e) => {
            eprintln!("✗ test_interface_synthesis_i2c failed: {:?}", e);
            failed = true;
        }
    }
    
    // Test 2: UART interface synthesis  
    match std::panic::AssertUnwindSafe(test_interface_synthesis_uart()).catch_unwind().await {
        Ok(_) => {},
        Err(e) => {
            eprintln!("✗ test_interface_synthesis_uart failed: {:?}", e);
            failed = true;
        }
    }
    
    // Test 3: Multiple interface instances
    match std::panic::AssertUnwindSafe(test_multiple_interface_instances()).catch_unwind().await {
        Ok(_) => {},
        Err(e) => {
            eprintln!("✗ test_multiple_interface_instances failed: {:?}", e);
            failed = true;
        }
    }
    
    // Summary
    println!("\n=== Test Summary ===");
    if failed {
        eprintln!("Some tests failed!");
        std::process::exit(1);
    } else {
        println!("All tests passed!");
    }
}