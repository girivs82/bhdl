use bhdl_parser::lex;

fn main() {
    let input = "board Test { satisfies { } }";
    
    println!("Lexing: {}", input);
    let tokens = lex(input);
    
    println!("\nTokens:");
    for (i, token) in tokens.iter().enumerate() {
        println!("  {}: {:?}", i, token);
    }
}