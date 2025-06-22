use bhdl_parser::{parse, SyntaxKind};

fn main() {
    println!("Testing @ Syntax Implementation\n");
    
    let test_cases = vec![
        (
            "Current: Power without @",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC -> Res(10k).1;
    GND -> Cap(1uF).2;
}
"#
        ),
        (
            "Desired: Power with @",
            r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> Res(10k).1;
    @GND -> Cap(1uF).2;
}
"#
        ),
        (
            "Named net with @",
            r#"
board Test {
    power VCC = 5V @ 1A;
    
    @VCC -> @filtered -> Res(10k).1;
    @filtered -> Cap(100n).1;
}
"#
        ),
        (
            "Component handles",
            r#"
board Test {
    power VCC = 5V @ 1A;
    
    @VCC -> r1: Res(10k).1;
    r1.2 -> led: LED(red).A;
}
"#
        ),
    ];
    
    for (name, code) in test_cases {
        println!("=== {} ===", name);
        let result = parse(code);
        let errors = result.errors();
        
        if errors.is_empty() {
            println!("✓ Parses successfully");
        } else {
            println!("✗ Parse errors:");
            for error in errors {
                println!("  - {}", error.message);
            }
        }
        println!();
    }
    
    // Test what tokens are produced
    println!("=== Token Analysis ===");
    let samples = vec![
        ("Net ref", "@VCC"),
        ("Power ref", "VCC"),
        ("Component", "r1: Res(10k)"),
        ("Flow", "@VCC -> @filtered"),
    ];
    
    for (desc, sample) in samples {
        println!("\n{}: {}", desc, sample);
        let tokens = bhdl_parser::lex(sample);
        for (kind, text) in &tokens {
            if !matches!(kind, SyntaxKind::WHITESPACE) {
                println!("  {:?} -> \"{}\"", kind, text);
            }
        }
    }
}