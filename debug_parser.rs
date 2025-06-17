use bhdl_parser::parse;

fn main() {
    let source = r#"
board LinearRegulator {
    VCC -> LED.A;
}
"#;
    
    let result = parse(source);
    if !result.errors().is_empty() {
        println!("Parse errors:");
        for error in result.errors() {
            println!("  - {}", error.message);
        }
    } else {
        println!("Parse successful!");
    }
    
    // Print the CST for debugging
    println!("\nCST:");
    println!("{:#?}", result.syntax());
}