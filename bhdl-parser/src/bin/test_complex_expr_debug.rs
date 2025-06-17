use bhdl_parser::parse;

fn main() {
    let inputs = vec![
        // Ternary expressions
        r#"board TestBoard {
    power VCC = 5V @ 1A;
    VCC -> Res(debug ? 1k : 10k).1 -> LED.A;
}"#,
        // Mathematical expressions
        r#"board TestBoard {
    clock: Oscillator(base_freq * 2);
}"#,
        // Simple identifier in component param
        r#"board TestBoard {
    divider: Counter(max_count - 1);
}"#,
    ];
    
    for (i, input) in inputs.iter().enumerate() {
        println!("\n=== Test case {} ===", i + 1);
        let result = parse(input);
        
        if !result.errors().is_empty() {
            println!("❌ Parse errors found:");
            for error in result.errors() {
                println!("  {}", error.message);
            }
        } else {
            println!("✅ Parsing successful!");
        }
    }
}