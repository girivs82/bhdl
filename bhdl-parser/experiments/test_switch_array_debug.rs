use bhdl_parser::parse;

fn main() {
    let input = r#"
board TestBoard {
    power VCC_3V3 = 3.3V @ 1A;
    ground GND;
    
    generate for i in 0..4 {
        VCC_3V3 -> pullup[i]: Res(10k).1 -> button[i]: Switch().1;
        button[i].2 -> GND;
    }
}
"#;
    
    let result = parse(input);
    
    println!("Errors: {}", result.errors().len());
    for error in result.errors() {
        println!("  {}", error.message);
    }
}