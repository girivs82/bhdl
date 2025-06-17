// Test the complete BHDL pipeline: Parser -> AST -> Analyzer -> Synthesizer -> Netlist -> Visualizer

use std::fs;
use bhdl_ast::{AstNode, source_file::Item};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL Complete Pipeline Test ===\n");
    
    // Step 1: Parse BHDL file
    println!("Step 1: Parsing BHDL file...");
    let bhdl_content = fs::read_to_string("examples/7805_regulator_v2.bhdl")?;
    let parse_result = bhdl_parser::parse(&bhdl_content);
    
    // Check for parse errors
    let errors = parse_result.errors();
    if !errors.is_empty() {
        eprintln!("❌ Parse errors found:");
        for error in errors {
            eprintln!("  - {}", error.message);
        }
        return Err("Parsing failed".into());
    }
    println!("✅ Parsing successful!");
    
    // Step 2: Convert to AST
    println!("\nStep 2: Converting to AST...");
    let syntax_tree = parse_result.syntax();
    let ast = bhdl_ast::SourceFile::cast(syntax_tree.clone())
        .ok_or("Failed to cast syntax tree to SourceFile")?;
    println!("✅ AST conversion successful!");
    
    // Print AST structure
    println!("\nAST Structure:");
    for item in ast.items() {
        match item {
            Item::BoardDef(board) => {
                println!("  Board: {}", board.name().map(|n| n.text()).unwrap_or("unnamed"));
                
                // Count items in board
                let mut power_count = 0;
                let mut ground_count = 0;
                let mut connection_count = 0;
                
                if let Some(body) = board.body() {
                    for stmt in body.statements() {
                        match stmt {
                            bhdl_ast::Statement::PowerDecl(_) => power_count += 1,
                            bhdl_ast::Statement::GroundDecl(_) => ground_count += 1,
                            bhdl_ast::Statement::ConnectionStmt(_) => connection_count += 1,
                            _ => {}
                        }
                    }
                }
                
                println!("    - {} power domains", power_count);
                println!("    - {} ground domains", ground_count);
                println!("    - {} connections", connection_count);
            }
            _ => println!("  Other item: {:?}", item.syntax().kind()),
        }
    }
    
    // Step 3: Semantic Analysis
    println!("\nStep 3: Running semantic analysis...");
    let mut analyzer = bhdl_analyzer::Analyzer::new();
    let analysis_result = analyzer.analyze(&ast);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("⚠️  Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  - {:?}: {}", diag.severity, diag.message);
        }
    } else {
        println!("✅ Semantic analysis completed without issues!");
    }
    
    // Print symbol table summary
    println!("\nSymbol Table Summary:");
    println!("  - {} symbols defined", analysis_result.symbol_table.len());
    
    // Step 4: Synthesis (Component Mapping)
    println!("\nStep 4: Synthesizing components...");
    // TODO: Implement synthesis step once bhdl-synthesizer is ready
    println!("⏭️  Synthesis step not yet implemented");
    
    // Step 5: Generate Netlist
    println!("\nStep 5: Generating netlist...");
    // TODO: Implement netlist generation from AST/analysis
    println!("⏭️  Netlist generation not yet implemented");
    
    // Step 6: Visualization
    println!("\nStep 6: Creating visualization...");
    // TODO: Implement visualization from netlist
    println!("⏭️  Visualization not yet implemented");
    
    println!("\n🎉 Pipeline test completed!");
    println!("\nNext steps:");
    println!("  1. Implement AST to Netlist conversion");
    println!("  2. Add component synthesis/mapping");
    println!("  3. Create SVG visualization from netlist");
    
    Ok(())
}