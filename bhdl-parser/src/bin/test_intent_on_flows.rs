// Test that intent clauses work on flow statements
use bhdl_parser::{parse, SyntaxKind};

fn main() {
    println!("Testing Intent on Flow Statements\n");
    
    let test_cases = vec![
        (
            "Named flow with intent",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    critical_path: @VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#
        ),
        (
            "Direct connection with intent",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#
        ),
        (
            "Flow with multiple intents",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    audio_path: @VCC -> @filtered -> amp.in for anti_alias(before: adc);
}
"#
        ),
        (
            "Complex flow with intent",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    power_path: @VCC -> fuse: Fuse(1A).1 -> @protected for overcurrent_protection();
}
"#
        ),
    ];
    
    for (name, code) in test_cases {
        println!("=== {} ===", name);
        let result = parse(code);
        
        if result.errors().is_empty() {
            println!("✓ Parsed successfully");
            
            // Find and display intent clauses
            find_intent_clauses(&result.syntax(), 0);
        } else {
            println!("✗ Parse errors:");
            for error in result.errors() {
                println!("  - {}", error.message);
            }
        }
        println!();
    }
}

fn find_intent_clauses(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    match node.kind() {
        SyntaxKind::INTENT_CLAUSE => {
            println!("{}Found INTENT_CLAUSE: {}", indent, node.text().to_string().trim());
        }
        SyntaxKind::INTENT_CALL => {
            println!("{}  Intent function: {}", indent, node.text().to_string().trim());
        }
        SyntaxKind::CONNECTION_STMT => {
            println!("{}In CONNECTION_STMT", indent);
        }
        SyntaxKind::FLOW_STMT => {
            println!("{}In FLOW_STMT", indent);
        }
        _ => {}
    }
    
    for child in node.children() {
        find_intent_clauses(&child, depth + 1);
    }
}