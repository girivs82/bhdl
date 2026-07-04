use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Complex component with multiple parameters
    reg: LinearReg(3.3V, 1A, package="TO-220").IN;
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