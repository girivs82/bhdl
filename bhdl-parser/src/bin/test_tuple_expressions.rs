use bhdl_parser::{parse, SyntaxKind};

fn main() {
    println!("Testing tuple/range expression parsing...");
    
    // Test cases for tuple expressions in const declarations
    let test_cases = vec![
        "const voltage_range: voltage_range = (3.5V, 28V);",  // Basic tuple with voltages
        "const specs: specs = (10k, 25%);",                    // Resistance and percentage  
        "const single: voltage = (1.2V);",                     // Single parenthesized expression
        "const triple: specs = (100mA, 2.5W, 5%);",           // Three-element tuple
        "const empty: empty = ();",                            // Empty parentheses
        "const colors: colors = (red, blue, green);",          // Identifiers
    ];
    
    for (i, input) in test_cases.iter().enumerate() {
        println!("\n--- Test Case {}: {} ---", i + 1, input);
        
        // Parse the const declaration
        let result = parse(input);
        
        if result.errors().is_empty() {
            println!("✅ Parsed successfully");
        } else {
            println!("❌ Parse errors:");
            for error in result.errors() {
                println!("  - {}", error.message);
            }
        }
        
        // Print the syntax tree structure (simplified)
        let root = result.syntax();
        print_node_kind(&root, 0);
    }
}

fn print_node_kind(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}{:?}", indent, node.kind());
    
    for child in node.children() {
        print_node_kind(&child, depth + 1);
    }
}