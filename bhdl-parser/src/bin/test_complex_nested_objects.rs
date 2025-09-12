use bhdl_parser::parse;

fn main() {
    println!("Testing complex nested object structures...");
    
    // Test cases for complex nested objects
    let test_cases = vec![
        // Simple nested object (already working)
        "const simple: simple = { 
            power: { voltage: 5V, current: 1A }
        };",
        
        // Deeply nested objects
        "const deep: deep = {
            level1: {
                level2: {
                    level3: {
                        value: 42
                    }
                }
            }
        };",
        
        // Mixed nesting with arrays and objects
        "const mixed: mixed = {
            components: [
                { type: \"resistor\", value: 10k },
                { type: \"capacitor\", value: 100µF }
            ],
            specs: {
                voltage_range: (3.3V, 5V),
                temperature: (-40C, 85C)
            }
        };",
        
        // Arrays of objects
        "const array_objects: array_objects = [
            { name: \"component1\", value: 1k },
            { name: \"component2\", value: 2k },
            { name: \"component3\", value: 3k }
        ];",
        
        // Objects with array fields
        "const object_arrays: object_arrays = {
            resistor_values: [1k, 2.2k, 4.7k, 10k],
            capacitor_values: [10pF, 100pF, 1µF],
            test_conditions: {
                temperatures: [-40C, 25C, 85C],
                voltages: [3.3V, 5V, 12V]
            }
        };",
        
        // Complex realistic configuration
        "const power_supply_config: power_supply_config = {
            input: {
                voltage: { min: 9V, max: 36V },
                current: 5A,
                protection: {
                    overvoltage: 40V,
                    overcurrent: 6A,
                    fuse_rating: 7A
                }
            },
            output: {
                rails: [
                    { voltage: 5V, current: 3A, ripple: 50mV },
                    { voltage: 3.3V, current: 2A, ripple: 30mV }
                ],
                regulation: { load: 0.5pct, line: 0.1pct }
            }
        };",
    ];
    
    for (i, input) in test_cases.iter().enumerate() {
        println!("\n--- Test Case {}: ---", i + 1);
        // Show first 100 chars for readability
        let display_input = if input.len() > 100 {
            format!("{}...", &input[..100])
        } else {
            input.to_string()
        };
        println!("{}", display_input);
        
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
        
        // Print a simple syntax tree summary
        let root = result.syntax();
        print_structure_summary(&root, 0);
    }
}

fn print_structure_summary(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    match node.kind() {
        bhdl_parser::SyntaxKind::STRUCT_LITERAL => {
            let field_count = count_direct_children(node, bhdl_parser::SyntaxKind::STRUCT_LITERAL) + 
                             count_direct_children(node, bhdl_parser::SyntaxKind::ARRAY_EXPR) +
                             count_direct_children(node, bhdl_parser::SyntaxKind::VALUE) +
                             count_direct_children(node, bhdl_parser::SyntaxKind::IDENT_REF);
            println!("{}OBJECT ({} fields)", indent, field_count);
            for child in node.children() {
                print_structure_summary(&child, depth + 1);
            }
        },
        bhdl_parser::SyntaxKind::ARRAY_EXPR => {
            let element_count = count_direct_children(node, bhdl_parser::SyntaxKind::STRUCT_LITERAL) + 
                               count_direct_children(node, bhdl_parser::SyntaxKind::ARRAY_EXPR) +
                               count_direct_children(node, bhdl_parser::SyntaxKind::VALUE) +
                               count_direct_children(node, bhdl_parser::SyntaxKind::IDENT_REF);
            println!("{}ARRAY ({} elements)", indent, element_count);
            for child in node.children() {
                print_structure_summary(&child, depth + 1);
            }
        },
        bhdl_parser::SyntaxKind::VALUE => {
            println!("{}VALUE", indent);
        },
        bhdl_parser::SyntaxKind::IDENT_REF => {
            println!("{}IDENT", indent);
        },
        _ => {
            // For other nodes, just recurse without printing
            for child in node.children() {
                print_structure_summary(&child, depth);
            }
        }
    }
}

fn count_direct_children(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, kind: bhdl_parser::SyntaxKind) -> usize {
    node.children().filter(|child| child.kind() == kind).count()
}