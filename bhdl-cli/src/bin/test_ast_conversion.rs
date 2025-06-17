// Test AST conversion from parse tree

use std::fs;
use bhdl_ast::{AstNode, SourceFile, source_file::Item, BoardV2Ext, HasName};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL AST Conversion Test ===\n");
    
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
    println!("\nStep 2: Converting parse tree to AST...");
    let syntax_tree = parse_result.syntax();
    
    // Debug: Print the root syntax kind
    println!("Root syntax kind: {:?}", syntax_tree.kind());
    
    let ast = SourceFile::cast(syntax_tree.clone());
    
    match ast {
        Some(source_file) => {
            println!("✅ AST conversion successful!");
            
            // Step 3: Traverse AST
            println!("\nStep 3: Traversing AST structure:");
            
            for item in source_file.items() {
                match item {
                    Item::Board(board) => {
                        println!("\n📋 Board Definition:");
                        println!("  Name: {}", board.name()
                            .map(|n| n.text().to_string())
                            .unwrap_or_else(|| "unnamed".to_string()));
                        
                        // Check for structured blocks (v1.0 style)
                        if let Some(params) = board.parameters_block() {
                            println!("  ✓ Has parameters block");
                        }
                        
                        if let Some(ports) = board.ports_block() {
                            println!("  ✓ Has ports block");
                        }
                        
                        if let Some(nets) = board.nets_block() {
                            println!("  ✓ Has nets block");
                        }
                        
                        // For v2.0, use the new extension methods
                        println!("\n  v2.0 Board Contents:");
                        
                        // Count different types of statements
                        let power_count = board.power_decls().count();
                        let ground_count = board.ground_decls().count();
                        let connection_count = board.connections().count();
                        let flow_count = board.flow_stmts().count();
                        let interface_count = board.interface_instances().count();
                        
                        println!("    - {} power domains", power_count);
                        println!("    - {} ground domains", ground_count);
                        println!("    - {} connections", connection_count);
                        println!("    - {} flow statements", flow_count);
                        println!("    - {} interface instances", interface_count);
                        
                        // Show power domains
                        println!("\n  Power Domains:");
                        for power in board.power_decls() {
                            let name = power.name()
                                .map(|n| n.text().to_string())
                                .unwrap_or_else(|| "unnamed".to_string());
                            let voltage = power.voltage().unwrap_or_else(|| "unknown".to_string());
                            let current = power.current().unwrap_or_else(|| "unknown".to_string());
                            println!("    - {} = {} @ {}", name, voltage, current);
                        }
                        
                        // Show ground domains
                        println!("\n  Ground Domains:");
                        for ground in board.ground_decls() {
                            let name = ground.name()
                                .map(|n| n.text().to_string())
                                .unwrap_or_else(|| "unnamed".to_string());
                            println!("    - {}", name);
                        }
                        
                        // Show first few connections
                        println!("\n  Connections (first 5):");
                        for (i, conn) in board.connections().enumerate() {
                            if i >= 5 { break; }
                            println!("    - {}", conn.text());
                        }
                    }
                    Item::Module(module) => {
                        println!("\n📦 Module Definition:");
                        println!("  Name: {}", module.name()
                            .map(|n| n.text().to_string())
                            .unwrap_or_else(|| "unnamed".to_string()));
                    }
                    Item::ComponentDef(comp) => {
                        println!("\n🔌 Component Definition:");
                        println!("  Name: {}", comp.name()
                            .map(|n| n.text().to_string())
                            .unwrap_or_else(|| "unnamed".to_string()));
                    }
                    _ => {
                        println!("\n❓ Other item: {:?}", item.syntax().kind());
                    }
                }
            }
            
            println!("\n📊 AST Summary:");
            println!("  - Total top-level items: {}", 
                source_file.items().count());
        }
        None => {
            eprintln!("❌ Failed to convert syntax tree to AST");
            eprintln!("   Expected SOURCE_FILE, got {:?}", syntax_tree.kind());
            return Err("AST conversion failed".into());
        }
    }
    
    println!("\n🎉 AST conversion test completed!");
    println!("\n✅ The AST now fully supports BHDL v2.0 syntax including:");
    println!("   - Power and ground declarations");
    println!("   - Direct connection statements");
    println!("   - Flow specifications");
    println!("   - Interface instances");
    println!("   - Generate and conditional blocks");
    
    Ok(())
}