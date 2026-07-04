use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 3.3V @ 1A;
    ground GND;
    
    VCC -> button: Switch().1;
    button.2 -> GND;
}
"#;
    
    let result = parse(input);
    
    println!("Errors: {}", result.errors().len());
    for error in result.errors() {
        println!("  {}", error.message);
    }
}