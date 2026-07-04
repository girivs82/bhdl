// Test parser specifically for the 7805 regulator circuit
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing 7805 Regulator Circuit Parser ===\n");

    let content = fs::read_to_string("test_7805_regulator_realistic.bhdl")?;
    let result = bhdl_parser::parse(&content);
    
    let errors = result.errors();
    println!("Parse errors: {}", errors.len());
    
    if !errors.is_empty() {
        println!("\nErrors:");
        for (i, err) in errors.iter().enumerate() {
            println!("{:3}. {}", i + 1, err.message);
        }
        
        // Debug: show first few lines that might be problematic
        println!("\nFirst few connection lines:");
        for (i, line) in content.lines().enumerate() {
            if line.contains("->") {
                println!("  Line {}: {}", i + 1, line);
                if i > 15 { break; } // Show first few connections
            }
        }
    } else {
        println!("✅ Parse successful!");
        let root = result.syntax();
        print_node_summary(&root, 0, 3);
    }
    
    Ok(())
}

fn print_node_summary(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }
    
    let indent = "  ".repeat(depth);
    let kind = node.kind();
    
    // For leaf nodes or specific nodes, show a preview
    let preview = if node.children().count() == 0 || depth == max_depth {
        let text = node.text().to_string();
        if text.len() > 40 {
            format!(" => \"{}...\"", &text[..40].replace('\n', " "))
        } else {
            format!(" => \"{}\"", text.replace('\n', " "))
        }
    } else {
        String::new()
    };
    
    println!("{}{:?}{}", indent, kind, preview);
    
    for child in node.children() {
        print_node_summary(&child, depth + 1, max_depth);
    }
}