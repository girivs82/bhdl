// Test AST generation for v2.0 BHDL
use std::fs;
use bhdl_ast::{AstNode, SourceFile, HasName};
use bhdl_ast::v2_board::BoardV2Ext;
use bhdl_ast::v2_statements::{PowerDecl, GroundDecl, ConnectionStmt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing AST Generation for 7805 Circuit ===\n");
    
    let content = fs::read_to_string("test_7805_regulator_realistic.bhdl")?;
    let parse_result = bhdl_parser::parse(&content);
    
    if !parse_result.errors().is_empty() {
        println!("❌ Parse errors found:");
        for err in parse_result.errors() {
            println!("  - {}", err.message);
        }
        return Ok(());
    }
    
    println!("✅ Parse successful, converting to AST...\n");
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax.clone()).ok_or("Failed to cast to SourceFile")?;
    
    // Extract boards
    for board in source_file.boards() {
        println!("Board: PowerSupply_7805"); // TODO: Extract board name when HasName is implemented
        
        // Extract power domains
        let power_decls: Vec<_> = board.power_decls().collect();
        println!("  Power domains: {}", power_decls.len());
        for decl in &power_decls {
            if let Some(name) = decl.name() {
                let spec = decl.spec_text().unwrap_or_else(|| "no spec".to_string());
                println!("    - {} = {}", name.text(), spec);
            }
        }
        
        // Extract ground domains
        let ground_decls: Vec<_> = board.ground_decls().collect();
        println!("  Ground domains: {}", ground_decls.len());
        for decl in &ground_decls {
            if let Some(name) = decl.name() {
                println!("    - {}", name.text());
            }
        }
        
        // Extract connections
        let mut conn_count = 0;
        for conn in board.connections() {
            conn_count += 1;
            if conn_count <= 5 {
                println!("    - {}", conn.text());
            }
        }
        println!("  Connections: {}", conn_count);
        if conn_count > 5 {
            println!("    ... and {} more", conn_count - 5);
        }
        
        // Extract attributes
        let mut attr_count = 0;
        for child in board.syntax().children() {
            if child.kind() == bhdl_parser::SyntaxKind::VALUE {
                attr_count += 1;
            }
        }
        println!("  Attributes: {}", attr_count);
    }
    
    println!("\n✅ AST conversion successful!");
    
    Ok(())
}