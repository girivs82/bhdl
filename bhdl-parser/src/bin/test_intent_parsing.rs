use bhdl_parser::{lex, parse, Parser, SyntaxKind};

fn main() {
    let test_cases = vec![
        // Basic net flow with intent
        r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    net delayed: signal -> R(10k).1 -> C(300pF).1 -> buffer.in
        for delay(3ms);
    
    net filtered: noisy_signal -> RC_Filter(1kHz)
        for anti_alias(before: adc);
    
    net protection: sensor -> tvs: TVSDiode(6V).cathode -> tvs.anode -> r: Res(1k).1 -> r.2 -> @protected
        for input_protection(overvoltage: 6V, current_limit: 5mA);
}
"#,
        // Net flow without intent (should still parse)
        r#"
board SimpleBoard {
    net simple: VCC -> R(1k).1 -> LED(red).A -> GND;
}
"#,
        // Multiple intents (future feature, should parse single intent for now)
        r#"
board MultiIntent {
    net critical: sensor -> amplifier -> adc
        for low_noise(max_ripple: 1mV);
}
"#,
    ];

    for (i, test) in test_cases.iter().enumerate() {
        println!("\n=== Test Case {} ===", i + 1);
        println!("Input:\n{}", test);
        
        // Lex the input
        let tokens = lex(test);
        println!("\nTokens: {:?}", tokens.iter()
            .filter(|(kind, _)| !matches!(kind, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
            .collect::<Vec<_>>());
        
        // Parse the input
        let result = parse(test);
        let syntax = result.syntax();
        println!("\nParsed successfully: {}", result.errors().is_empty());
        
        if !result.errors().is_empty() {
            println!("Errors:");
            for error in result.errors() {
                println!("  - {}", error.message);
            }
        }
        
        // Print the syntax tree
        println!("\nSyntax tree:");
        print_tree(&syntax, 0);
        
        // Look for intent clauses
        println!("\n--- Intent Analysis ---");
        find_intents(&syntax, 0);
    }
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let kind = node.kind();
    let text = node.text().to_string().replace('\n', "\\n");
    
    if node.children().count() == 0 {
        // Leaf node - show text
        println!("{:indent$}{:?} \"{}\"", "", kind, text, indent = indent);
    } else {
        // Internal node
        println!("{:indent$}{:?}", "", kind, indent = indent);
        for child in node.children() {
            print_tree(&child, indent + 2);
        }
    }
}

fn find_intents(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    if node.kind() == SyntaxKind::NET_FLOW_STMT {
        println!("Found NET_FLOW_STMT at depth {}", depth);
        
        // Look for intent clause
        for child in node.children() {
            if child.kind() == SyntaxKind::INTENT_CLAUSE {
                println!("  Has INTENT_CLAUSE!");
                
                // Find the intent call
                for intent_child in child.children() {
                    if intent_child.kind() == SyntaxKind::INTENT_CALL {
                        println!("    Intent call: {}", intent_child.text());
                    }
                }
            }
        }
    }
    
    // Recurse into children
    for child in node.children() {
        find_intents(&child, depth + 1);
    }
}