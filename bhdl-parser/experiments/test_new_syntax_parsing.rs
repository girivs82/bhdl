use bhdl_parser::parse;
use std::fs;

fn main() {
    println!("Testing new BHDL v2.0 syntax parsing...");
    
    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "test_new_syntax_end_to_end.bhdl".to_string());
    
    // Read the test file
    let content = match fs::read_to_string(&test_file) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file {}: {}", test_file, e);
            return;
        }
    };
    
    println!("Parsing file: {}", test_file);
    
    // Parse the content
    let result = parse(&content);
    
    if result.errors().is_empty() {
        println!("✅ Successfully parsed {} with new advanced syntax!", test_file);
        
        // Print a summary of the syntax tree
        let root = result.syntax();
        print_syntax_summary(&root, 0);
    } else {
        println!("❌ Parse errors found:");
        for (i, error) in result.errors().iter().enumerate() {
            if i < 10 { // Show only first 10 errors
                println!("  {}: {}", i + 1, error.message);
            }
        }
        if result.errors().len() > 10 {
            println!("  ... and {} more errors", result.errors().len() - 10);
        }
    }
}

fn print_syntax_summary(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    match node.kind() {
        bhdl_parser::SyntaxKind::SOURCE_FILE => {
            println!("{}SOURCE_FILE", indent);
            for child in node.children() {
                print_syntax_summary(&child, depth + 1);
            }
        },
        bhdl_parser::SyntaxKind::BOARD_DEF => {
            println!("{}BOARD_DEF", indent);
            for child in node.children() {
                print_syntax_summary(&child, depth + 1);
            }
        },
        bhdl_parser::SyntaxKind::PARAM_DECL => {
            let const_count = count_children_of_type(node, bhdl_parser::SyntaxKind::STRUCT_LITERAL) +
                             count_children_of_type(node, bhdl_parser::SyntaxKind::ARRAY_EXPR) +
                             count_children_of_type(node, bhdl_parser::SyntaxKind::VALUE);
            println!("{}CONST_DECLARATION ({})", indent, 
                if const_count > 1 { "complex" } else { "simple" });
        },
        bhdl_parser::SyntaxKind::STRUCT_LITERAL => {
            let field_count = count_direct_structure_children(node);
            println!("{}OBJECT ({} fields)", indent, field_count);
        },
        bhdl_parser::SyntaxKind::ARRAY_EXPR => {
            let element_count = count_direct_structure_children(node);
            println!("{}TUPLE/ARRAY ({} elements)", indent, element_count);
        },
        bhdl_parser::SyntaxKind::CONNECTION_STMT => {
            println!("{}CONNECTION", indent);
        },
        _ => {
            // For other nodes, recurse without printing
            for child in node.children() {
                print_syntax_summary(&child, depth);
            }
        }
    }
}

fn count_children_of_type(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, kind: bhdl_parser::SyntaxKind) -> usize {
    node.children().filter(|child| child.kind() == kind).count()
}

fn count_direct_structure_children(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> usize {
    node.children().filter(|child| {
        matches!(child.kind(), 
            bhdl_parser::SyntaxKind::STRUCT_LITERAL |
            bhdl_parser::SyntaxKind::ARRAY_EXPR |
            bhdl_parser::SyntaxKind::VALUE |
            bhdl_parser::SyntaxKind::IDENT_REF
        )
    }).count()
}