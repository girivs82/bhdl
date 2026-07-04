use bhdl_parser::{lex, parse, SyntaxKind};

fn main() {
    println!("Testing intent syntax options...\n");
    
    let test_cases = vec![
        (
            "Option 1: net keyword (current)", 
            "board Test { net critical: VCC -> R(1k).1 for delay(3ms); }"
        ),
        (
            "Option 2: flow with intent",
            "board Test { critical: VCC -> R(1k).1 for delay(3ms); }"
        ),
        (
            "Option 3: connection with intent",
            "board Test { VCC -> R(1k).1 for delay(3ms); }"
        ),
    ];
    
    for (name, code) in test_cases {
        println!("=== {} ===", name);
        println!("Code: {}", code);
        
        let result = parse(code);
        let errors = result.errors();
        
        if errors.is_empty() {
            println!("✓ Parsed successfully!");
        } else {
            println!("✗ Parse errors:");
            for error in errors {
                println!("  - {}", error.message);
            }
        }
        println!();
    }
}