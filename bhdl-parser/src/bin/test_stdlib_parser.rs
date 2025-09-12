// Test parser with stdlib files

use std::fs;
use std::env;
use bhdl_parser::parse;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let file_path = if args.len() > 1 {
        args[1].clone()
    } else {
        println!("Usage: {} <file.bhdl>", args[0]);
        return Ok(());
    };

    println!("=== Testing Parser with {} ===\n", file_path);

    // Read the file
    let bhdl_content = fs::read_to_string(&file_path)?;
    
    println!("File size: {} bytes\n", bhdl_content.len());
    
    // Parse the file
    let parse_result = parse(&bhdl_content);
    
    // Check for errors
    let errors = parse_result.errors();
    if errors.is_empty() {
        println!("✅ SUCCESS: Parser handled all constructs!");
        println!("\nParsed syntax tree structure:");
        
        // Print a summary of the parsed structure
        let syntax = parse_result.syntax();
        print_tree_summary(&syntax, 0);
        
        // Count specific constructs
        println!("\n=== Construct Summary ===");
        let content_str = bhdl_content.as_str();
        
        if content_str.contains("import") {
            println!("✅ Import statements detected");
        }
        if content_str.contains("module") {
            println!("✅ Module definitions detected");
        }
        if content_str.contains("const") {
            println!("✅ Const declarations detected");
        }
        if content_str.contains("when") {
            println!("✅ Conditional pins (when) detected");
        }
        if content_str.contains("alias") {
            println!("✅ Alias declarations detected");
        }
        if content_str.contains("@metadata") {
            println!("✅ Metadata annotations detected");
        }
        if content_str.contains("?") && content_str.contains(":") {
            println!("✅ Ternary operators detected");
        }
        if content_str.contains("||") {
            println!("✅ Logical OR operators detected");
        }
        if content_str.contains("&&") {
            println!("✅ Logical AND operators detected");
        }
        
    } else {
        println!("❌ FAILED: Parser found {} errors\n", errors.len());
        
        // Show first 10 errors
        for (i, error) in errors.iter().take(10).enumerate() {
            println!("Error {}: {}", i + 1, error.message);
        }
        
        if errors.len() > 10 {
            println!("\n... and {} more errors", errors.len() - 10);
        }
    }
    
    Ok(())
}

fn print_tree_summary(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    if depth > 3 { return; } // Limit depth
    
    let indent = "  ".repeat(depth);
    let kind = node.kind();
    
    // Only show important node types
    use bhdl_parser::SyntaxKind;
    match kind {
        SyntaxKind::SOURCE_FILE |
        SyntaxKind::IMPORT_STMT |
        SyntaxKind::MODULE_DEF |
        SyntaxKind::PARAM_DECL |
        SyntaxKind::ALIAS |
        SyntaxKind::PIN_DECL |
        SyntaxKind::ATTRIBUTE_DECL => {
            println!("{}{:?}", indent, kind);
        }
        _ => {}
    }
    
    // Show children
    for child in node.children() {
        print_tree_summary(&child, depth + 1);
    }
}