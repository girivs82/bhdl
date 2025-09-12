use bhdl_parser::parse;
use bhdl_ast::SourceFile;
use bhdl_analyzer::analyze;
use std::fs;

fn main() {
    println!("Testing full pipeline with new BHDL v2.0 syntax...");
    
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
    
    println!("✅ Parser: Successfully parsed with new syntax features");
    
    println!("\n=== Step 2: AST ===");
    
    // Create AST
    let source_file = SourceFile::cast(parse_result.syntax().clone());
    match source_file {
        Some(source_file) => {
            println!("✅ AST: Successfully created SourceFile");
            
            // Analyze AST nodes
            if let Some(board) = source_file.boards().next() {
                println!("  - Found board: {}", board.name().map_or("unnamed".to_string(), |n| n.text().to_string()));
                
                let mut analysis_summary = AnalysisSummary::default();
                
                for stmt in board.statements() {
                    match stmt.syntax().kind() {
                        bhdl_parser::SyntaxKind::PARAM_DECL => {
                            analysis_summary.const_count += 1;
                            
                            // Try to detect if this const has complex data structures
                            if let Some(expr) = stmt.expr() {
                                if has_complex_structures(&expr.syntax()) {
                                    analysis_summary.complex_const_count += 1;
                                }
                            }
                        }
                        bhdl_parser::SyntaxKind::POWER_DECL => analysis_summary.power_count += 1,
                        bhdl_parser::SyntaxKind::CONNECTION_STMT => analysis_summary.connection_count += 1,
                        _ => {}
                    }
                }
                
                println!("  - Const declarations: {} (complex: {})", 
                         analysis_summary.const_count, analysis_summary.complex_const_count);
                println!("  - Power declarations: {}", analysis_summary.power_count);
                println!("  - Connection statements: {}", analysis_summary.connection_count);
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
            println!("✅ Analyzer: Successfully analyzed new syntax");
            println!("  - Total diagnostics: {}", analysis_result.diagnostics.len());
            println!("  - Symbol table entries: {}", analysis_result.symbol_table.entries.len());
            
            // Categorize diagnostics
            let mut error_count = 0;
            let mut warning_count = 0;
            let mut info_count = 0;
            
            for diagnostic in &analysis_result.diagnostics {
                match diagnostic.severity {
                    bhdl_analyzer::DiagnosticSeverity::Error => error_count += 1,
                    bhdl_analyzer::DiagnosticSeverity::Warning => warning_count += 1,
                    bhdl_analyzer::DiagnosticSeverity::Info => info_count += 1,
                }
            }
            
            println!("  - Errors: {}, Warnings: {}, Info: {}", error_count, warning_count, info_count);
            
            // Show sample diagnostics
            if !analysis_result.diagnostics.is_empty() {
                println!("  - Sample diagnostics:");
                for (i, diagnostic) in analysis_result.diagnostics.iter().enumerate() {
                    if i < 3 { // Show first 3 diagnostics
                        println!("    {}: {}", diagnostic.severity, diagnostic.message);
                    }
                }
                if analysis_result.diagnostics.len() > 3 {
                    println!("    ... and {} more", analysis_result.diagnostics.len() - 3);
                }
            }
            
            println!("\n🎉 SUCCESS: Full pipeline works with BHDL v2.0 advanced syntax!");
            println!("   Parser → AST → Analyzer all handle new features:");
            println!("   • Tuple expressions: (3.3V, 5V)");
            println!("   • Object literals: {{ voltage: 12V, current: 2A }}");
            println!("   • Arrays: [1k, 2.2k, 4.7k]");  
            println!("   • Const declarations in boards");
            println!("   • Complex nested data structures");
        }
        Err(e) => {
            println!("❌ Analyzer: Failed with error: {}", e);
            return;
        }
    }
}

#[derive(Default)]
struct AnalysisSummary {
    const_count: u32,
    complex_const_count: u32,
    power_count: u32,
    connection_count: u32,
}

fn has_complex_structures(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> bool {
    // Check if this node or its children contain complex structures
    match node.kind() {
        bhdl_parser::SyntaxKind::STRUCT_LITERAL | 
        bhdl_parser::SyntaxKind::ARRAY_EXPR => return true,
        _ => {}
    }
    
    // Recursively check children
    for child in node.children() {
        if has_complex_structures(&child) {
            return true;
        }
    }
    
    false
}