use bhdl_parser::{lex, parse, SyntaxKind, BhdlLanguage};

fn main() {
    println!("Testing different intent syntax options...\n");
    
    let test_cases = vec![
        // Option 1: Current implementation (net keyword)
        (
            "Option 1: net keyword (current implementation)", 
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    net critical: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#
        ),
        
        // Option 2: Flow statement with intent (aligns with spec)
        (
            "Option 2: flow statement with intent",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    critical_flow: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#
        ),
        
        // Option 3: Intent on connection statement
        (
            "Option 3: intent on direct connection",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#
        ),
        
        // Option 4: @ syntax with intent
        (
            "Option 4: @ syntax with intent",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC @critical-> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#
        ),
    ];
    
    for (name, code) in test_cases {
        println!("=== {} ===", name);
        println!("Code:\n{}", code);
        
        let result = parse(code);
        let errors = result.errors();
        
        if errors.is_empty() {
            println!("✓ Parsed successfully!");
            
            // Look for intent-related nodes
            let syntax = result.syntax();
            find_intent_nodes(&syntax, 0);
        } else {
            println!("✗ Parse errors:");
            for error in errors {
                println!("  - {}", error.message);
            }
        }
        println!();
    }
    
    // Show what tokens the lexer produces for "for delay"
    println!("=== Lexer Analysis ===");
    let sample = "VCC -> R(10k).1 for delay(3ms);";
    println!("Input: {}", sample);
    let tokens = lex(sample);
    println!("Tokens:");
    for (kind, text) in &tokens {
        if !matches!(kind, SyntaxKind::WHITESPACE) {
            println!("  {:?} -> \"{}\"", kind, text);
        }
    }
}

fn find_intent_nodes(node: &rowan::SyntaxNode<BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    match node.kind() {
        SyntaxKind::NET_FLOW_STMT => {
            println!("{}Found NET_FLOW_STMT", indent);
        }
        SyntaxKind::FLOW_STMT => {
            println!("{}Found FLOW_STMT", indent);
        }
        SyntaxKind::CONNECTION_STMT => {
            println!("{}Found CONNECTION_STMT", indent);
        }
        SyntaxKind::INTENT_CLAUSE => {
            println!("{}Found INTENT_CLAUSE: {}", indent, node.text());
        }
        _ => {}
    }
    
    for child in node.children() {
        find_intent_nodes(&child, depth + 1);
    }
}