use bhdl_parser::lexer::LexerToken;
use logos::Logos;

fn main() {
    let source = "board TestBoard {\n    power VDD = 3.3V @ 1A;\n}";
    println!("Source: {}", source);
    
    let mut lex = LexerToken::lexer(source);
    let mut tokens = Vec::new();
    
    while let Some(token_result) = lex.next() {
        let text = lex.slice();
        match token_result {
            Ok(token) => {
                println!("Token: {:?} = '{}'", token, text);
                tokens.push(token);
            }
            Err(()) => {
                println!("Error token: '{}'", text);
            }
        }
    }
}