use bhdl_parser::{parse, SyntaxKind};
use std::fs;

fn main() {
    // Read the test file
    let content = fs::read_to_string("test_module_parser.bhdl")
        .expect("Failed to read test file");
    
    println!("Parsing BHDL v2.0 module definitions:\n{}\n", content);
    
    // Parse the content
    let parsed = parse(&content);
    
    // Print diagnostics
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        println!();
    } else {
        println!("✓ No parse errors\n");
    }
    
    // Print the parse tree
    println!("Parse tree:");
    let syntax_tree = parsed.syntax();
    print_tree(&syntax_tree, 0);
    
    // Walk the tree to find module definitions
    println!("\n\nModule definitions found:");
    find_modules(&syntax_tree);
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}{:?} {:?}", indent, node.kind(), node.text());
    
    for child in node.children() {
        print_tree(&child, depth + 1);
    }
}

fn find_modules(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) {
    // Find all MODULE_DEF nodes
    for child in node.children() {
        if child.kind() == SyntaxKind::MODULE_DEF {
            print_module_info(&child);
        } else {
            find_modules(&child);
        }
    }
}

fn print_module_info(module_node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) {
    // Get module name
    let mut name = None;
    for token in module_node.children_with_tokens() {
        if let Some(t) = token.as_token() {
            if t.kind() == SyntaxKind::IDENT && name.is_none() {
                name = Some(t.text().to_string());
            }
        }
    }
    
    println!("\n- Module: {}", name.unwrap_or_else(|| "<unnamed>".to_string()));
    
    // Check for parameters
    let module_text = module_node.text().to_string();
    if let Some(params_text) = module_text
        .split('(')
        .nth(1)
        .and_then(|s| s.split(')').next()) 
    {
        println!("  Parameters: ({})", params_text);
    }
    
    // Look for PORT_DECL nodes (v2.0 pin declarations)
    for child in module_node.children() {
        if child.kind() == SyntaxKind::PORT_DECL {
            let text = child.text().to_string();
            println!("  Pin: {}", text.trim());
        }
    }
    
    // Look for metadata (attributes)
    for token in module_node.children_with_tokens() {
        if let Some(t) = token.as_token() {
            if t.kind() == SyntaxKind::AT {
                // Found an @ attribute
                let remaining = module_node.text().to_string();
                if let Some(attr_start) = remaining.find('@') {
                    if let Some(line_end) = remaining[attr_start..].find(';') {
                        let attr_line = &remaining[attr_start..attr_start + line_end + 1];
                        println!("  Metadata: {}", attr_line);
                    }
                }
            }
        }
    }
}