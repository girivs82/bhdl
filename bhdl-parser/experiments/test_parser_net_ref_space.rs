use bhdl_parser::parse;

fn main() {
    let input = std::fs::read_to_string("tests/test_parser_net_ref_space.bhdl")
        .expect("Failed to read test file");
    
    println!("=== Testing Parser with 'VIN @RAW->' Pattern ===\n");
    println!("Input:\n{}", input);
    
    let parse_result = parse(&input);
    
    // Print syntax tree
    println!("\nSyntax tree:");
    let syntax = parse_result.syntax();
    println!("{:#?}", syntax);
    
    // Print errors
    if !parse_result.errors().is_empty() {
        println!("\nParse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
    } else {
        println!("\n✅ No parse errors!");
    }
}