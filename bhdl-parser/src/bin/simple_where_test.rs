use bhdl_parser::{parse, SyntaxKind, BhdlLanguage};

fn main() {
    // Simplest possible test
    let test = r#"
board Test {
    A -> B where x = 1;
}
"#;

    println!("Input:\n{}", test);
    
    let result = parse(test);
    
    println!("\nErrors: {}", result.errors().len());
    for err in result.errors() {
        println!("  {:?}", err);
    }
    
    // Print all tokens
    println!("\nTokens:");
    for token in result.syntax().descendants_with_tokens() {
        if let rowan::NodeOrToken::Token(t) = token {
            let kind = t.kind();
            if kind != SyntaxKind::WHITESPACE && kind != SyntaxKind::COMMENT {
                println!("  {:?} '{}'", kind, t.text());
            }
        }
    }
}