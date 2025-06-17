use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Test cases that might be failing
    osc: Oscillator(10M);
    clock: Clock(100k);
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