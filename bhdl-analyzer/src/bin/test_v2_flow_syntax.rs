// Test analyzer support for v2.0 flow syntax

use bhdl_ast::{AstNode, SourceFile};
use bhdl_parser::parse;

fn main() {
    println!("=== Testing BHDL Analyzer v2.0 Flow Syntax Support ===\n");
    
    // Test case 1: Simple flow with component instantiation
    let test1 = r#"
board SimpleFlow {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> R1: Res(10k).1 -> R1.2 -> LED1: LED(red).A -> LED1.K -> @GND;
}
"#;
    
    println!("Test 1: Simple flow with component instantiation");
    println!("Code:\n{}", test1);
    
    let parse_result = parse(test1);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  {:?}", error);
        }
    } else {
        println!("✅ Parsing successful");
    }
    
    let ast = SourceFile::cast(parse_result.syntax().clone()).unwrap();
    let analysis_result = bhdl_analyzer::analyze(&ast);
    
    println!("\nDiagnostics:");
    if analysis_result.diagnostics.is_empty() {
        println!("  ✅ No diagnostics - v2.0 flow syntax correctly recognized!");
    } else {
        for diag in &analysis_result.diagnostics {
            println!("  ❌ {:?}: {}", diag.range, diag.message);
        }
    }
    
    // Test case 2: Flow with undefined component
    let test2 = r#"
board UndefinedComponent {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> UndefinedComp(10k).1 -> @GND;
}
"#;
    
    println!("\n\nTest 2: Flow with undefined component");
    println!("Code:\n{}", test2);
    
    let parse_result = parse(test2);
    let ast = SourceFile::cast(parse_result.syntax().clone()).unwrap();
    let analysis_result = bhdl_analyzer::analyze(&ast);
    
    println!("\nDiagnostics:");
    if analysis_result.diagnostics.is_empty() {
        println!("  ❌ Expected diagnostic for undefined component!");
    } else {
        for diag in &analysis_result.diagnostics {
            println!("  Expected: {:?}: {}", diag.range, diag.message);
        }
    }
    
    // Test case 3: Flow with LM7805 (should work if stdlib is loaded)
    let test3 = r#"
board VoltageRegulator {
    power VIN = 12V @ 500mA;
    power VOUT = 5V @ 400mA;
    ground GND;
    
    @VIN -> U1: LM7805().IN;
    U1.GND -> @GND;
    U1.OUT -> @VOUT;
}
"#;
    
    println!("\n\nTest 3: Flow with LM7805 regulator");
    println!("Code:\n{}", test3);
    
    let parse_result = parse(test3);
    let ast = SourceFile::cast(parse_result.syntax().clone()).unwrap();
    let analysis_result = bhdl_analyzer::analyze(&ast);
    
    println!("\nDiagnostics:");
    for diag in &analysis_result.diagnostics {
        println!("  {:?}: {}", diag.range, diag.message);
    }
    
    if analysis_result.diagnostics.is_empty() {
        println!("  ✅ No diagnostics - LM7805 correctly recognized!");
    }
    
    println!("\n=== Test Complete ===");
}