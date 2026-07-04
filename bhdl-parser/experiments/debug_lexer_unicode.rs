use bhdl_parser::lex;

fn main() {
    let test_strings = vec![
        "10μF",
        "0.5Ω", 
        "value >= 10μF",
        "10uF",  // ASCII version
        "0.5ohm", // ASCII version
        "0.5R",   // Resistance unit
        "2.2kΩ",  // kilo-ohm with Unicode
        "2.2kohm", // kilo-ohm ASCII
    ];
    
    for test in test_strings {
        println!("\n=== Lexing: '{}' ===", test);
        let tokens = lex(test);
        for (i, (token, text)) in tokens.iter().enumerate() {
            println!("  {}: {:?} = '{}'", i, token, text);
        }
    }
}