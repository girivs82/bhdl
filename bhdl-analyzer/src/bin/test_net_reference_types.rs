use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Different Types of Net References\n");
    
    let test_cases = vec![
        (
            "Power ref with @ (should work)",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> @GND;
}
"#,
        ),
        (
            "Power ref without @ (should fail)",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> GND;
}
"#,
        ),
        (
            "Inline net creation",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> @filtered -> Res(10k).1;
    @filtered -> Cap(100n).1 -> @GND;
}
"#,
        ),
        (
            "Reference to non-existent net",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @undefined -> LED(red).A;
    LED(red).K -> @GND;
}
"#,
        ),
    ];
    
    for (name, code) in test_cases {
        println!("=== {} ===", name);
        
        // Parse
        let parse_result = parse(code);
        if !parse_result.errors().is_empty() {
            println!("Parse errors: {:?}", parse_result.errors());
            continue;
        }
        
        let source_file = SourceFile::cast(parse_result.syntax())
            .expect("Should be a SourceFile");
        
        // Analyze
        let result = analyze(&source_file);
        
        // Check for "undefined" errors
        let undefined_errors: Vec<_> = result.diagnostics.iter()
            .filter(|d| d.message.contains("Undefined"))
            .collect();
        
        if undefined_errors.is_empty() {
            println!("✓ No undefined reference errors");
        } else {
            println!("Undefined reference errors:");
            for error in undefined_errors {
                println!("  - {}", error.message);
            }
        }
        
        // Show nets in board scope
        for (_, scope) in result.definition_scopes.iter() {
            if scope.scope_name.as_ref().map(|n| n == "Test").unwrap_or(false) {
                if !scope.get_nets().is_empty() {
                    println!("Nets in board scope:");
                    for (name, _) in scope.get_nets().iter() {
                        println!("  - {}", name);
                    }
                }
            }
        }
        
        println!();
    }
}