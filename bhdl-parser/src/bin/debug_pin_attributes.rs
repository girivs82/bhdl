use bhdl_parser::lex;

fn main() {
    let input = r#"entity TestModule {
    pin VIN: power in {
        voltage_range: (3.5V, 28V),
        voltage_abs_max: 30V,
        description: "Input supply"
    };
}"#;
    
    println!("Testing pin attribute parsing:");
    println!("Input: {}", input);
    println!("\nTokens:");
    let tokens = lex(input);
    
    for (i, (kind, text)) in tokens.iter().enumerate() {
        println!("{}: {:?} = '{}'", i, kind, text);
    }
    
    println!("\nParsing result:");
    let parse_result = bhdl_parser::parse(input);
    println!("Errors: {:?}", parse_result.errors());
}