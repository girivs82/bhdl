use bhdl_parser::parse;

fn main() {
    println!("Testing object literal support in const declarations...");
    
    // Test cases for object literals
    let test_cases = vec![
        "const config: config = { voltage: 3.3V, current: 100mA };",  // Simple object
        "const timing: timing = { setup: 0.5ns, hold: 0.5ns };",       // Timing parameters
        "const specs: specs = { 
            vds_max: 30V, 
            id_max: 5A, 
            package: \"TO-220\" 
        };",                                                            // Multi-line object
        "const nested: nested = { 
            power: { voltage: 5V, current: 1A }, 
            timing: { freq: 1MHz } 
        };",                                                            // Nested objects
        "const empty: empty = {};",                                     // Empty object
        "const mixed: mixed = { 
            count: 4, 
            enabled: true, 
            name: \"test\", 
            range: (3.3V, 5V) 
        };",                                                            // Mixed types including tuple
    ];
    
    for (i, input) in test_cases.iter().enumerate() {
        println!("\n--- Test Case {}: ---", i + 1);
        println!("{}", input);
        
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