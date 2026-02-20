use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Comprehensive Net Syntax Rules\n");
    
    let test_cases = vec![
        (
            "Power/ground without @ (legacy - should error)",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> GND;
}
"#,
            vec!["Net 'VCC' must be referenced with @", "Net 'GND' must be referenced with @"],
        ),
        (
            "Power/ground with @ (correct)",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> @GND;
}
"#,
            vec![],
        ),
        (
            "Implicit net creation in flow",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> @filtered -> Res(10k).1;
    @filtered -> Cap(100n).1 -> @GND;
}
"#,
            vec![],
        ),
        (
            "Undefined net reference (should error)",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @undefined_net -> LED(red).A;
    LED(red).K -> @GND;
}
"#,
            vec!["Undefined net: @undefined_net"],
        ),
        (
            "Component handles vs nets",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> r1: Res(10k).1;
    r1.2 -> led: LED(red).A;
    led.K -> @GND;
}
"#,
            vec![],
        ),
        (
            "Net assignment syntax",
            r#"
board Test {
    power VIN = 12V @ 2A;
    ground GND;
    
    @protected: TVSDiode(15V).K <- @VIN;
    TVSDiode(15V).A -> @GND;
}
"#,
            vec![],
        ),
    ];
    
    for (name, code, expected_errors) in test_cases {
        println!("=== {} ===", name);
        
        // Parse
        let parse_result = parse(code);
        if !parse_result.errors().is_empty() {
            println!("✗ Parse errors: {:?}", parse_result.errors());
            continue;
        }
        
        let source_file = SourceFile::cast(parse_result.syntax())
            .expect("Should be a SourceFile");
        
        // Analyze
        let result = analyze(&source_file);
        
        // Check for expected errors
        let actual_errors: Vec<String> = result.diagnostics.iter()
            .filter(|d| d.message.contains("Undefined") || d.message.contains("Net") || d.message.contains("prefix"))
            .map(|d| d.message.clone())
            .collect();
        
        let mut found_all_expected = true;
        for expected in &expected_errors {
            if !actual_errors.iter().any(|e| e.contains(expected)) {
                println!("✗ Missing expected error: {}", expected);
                found_all_expected = false;
            }
        }
        
        let unexpected_errors: Vec<_> = actual_errors.iter()
            .filter(|e| !expected_errors.iter().any(|exp| e.contains(exp)))
            .collect();
        
        if !unexpected_errors.is_empty() {
            println!("✗ Unexpected errors:");
            for error in unexpected_errors {
                println!("  - {}", error);
            }
            found_all_expected = false;
        }
        
        // Debug: Show all actual errors
        if !found_all_expected {
            println!("Debug - All actual errors:");
            for error in &actual_errors {
                println!("  - {}", error);
            }
        }
        
        if found_all_expected {
            println!("✓ Correct error handling");
        }
        
        // Show power domains found
        if result.power_analysis.domains.len() > 0 {
            println!("  Power domains: {:?}", 
                result.power_analysis.domains.keys().collect::<Vec<_>>());
        }
        
        println!();
    }
}