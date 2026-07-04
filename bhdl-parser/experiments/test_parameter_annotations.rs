use bhdl_parser::parse;

fn main() {
    println!("Testing parameter type annotations with defaults...");
    
    // Test cases for parameter type annotations
    let test_cases = vec![
        "entity Resistor(value: resistance) { pin 1: signal inout; pin 2: signal inout; }",  // Type annotation without default
        "entity Resistor(value: resistance = 10k) { pin 1: signal inout; pin 2: signal inout; }",  // Type annotation with default
        "entity LinearRegulator(
            input_voltage: voltage = 12V,
            output_voltage: voltage = 5V,
            current_rating: current = 1A,
            package: string = \"TO-220\"
        ) { 
            pin IN: power in; 
            pin OUT: power out; 
            pin GND: ground inout; 
        }",  // Multiple typed parameters with defaults
        "entity OpAmp(
            supply_voltage: voltage,
            gain: number = 1.0,
            bandwidth: frequency = 1MHz
        ) { 
            pin VIN+: signal in; 
            pin VIN-: signal in; 
            pin VOUT: signal out; 
        }",  // Mixed parameters (some with defaults, some without)
        "entity Buffer() { pin IN: signal in; pin OUT: signal out; }",  // Empty parameters
    ];
    
    for (i, input) in test_cases.iter().enumerate() {
        println!("\n--- Test Case {}: ---", i + 1);
        // Show a shortened version for readability
        let display_input = if input.len() > 100 {
            format!("{}...", &input[..100])
        } else {
            input.to_string()
        };
        println!("{}", display_input);
        
        // Parse the module declaration
        let result = parse(input);
        
        if result.errors().is_empty() {
            println!("✅ Parsed successfully");
        } else {
            println!("❌ Parse errors:");
            for error in result.errors() {
                println!("  - {}", error.message);
            }
        }
        
        // Print the syntax tree structure (focused on parameters)
        let root = result.syntax();
        print_parameter_nodes(&root, 0);
    }
}

fn print_parameter_nodes(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    match node.kind() {
        bhdl_parser::SyntaxKind::PARAM_LIST => {
            println!("{}PARAM_LIST", indent);
            for child in node.children() {
                print_parameter_nodes(&child, depth + 1);
            }
        },
        bhdl_parser::SyntaxKind::PARAM_DECL => {
            println!("{}PARAM_DECL", indent);
            for child in node.children() {
                print_parameter_nodes(&child, depth + 1);
            }
        },
        bhdl_parser::SyntaxKind::TYPE_REF => {
            println!("{}TYPE_REF", indent);
        },
        bhdl_parser::SyntaxKind::VALUE => {
            println!("{}VALUE", indent);
        },
        bhdl_parser::SyntaxKind::IDENT_REF => {
            println!("{}IDENT_REF", indent);
        },
        _ => {
            // For other nodes, just show them and recurse
            if node.children().count() > 0 {
                for child in node.children() {
                    print_parameter_nodes(&child, depth);
                }
            }
        }
    }
}