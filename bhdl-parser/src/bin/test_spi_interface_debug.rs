use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    main_spi: SPI(3.3V, 10M);
}
"#;
    
    let result = parse(input);
    
    println!("Errors: {}", result.errors().len());
    for error in result.errors() {
        println!("  {}", error.message);
    }
}