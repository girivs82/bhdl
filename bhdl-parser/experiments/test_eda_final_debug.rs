use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC -> osc: Oscillator(10M).OUT;
}
"#;
    
    let result = parse(input);
    
    println!("Errors: {}", result.errors().len());
    for error in result.errors() {
        println!("  {}", error.message);
    }
}