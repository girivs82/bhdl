use bhdl_parser::parse;

fn main() {
    let input = r#"
board MixedTest {
    data_flow: sensor |> main_spi |> processor;
}
"#;
    
    let result = parse(input);
    
    println!("Errors: {}", result.errors().len());
    for error in result.errors() {
        println!("  {}", error.message);
    }
}