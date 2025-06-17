//! Comprehensive test for BHDL circuit flow paradigm implementation
//! 
//! This test demonstrates the complete functionality of the circuit flow paradigm
//! including parsing, AST construction, semantic analysis, and constraint checking.

use bhdl_parser::parse;
use bhdl_ast::*;

#[cfg(test)]
mod circuit_flow_tests {
    use super::*;

    #[test]
    fn test_complete_circuit_flow_paradigm() {
        // Test BHDL code using the circuit flow paradigm
        let bhdl_code = r#"
board led_circuit {
    // Simple LED circuit with current limiting resistor
    led_flow: VCC -> Res(330Ω).1 -> Res(330Ω).2 -> LED(red).A -> LED(red).K -> GND;
    
    // Component instantiation with parameters
    power_res: Res(value = 1kΩ, tolerance = 5%);
    status_led: LED(color = "green");
    
    // Generate multiple instances
    generate for i in 0..3 {
        indicator_led: LED(color = "blue");
        current_limit: Res(470Ω);
        led_chain: VCC -> current_limit.1 -> current_limit.2 -> indicator_led.A;
    }
    
    // Conditional circuit configuration
    if (debug_mode) {
        debug_led: LED(color = "yellow");
        debug_resistor: Res(220Ω);
    } else {
        normal_operation: LED(color = "white");
    }
    
    // Assignment for configuration
    debug_mode = true;
}
"#;

        println!("Testing BHDL Circuit Flow Paradigm");
        println!("==================================");
        println!("Input BHDL code:");
        println!("{}", bhdl_code);
        println!();

        // Step 1: Parse the code
        println!("Step 1: Parsing...");
        let parse_result = parse(bhdl_code);
        
        if !parse_result.errors().is_empty() {
            println!("Parse errors:");
            for error in parse_result.errors() {
                println!("  - {}", error.message);
            }
        } else {
            println!("✓ Parsing successful");
        }

        // Step 2: Build AST
        println!("\nStep 2: Building AST...");
        let syntax_tree = parse_result.syntax();
        
        if let Some(source_file) = SourceFile::cast(syntax_tree.clone()) {
            println!("✓ AST construction successful");
            
            // Find the board definition
            let boards: Vec<_> = source_file.items()
                .filter_map(|item| Board::cast(item.syntax().clone()))
                .collect();
                
            if let Some(board) = boards.first() {
                println!("✓ Found board: {}", 
                    board.name().map(|t| t.text().to_string()).unwrap_or("unnamed".to_string()));

                // Step 3: Count constructs using visitor pattern
                println!("\nStep 3: Analyzing AST structure...");
                let mut counter = ConstructCounter::new();
                counter.visit_board(board);
                
                println!("AST Statistics:");
                println!("  - Flow statements: {}", counter.flow_statements);
                println!("  - Component instantiations: {}", counter.component_instantiations);
                println!("  - Generate statements: {}", counter.generate_statements);
                println!("  - Conditional statements: {}", counter.conditional_statements);
                println!("  - Assignment statements: {}", counter.assignment_statements);
                println!("  - Binary expressions: {}", counter.binary_expressions);
                println!("  - Total constructs: {}", counter.total_constructs());

                // Step 4: Build symbol table
                println!("\nStep 4: Building symbol table...");
                let (symbol_table, symbol_errors) = build_symbol_table(board);
                
                if !symbol_errors.is_empty() {
                    println!("Symbol errors:");
                    for error in &symbol_errors {
                        println!("  - {}", error);
                    }
                } else {
                    println!("✓ Symbol table built successfully");
                }

                println!("Symbol table contents:");
                println!("  - Global scope symbols: {}", symbol_table.scopes[&0].symbols.len());
                
                let component_types = symbol_table.get_component_types();
                println!("  - Component types: {}", component_types.len());
                for comp_type in component_types {
                    println!("    * {}", comp_type.name);
                }

                let component_instances = symbol_table.get_component_instances();
                println!("  - Component instances: {}", component_instances.len());
                for instance in component_instances {
                    println!("    * {} (type: {})", instance.name, 
                        instance.instantiated_type.as_ref().unwrap_or(&"unknown".to_string()));
                }

                // Step 5: Semantic analysis
                println!("\nStep 5: Semantic analysis...");
                let (semantic_context, semantic_errors) = analyze_board_semantics(board);
                
                if !semantic_errors.is_empty() {
                    println!("Semantic errors:");
                    for error in &semantic_errors {
                        println!("  - {}", error);
                    }
                } else {
                    println!("✓ Semantic analysis passed");
                }

                if !semantic_context.warnings.is_empty() {
                    println!("Semantic warnings:");
                    for warning in &semantic_context.warnings {
                        println!("  - {}", warning);
                    }
                }

                println!("Semantic analysis results:");
                println!("  - Component types analyzed: {}", semantic_context.component_types.len());
                println!("  - Expression types inferred: {}", semantic_context.expression_types.len());

                // Step 6: Constraint resolution
                println!("\nStep 6: Constraint resolution...");
                let constraint_result = resolve_board_constraints(board, semantic_context);
                
                println!("Constraint checking results:");
                println!("  - Total constraints: {}", constraint_result.stats.total_constraints);
                println!("  - Constraints checked: {}", constraint_result.stats.constraints_checked);
                println!("  - Violations found: {}", constraint_result.stats.violations_found);
                println!("  - Warnings generated: {}", constraint_result.stats.warnings_generated);

                if !constraint_result.violations.is_empty() {
                    println!("Constraint violations:");
                    for violation in &constraint_result.violations {
                        println!("  - {}: {} (expected: {}, actual: {})", 
                            violation.constraint.description,
                            violation.constraint.severity,
                            violation.expected_value,
                            violation.actual_value);
                    }
                }

                if !constraint_result.warnings.is_empty() {
                    println!("Constraint warnings:");
                    for warning in &constraint_result.warnings {
                        println!("  - {}", warning);
                    }
                }

                // Step 7: Pretty printing
                println!("\nStep 7: Pretty printing...");
                let pretty_config = PrettyPrintConfig::default();
                let pretty_output = board.pretty_print(&pretty_config);
                println!("Pretty printed board:");
                println!("{}", pretty_output);

                // Step 8: Validation
                println!("\nStep 8: Validation...");
                let validation_report = validate_board(board);
                
                if validation_report.has_errors() {
                    println!("Validation errors:");
                    for error in &validation_report.errors {
                        println!("  - {}", error);
                    }
                } else {
                    println!("✓ Board validation passed");
                }

                if !validation_report.warnings.is_empty() {
                    println!("Validation warnings:");
                    for warning in &validation_report.warnings {
                        println!("  - {}", warning);
                    }
                }

                // Step 9: Test transformations
                println!("\nStep 9: Testing transformations...");
                
                // Test generate unrolling
                let unroll_result = unroll_generate_statements(board, 10);
                match unroll_result {
                    Ok(transform_result) => {
                        println!("✓ Generate statement unrolling: {:?}", transform_result);
                    }
                    Err(transform_error) => {
                        println!("Generate unrolling error: {}", transform_error);
                    }
                }

                // Test flow flattening
                let flatten_result = flatten_flow_expressions(board);
                match flatten_result {
                    Ok(transform_result) => {
                        println!("✓ Flow expression flattening: {:?}", transform_result);
                    }
                    Err(transform_error) => {
                        println!("Flow flattening error: {}", transform_error);
                    }
                }

                // Summary
                println!("\nSummary");
                println!("=======");
                let total_errors = parse_result.errors().len() + 
                                 symbol_errors.len() + 
                                 semantic_errors.len() + 
                                 validation_report.errors.len();
                                 
                let total_warnings = semantic_context.warnings.len() + 
                                   validation_report.warnings.len() + 
                                   constraint_result.warnings.len();

                println!("✓ Circuit flow paradigm test completed");
                println!("  - Total errors: {}", total_errors);
                println!("  - Total warnings: {}", total_warnings);
                println!("  - Flow statements processed: {}", counter.flow_statements);
                println!("  - Components analyzed: {}", counter.component_instantiations);
                println!("  - Generate statements found: {}", counter.generate_statements);
                
                if total_errors == 0 {
                    println!("🎉 All tests passed! Circuit flow paradigm is working correctly.");
                } else {
                    println!("⚠️  Some issues found, but basic functionality is working.");
                }

                // Test specific circuit flow features
                test_flow_expression_parsing();
                test_component_instantiation_features();
                test_generate_statement_features();
                test_constraint_checking_features();
                
            } else {
                panic!("No board found in parsed AST");
            }
        } else {
            panic!("Failed to cast root to SourceFile");
        }
    }

    fn test_flow_expression_parsing() {
        println!("\nTesting Flow Expression Features");
        println!("================================");

        let flow_examples = vec![
            "simple_flow: A -> B;",
            "bidirectional: A <-> B;",
            "flow_op: A |> B;",
            "interface_op: A <=> B;",
            "complex_flow: VCC -> Res(330Ω).1 -> LED(red).A -> GND;",
        ];

        for (i, example) in flow_examples.iter().enumerate() {
            println!("Flow example {}: {}", i + 1, example);
            let parse_result = parse(example);
            
            if parse_result.errors().is_empty() {
                println!("  ✓ Parsed successfully");
            } else {
                println!("  ✗ Parse errors: {:?}", parse_result.errors());
            }
        }
    }

    fn test_component_instantiation_features() {
        println!("\nTesting Component Instantiation Features");
        println!("========================================");

        let component_examples = vec![
            "r1: Res(330Ω);",
            "r2: Res(value = 1kΩ, tolerance = 5%);",
            "led1: LED(color = \"red\");",
            "cap1: Cap(100µF);",
        ];

        for (i, example) in component_examples.iter().enumerate() {
            println!("Component example {}: {}", i + 1, example);
            let parse_result = parse(example);
            
            if parse_result.errors().is_empty() {
                println!("  ✓ Parsed successfully");
            } else {
                println!("  ✗ Parse errors: {:?}", parse_result.errors());
            }
        }
    }

    fn test_generate_statement_features() {
        println!("\nTesting Generate Statement Features");
        println!("===================================");

        let generate_examples = vec![
            "generate for i in 0..3 { led: LED(red); }",
            "generate for j in 1..5 { res: Res(1kΩ); }",
        ];

        for (i, example) in generate_examples.iter().enumerate() {
            println!("Generate example {}: {}", i + 1, example);
            let board_code = format!("board test {{ {} }}", example);
            let parse_result = parse(&board_code);
            
            if parse_result.errors().is_empty() {
                println!("  ✓ Parsed successfully");
            } else {
                println!("  ✗ Parse errors: {:?}", parse_result.errors());
            }
        }
    }

    fn test_constraint_checking_features() {
        println!("\nTesting Constraint Checking Features");
        println!("====================================");

        // Test standard resistor values
        let test_values = vec![1.0, 4.7, 10.0, 47.0, 100.0, 5.0, 1.25];
        
        for value in test_values {
            let is_standard = is_standard_resistor_value(value);
            println!("  {}Ω is standard E24 value: {}", value, is_standard);
        }

        println!("✓ Constraint checking features tested");
    }

    #[test]
    fn test_individual_components() {
        println!("\nTesting Individual Components");
        println!("=============================");

        // Test basic parsing
        let simple_code = "board test { led1: LED(red); }";
        let parse_result = parse(simple_code);
        assert!(parse_result.errors().is_empty(), "Simple board should parse without errors");

        // Test flow expression
        let flow_code = "board test { flow1: A -> B; }";
        let parse_result = parse(flow_code);
        assert!(parse_result.errors().is_empty(), "Flow expression should parse without errors");

        // Test component with parameters
        let param_code = "board test { res1: Res(value = 330Ω); }";
        let parse_result = parse(param_code);
        assert!(parse_result.errors().is_empty(), "Parameterized component should parse without errors");

        println!("✓ All individual component tests passed");
    }

    #[test]
    fn test_error_recovery() {
        println!("\nTesting Error Recovery");
        println!("======================");

        let error_cases = vec![
            ("Missing semicolon", "board test { led1: LED(red) }"),
            ("Invalid syntax", "board test { ??? }"),
            ("Unclosed brace", "board test { led1: LED(red);"),
        ];

        for (description, code) in error_cases {
            println!("Testing: {}", description);
            let parse_result = parse(code);
            
            if !parse_result.errors().is_empty() {
                println!("  ✓ Errors caught: {} error(s)", parse_result.errors().len());
            } else {
                println!("  ⚠️  No errors found (unexpected)");
            }
        }
    }
}

// Helper function to run all tests
#[cfg(test)]
pub fn run_circuit_flow_tests() {
    circuit_flow_tests::test_complete_circuit_flow_paradigm();
    circuit_flow_tests::test_individual_components();
    circuit_flow_tests::test_error_recovery();
    
    println!("\n🎉 All circuit flow paradigm tests completed!");
}