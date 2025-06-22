use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing @ Net Resolution in Analyzer\n");
    
    let test_cases = vec![
        (
            "Power without @ (current)",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> GND;
}
"#
        ),
        (
            "Power with @ (new syntax)",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> @GND;
}
"#
        ),
        (
            "Mixed nets and components",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> r1: Res(10k).1;
    r1.2 -> @signal_net -> led: LED(red).A;
    led.K -> @GND;
}
"#
        ),
        (
            "Undefined net with @",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> Res(10k).1 -> @intermediate;
    @undefined_net -> LED(red).A;
    LED(red).K -> @GND;
}
"#
        ),
    ];
    
    for (name, code) in test_cases {
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
        
        // Check for errors - diagnostics are just warnings/errors
        let errors: Vec<_> = result.diagnostics.iter()
            .filter(|d| d.message.contains("undefined") || d.message.contains("error"))
            .collect();
        
        if errors.is_empty() {
            println!("✓ Analyzes successfully");
            
            // Show what symbols were found
            println!("  Power domains: {:?}", 
                result.power_analysis.domains.keys().collect::<Vec<_>>());
            
            // Show netlist info if available
            if let Some(netlist) = &result.netlist {
                println!("  Netlist generated successfully");
            }
        } else {
            println!("✗ Analysis errors:");
            for error in errors {
                println!("  - {}", error.message);
            }
        }
        println!();
    }
}