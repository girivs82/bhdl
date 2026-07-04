use bhdl_parser::lex;

fn main() {
    let input = r#"import { voltage, current, power, resistance } from "../units.bhdl";"#;
    let tokens = lex(input);
    
    for (i, (kind, text)) in tokens.iter().enumerate() {
        println!("{}: {:?} = '{}'", i, kind, text);
    }
    
    println!("\nParsing result:");
    let parse_result = bhdl_parser::parse(input);
    println!("Errors: {:?}", parse_result.errors());
}