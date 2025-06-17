use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    constrain {
        // Length matching
        length(clk) == length(data) +/- 10mil;
    }
}
"#;
    
    let result = parse(input);
    
    if !result.errors().is_empty() {
        println!("Parse errors found:");
        for error in result.errors() {
            println!("  {}", error.message);
        }
        
        // Print syntax tree to debug
        println!("\nSyntax tree:");
        let syntax = result.syntax();
        println!("{:#?}", syntax);
    } else {
        println!("✅ Parsing successful!");
    }
}