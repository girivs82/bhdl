use bhdl_parser::parse;

fn main() {
    println!("Flow-Based Intent Parsing Test\n");
    
    // Test current syntax
    println!("=== Current (with 'net' keyword) ===");
    let current = "board Test { net critical: VCC -> R(1k).1 for delay(3ms); }";
    let result = parse(current);
    println!("Parses: {}", result.errors().is_empty());
    
    // Test desired syntax
    println!("\n=== Desired (without 'net' keyword) ===");
    let tests = vec![
        ("Named flow", "board Test { critical: VCC -> R(1k).1 for delay(3ms); }"),
        ("Direct flow", "board Test { VCC -> R(1k).1 for delay(3ms); }"),
    ];
    
    for (name, code) in tests {
        let result = parse(code);
        println!("{}: {}", name, if result.errors().is_empty() { "✓" } else { "✗" });
    }
}