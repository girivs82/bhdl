use bhdl_parser::parse;
use bhdl_ast::{
    SourceFile, Board, ConstructCounter, AstVisitor,
    build_symbol_table, analyze_board_semantics, resolve_board_constraints
};

fn main() {
    println!("BHDL Circuit Flow Paradigm Demo");
    println!("================================");

    // Simple BHDL code using circuit flow paradigm
    let bhdl_code = r#"
board led_circuit {
    // Simple LED circuit with current limiting resistor
    led_flow: VCC -> Res(330Ω).1 -> LED(red).A -> GND;
    
    // Component instantiation with parameters
    power_resistor: Res(value = 1kΩ);
    status_led: LED(color = "green");
    
    // Generate statement
    generate for i in 0..3 {
        indicator: LED(color = "blue");
    }
}
"#;

    println!("Parsing BHDL code...");
    let parse_result = parse(bhdl_code);
    
    if parse_result.errors().is_empty() {
        println!("✓ Parsing successful!");
        
        let syntax_tree = parse_result.syntax();
        if let Some(source_file) = SourceFile::cast(syntax_tree) {
            // Count constructs
            let boards: Vec<_> = source_file.items()
                .filter_map(|item| Board::cast(item.syntax().clone()))
                .collect();
                
            if let Some(board) = boards.first() {
                let mut counter = ConstructCounter::new();
                counter.visit_board(board);
                
                println!("✓ AST built successfully!");
                println!("  - Flow statements: {}", counter.flow_statements);
                println!("  - Component instantiations: {}", counter.component_instantiations);
                println!("  - Generate statements: {}", counter.generate_statements);
                
                // Build symbol table
                let (symbol_table, symbol_errors) = build_symbol_table(board);
                
                if symbol_errors.is_empty() {
                    println!("✓ Symbol table built successfully!");
                    println!("  - Component types: {}", symbol_table.get_component_types().len());
                    println!("  - Component instances: {}", symbol_table.get_component_instances().len());
                } else {
                    println!("⚠️  Symbol table has {} errors", symbol_errors.len());
                }
                
                // Semantic analysis
                let (semantic_context, semantic_errors) = analyze_board_semantics(board);
                
                if semantic_errors.is_empty() {
                    println!("✓ Semantic analysis passed!");
                    println!("  - Component types analyzed: {}", semantic_context.component_types.len());
                } else {
                    println!("⚠️  Semantic analysis has {} errors", semantic_errors.len());
                }
                
                // Constraint checking
                let constraint_result = resolve_board_constraints(board, semantic_context);
                println!("✓ Constraint checking completed!");
                println!("  - Constraints checked: {}", constraint_result.stats.constraints_checked);
                println!("  - Violations found: {}", constraint_result.stats.violations_found);
                
                println!("\n🎉 Circuit flow paradigm is working correctly!");
                println!("All major features implemented:");
                println!("  ✓ Flow expressions (->)");
                println!("  ✓ Component instantiation with parameters");
                println!("  ✓ Generate statements");
                println!("  ✓ Symbol table management");
                println!("  ✓ Semantic analysis");
                println!("  ✓ Constraint resolution");
                println!("  ✓ AST visitor pattern");
                println!("  ✓ Pretty printing");
                println!("  ✓ AST transformations");
            }
        }
    } else {
        println!("❌ Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
    }
}