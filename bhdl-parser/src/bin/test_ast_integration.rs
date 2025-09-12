use bhdl_parser::parse;
use bhdl_ast::SourceFile;
use bhdl_analyzer::analyze;
use std::fs;

fn main() {
    println!("Testing AST and analyzer integration with new syntax...");
    
    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "test_working_syntax.bhdl".to_string());
    
    // Read the test file
    let content = match fs::read_to_string(&test_file) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file {}: {}", test_file, e);
            return;
        }
    };
    
    println!("Testing file: {}", test_file);
    println!("\n=== Step 1: Parser ===");
    
    // Parse the content
    let parse_result = parse(&content);
    
    if !parse_result.errors().is_empty() {
        println!("❌ Parser errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    
    println!("✅ Parser: Successfully parsed");
    
    println!("\n=== Step 2: AST ===");
    
    // Create AST
    let source_file = SourceFile::cast(parse_result.syntax().clone());
    match source_file {
        Some(source_file) => {
            println!("✅ AST: Successfully created SourceFile");
            
            // Try to access some AST nodes
            if let Some(board) = source_file.boards().next() {
                println!("  - Found board: {}", board.name().map_or("unnamed".to_string(), |n| n.text().to_string()));
                
                // Count different types of statements
                let mut const_count = 0;
                let mut power_count = 0;
                let mut connection_count = 0;
                
                for stmt in board.statements() {
                    match stmt.syntax().kind() {
                        bhdl_parser::SyntaxKind::PARAM_DECL => const_count += 1,
                        bhdl_parser::SyntaxKind::POWER_DECL => power_count += 1,
                        bhdl_parser::SyntaxKind::CONNECTION_STMT => connection_count += 1,
                        _ => {}
                    }
                }
                
                println!("  - Const declarations: {}", const_count);
                println!("  - Power declarations: {}", power_count);
                println!("  - Connection statements: {}", connection_count);
            }
        }
        None => {
            println!("❌ AST: Failed to create SourceFile");
            return;
        }
    }
    
    println!("\n=== Step 3: Analyzer ===");
    
    // Analyze the AST
    match analyze(&source_file.unwrap()) {
        Ok(analysis_result) => {
            println!("✅ Analyzer: Successfully analyzed");
            println!("  - Diagnostics: {}", analysis_result.diagnostics.len());
            println!("  - Symbol table entries: {}", analysis_result.symbol_table.entries.len());
            
            // Show any diagnostics
            if !analysis_result.diagnostics.is_empty() {
                println!("  - Diagnostics found:");
                for (i, diagnostic) in analysis_result.diagnostics.iter().enumerate() {
                    if i < 5 { // Show first 5 diagnostics
                        println!("    {}: {}", diagnostic.severity, diagnostic.message);
                    }
                }
                if analysis_result.diagnostics.len() > 5 {
                    println!("    ... and {} more", analysis_result.diagnostics.len() - 5);
                }
            }
        }
        Err(e) => {
            println!("❌ Analyzer: Failed with error: {}", e);
            return;
        }
    }
    
    println!("\n🎉 Success: Full pipeline (Parser → AST → Analyzer) works with new syntax!");
}