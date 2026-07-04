use bhdl_parser::parse;

fn main() {
    let test_code = r#"
entity PWMController(frequency: frequency = 100kHz) {
    pin VCC: power in;
    pin OUT: signal out;
    pin EN: signal in;
    
    parameter duty_cycle: percentage = 50%;
}
"#;

    println!("Testing code:\n{}", test_code);
    
    let parse_result = parse(test_code);
    
    println!("\nParse errors: {}", parse_result.errors().len());
    for (i, error) in parse_result.errors().iter().enumerate() {
        println!("  {}. {}", i+1, error.message);
    }
    
    // Print syntax tree structure
    let syntax = parse_result.syntax();
    println!("\nSyntax tree structure:");
    print_tree(&syntax, 0);
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}{:?}", indent, node.kind());
    
    for child in node.children() {
        print_tree(&child, depth + 1);
    }
}