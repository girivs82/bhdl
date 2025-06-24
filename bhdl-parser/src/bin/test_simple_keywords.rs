use bhdl_parser::{parse, SyntaxKind};

fn main() {
    // Test if various keywords are recognized
    let tests = vec![
        ("module test {}", "MODULE_KW"),
        ("with test {}", "WITH_KW"),  
        ("where test", "WHERE_KW"),
        ("if test", "IF_KW"),
        ("for test", "FOR_KW"),
    ];
    
    for (input, expected_kw) in tests {
        println!("\nTesting: '{}'", input);
        let result = parse(input);
        
        // Find first token
        let mut found_keyword = false;
        for token in result.syntax().descendants_with_tokens() {
            if let rowan::NodeOrToken::Token(t) = token {
                let kind = t.kind();
                if format!("{:?}", kind).contains(expected_kw) {
                    println!("  ✓ Found {}", expected_kw);
                    found_keyword = true;
                    break;
                }
            }
        }
        
        if !found_keyword {
            println!("  ✗ Did NOT find {}", expected_kw);
            // Print first few tokens
            println!("  First tokens:");
            let mut count = 0;
            for token in result.syntax().descendants_with_tokens() {
                if let rowan::NodeOrToken::Token(t) = token {
                    let kind = t.kind();
                    if kind != SyntaxKind::WHITESPACE {
                        println!("    {:?} '{}'", kind, t.text());
                        count += 1;
                        if count > 5 { break; }
                    }
                }
            }
        }
    }
}