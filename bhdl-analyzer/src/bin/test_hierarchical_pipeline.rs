use std::fs;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, HasName, ModuleInst};
use bhdl_analyzer::analyze;

fn main() {
    println!("=== Testing Hierarchical Module Pipeline ===\n");
    
    // Read the test file
    let test_file = "tests/circuits/hierarchical/simple_hierarchy.bhdl";
    let code = fs::read_to_string(test_file)
        .expect("Failed to read test file");
    
    println!("1. Parsing hierarchical module design...");
    let parse_result = parse(&code);
    
    if !parse_result.errors().is_empty() {
        println!("Parse errors found:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    println!("✓ Parsing successful");
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Count modules and instances
    let mut module_count = 0;
    let mut instance_count = 0;
    
    for item in source_file.items() {
        match item {
            bhdl_ast::source_file::Item::Module(module) => {
                module_count += 1;
                println!("\n  Module: {}", module.name().map(|t| t.text().to_string()).unwrap_or_default());
                
                // Count module instances within this module
                for inst in module.module_instances() {
                    instance_count += 1;
                    println!("    - Instance: {} of type {}", 
                        inst.name().map(|t| t.text().to_string()).unwrap_or_default(),
                        inst.module_type().map(|t| t.text().to_string()).unwrap_or_default()
                    );
                }
            }
            bhdl_ast::source_file::Item::Board(board) => {
                println!("\n  Board: {}", board.name().map(|t| t.text().to_string()).unwrap_or_default());
                
                // Count module instances within the board
                for inst in board.module_instances() {
                    instance_count += 1;
                    println!("    - Instance: {} of type {}", 
                        inst.name().map(|t| t.text().to_string()).unwrap_or_default(),
                        inst.module_type().map(|t| t.text().to_string()).unwrap_or_default()
                    );
                }
            }
            _ => {}
        }
    }
    
    println!("\n  Total modules defined: {}", module_count);
    println!("  Total module instances: {}", instance_count);
    
    println!("\n2. Running semantic analysis...");
    let analysis_result = analyze(&source_file);
    
    println!("  Diagnostics: {}", analysis_result.diagnostics.len());
    for diag in &analysis_result.diagnostics {
        println!("    - {}", diag.message);
    }
    
    println!("\n3. Symbol table analysis:");
    println!("  Global symbols: {}", count_symbols(&analysis_result.global_scope));
    println!("  Definition scopes: {}", analysis_result.definition_scopes.len());
    
    // Test hierarchical symbol resolution
    println!("\n4. Testing hierarchical symbol resolution:");
    let mut hier_table = bhdl_analyzer::hierarchical_symbol_table::HierarchicalSymbolTable::new(
        analysis_result.global_scope.clone(),
        analysis_result.definition_scopes.clone()
    );
    
    // Register module instances (this would normally be done during analysis)
    // For now, just test the infrastructure
    
    let test_paths = vec![
        "SimplePWM",
        "SimpleRegulator", 
        "SimpleBoard",
        "pwm1",
        "reg1",
        "VDD",
    ];
    
    for path_str in test_paths {
        let path = bhdl_analyzer::hierarchical_symbol_table::SymbolPath::from_str(path_str);
        let result = hier_table.resolve_path(&path, None);
        match result {
            Some(symbol) => {
                println!("  ✓ {} -> Found (kind: {:?})", path_str, symbol.kind);
            }
            None => {
                println!("  ✗ {} -> Not found", path_str);
            }
        }
    }
    
    println!("\n=== Test Complete ===");
}

fn count_symbols(table: &bhdl_analyzer::symbol_table::SymbolTable) -> usize {
    table.iter().count()
}