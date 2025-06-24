use bhdl_parser::{lexer::{lex, LexerToken}, SyntaxKind};

fn main() {
    let test_inputs = vec![
        "where",
        "with",
        "where trace_length < 10mm",
        "with routing(impedance = 50)",
    ];
    
    for input in test_inputs {
        println!("\nInput: '{}'", input);
        let tokens = lex(input);
        println!("Tokens:");
        for (i, token) in tokens.iter().enumerate() {
            match &token.0 {
                Ok(token) => {
                    let kind = SyntaxKind::from(token.clone());
                    println!("  [{}] {:?} -> SyntaxKind::{:?}", i, token, kind);
                }
                Err(e) => println!("  [{}] Error: {:?}", i, e),
            }
        }
    }
}