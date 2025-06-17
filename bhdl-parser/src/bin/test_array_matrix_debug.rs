use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 3.3V @ 1A;
    ground GND;
    
    // Complex array access
    matrix[2][3].K -> GND;
}
"#;
    
    let result = parse(input);
    
    if !result.errors().is_empty() {
        println!("Parse errors found:");
        for error in result.errors() {
            println!("  {}", error.message);
        }
    } else {
        println!("✅ Parsing successful!");
    }
}