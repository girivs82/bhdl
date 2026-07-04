use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 3.3V @ 1A;
    ground GND;
    
    // Nested generate
    generate for row in 0..4 {
        generate for col in 0..4 {
            matrix[row][col]: LED(red);
        }
    }
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