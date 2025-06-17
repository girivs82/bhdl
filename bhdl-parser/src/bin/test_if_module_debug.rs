use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 3.3V @ 1A;
    ground GND;
    
    if (use_filter) {
        module NoiseFilter(VCC, VCC_FILTERED, GND) {
            flow: VCC |> filtering |> VCC_FILTERED;
        }
    }
}
"#;
    
    let result = parse(input);
    
    println!("Errors: {}", result.errors().len());
    for error in result.errors() {
        println!("  {}", error.message);
    }
}