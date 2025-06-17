use bhdl_parser::{parse, SyntaxKind};
use logos::Logos;
use bhdl_parser::lexer::LexerToken;

fn main() {
    // Test lexing of "10k"
    println!("=== Testing lexer on '10k' ===");
    let input = "10k";
    let lexer = LexerToken::lexer(input);
    let tokens: Vec<_> = lexer.spanned().collect();
    
    for (token_result, range) in &tokens {
        match token_result {
            Ok(token) => {
                println!("Token: {:?} at {:?} -> '{}'", token, range, &input[range.clone()]);
            }
            Err(_) => {
                println!("Error token at {:?}", range);
            }
        }
    }
    
    // Test lexing of "10kOhm"
    println!("\n=== Testing lexer on '10kOhm' ===");
    let input2 = "10kOhm";
    let lexer2 = LexerToken::lexer(input2);
    let tokens2: Vec<_> = lexer2.spanned().collect();
    
    for (token_result, range) in &tokens2 {
        match token_result {
            Ok(token) => {
                println!("Token: {:?} at {:?} -> '{}'", token, range, &input2[range.clone()]);
            }
            Err(_) => {
                println!("Error token at {:?}", range);
            }
        }
    }
    
    // Test parsing with abbreviated units
    println!("\n=== Testing parser with 'Res(10k)' ===");
    let test_input = r#"
    board test {
        components {
            R1: Res(10k);
        }
    }
    "#;
    
    let result = parse(test_input);
    if !result.errors().is_empty() {
        println!("Parse errors:");
        for error in result.errors() {
            println!("  - {}", error.message);
        }
    } else {
        println!("Parsed successfully!");
    }
    
    // Test parsing with full units
    println!("\n=== Testing parser with 'Res(10kOhm)' ===");
    let test_input2 = r#"
    board test {
        components {
            R1: Res(10kOhm);
        }
    }
    "#;
    
    let result2 = parse(test_input2);
    if !result2.errors().is_empty() {
        println!("Parse errors:");
        for error in result2.errors() {
            println!("  - {}", error.message);
        }
    } else {
        println!("Parsed successfully!");
    }
}