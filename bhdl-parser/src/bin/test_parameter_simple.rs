use bhdl_parser::parse;

fn main() {
    println!("Testing parameter type annotations - simplified...");
    
    // Simple test case for mixed parameters
    let input = "module OpAmp(
        supply_voltage: voltage,
        gain: number = 1.0,
        bandwidth: frequency = 1MHz
    ) { 
        pin VIN_POS: signal in; 
        pin VIN_NEG: signal in; 
        pin VOUT: signal out; 
    }";
    
    println!("Input: {}", input);
    
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
        _ => {
            // For other nodes, just recurse
            for child in node.children() {
                print_parameter_nodes(&child, depth);
            }
        }
    }
}