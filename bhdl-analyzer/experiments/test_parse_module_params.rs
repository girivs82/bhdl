use bhdl_parser::parse;

fn main() {
    let test_code = r#"
entity PWMController(frequency: frequency = 100kHz) {
    pin VCC: power in;
    pin OUT: signal out;
    pin EN: signal in;
    
    parameter duty_cycle: percentage = 50%;
}
"#;

    println!("Testing entity parameter parsing...\n");
    println!("Code:\n{}", test_code);
    
    let parse_result = parse(test_code);
    
    println!("\nParse errors: {}", parse_result.errors().len());
    for error in parse_result.errors() {
        println!("  - {}", error.message);
    }
    
    let syntax = parse_result.syntax();
    println!("\nSyntax tree:");
    println!("{:#?}", syntax);
}