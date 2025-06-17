use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 3.3V @ 1A;
    ground GND;
    
    if (high_speed) {
        VCC -> clock: Oscillator(100M).VDD;
    } else {
        VCC -> clock: Oscillator(10M).VDD;
    }
}
"#;
    
    let result = parse(input);
    
    println!("Errors: {}", result.errors().len());
    for error in result.errors() {
        println!("  {}", error.message);
    }
}