use std::env;
use std::collections::HashMap;
use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;
use bhdl_analyzer::expression_evaluator::{ExpressionEvaluator, EvaluationContext, RuntimeValue};
use bhdl_analyzer::builtin_variables::SimulationContext;

fn main() {
    let args: Vec<String> = env::args().collect();
    let test_file = if args.len() > 1 {
        args[1].clone()
    } else {
        "tests/circuits/simple/test_behavioral_complete.bhdl".to_string()
    };
    
    println!("Testing expression evaluator with: {}", test_file);
    
    // Read the test file
    let content = std::fs::read_to_string(&test_file)
        .expect(&format!("Failed to read {}", test_file));
    
    // Parse the content
    let parse_result = parse(&content);
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax)
        .expect("Failed to create AST");
    
    // Run semantic analysis first
    let analysis_result = bhdl_analyzer::analyze(&source_file);
    
    println!("\n=== Analysis Results ===");
    println!("Diagnostics: {}", analysis_result.diagnostics.len());
    for diag in &analysis_result.diagnostics {
        println!("  {:?}: {}", diag.range, diag.message);
    }
    
    // Set up simulation context
    let sim_context_ref = SimulationContext::new(0.001); // 1ms time step
    
    // Set up evaluation context
    let mut eval_context = EvaluationContext::new(&sim_context_ref);
    
    // Collect all attributes from the board
    let mut attributes = HashMap::new();
    if let Some(board) = source_file.boards().next() {
        for attr_decl in board.attribute_decls() {
            if let Some(name_token) = attr_decl.name() {
                attributes.insert(name_token.text().to_string(), attr_decl);
            }
        }
    }
    
    println!("\n=== Initial Attribute Evaluation ===");
    
    // First pass: evaluate static attributes
    for (name, attr_decl) in &attributes {
        if let Some(expr) = attr_decl.value() {
            match ExpressionEvaluator::evaluate(&expr, &eval_context) {
                Ok(value) => {
                    println!("{} = {:?}", name, value);
                    eval_context.set_attribute(name.clone(), value);
                }
                Err(e) => {
                    println!("{} = ERROR: {}", name, e);
                }
            }
        }
    }
    
    // Simulate some pin values for testing
    eval_context.set_pin("voltage_level".to_string(), RuntimeValue::Real(2.5));
    
    println!("\n=== Simulation Steps ===");
    
    // Simulate a few time steps
    let mut current_time = 0.0;
    let time_step = 0.001;
    
    for step in 0..5 {
        println!("\nStep {}: t = {:.3}s", step, current_time);
        
        // Create new simulation context for this time step
        let mut step_sim_context = SimulationContext::new(time_step);
        step_sim_context.current_time = current_time;
        
        // Preserve attributes from previous step
        let prev_attributes = eval_context.attributes.clone();
        
        // Create new evaluation context for this step
        eval_context = EvaluationContext::new(&step_sim_context);
        
        // Restore attributes
        for (name, value) in prev_attributes {
            eval_context.set_attribute(name, value);
        }
        
        // Update voltage level (simulate a square wave)
        let voltage = if step % 2 == 0 { 0.0 } else { 5.0 };
        eval_context.set_attribute("voltage_level".to_string(), RuntimeValue::Real(voltage));
        println!("  voltage_level = {:.1}V", voltage);
        
        // Re-evaluate dependent attributes
        let attr_result = &analysis_result.attribute_analysis;
        for attr_name in &attr_result.evaluation_order {
            if let Some(attr_decl) = attributes.get(attr_name as &str) {
                if let Some(expr) = attr_decl.value() {
                    match ExpressionEvaluator::evaluate(&expr, &eval_context) {
                        Ok(value) => {
                            println!("  {} = {:?}", attr_name, value);
                            eval_context.set_attribute(attr_name.clone(), value);
                        }
                        Err(e) => {
                            println!("  {} = ERROR: {}", attr_name, e);
                        }
                    }
                }
            }
        }
        
        // Check when block conditions
        if let Some(board) = source_file.boards().next() {
            for when_block in board.when_blocks() {
                if let Some(condition) = when_block.condition() {
                    match ExpressionEvaluator::evaluate(&condition, &eval_context) {
                        Ok(RuntimeValue::Boolean(true)) => {
                            println!("  WHEN condition triggered: {}", 
                                when_block.syntax().text().to_string().lines().next().unwrap_or(""));
                            
                            // Process attribute assignments in when block
                            for assignment in when_block.attribute_assignments() {
                                if let Some(name_token) = assignment.name() {
                                    let attr_name = name_token.text();
                                    
                                    // Handle increment/decrement operators
                                    if let Some(op) = assignment.op_token() {
                                        let current = eval_context.attributes.get(attr_name)
                                            .cloned()
                                            .unwrap_or(RuntimeValue::Integer(0));
                                        
                                        if let Some(expr) = assignment.value() {
                                            match ExpressionEvaluator::evaluate(&expr, &eval_context) {
                                                Ok(inc_value) => {
                                                    match op.text() {
                                                        "+=" => {
                                                            if let (Ok(curr_f), Ok(inc_f)) = 
                                                                (current.to_f64(), inc_value.to_f64()) {
                                                                let new_val = RuntimeValue::Real(curr_f + inc_f);
                                                                eval_context.set_attribute(attr_name.to_string(), new_val.clone());
                                                                println!("    {} += {:?} -> {:?}", attr_name, inc_value, new_val);
                                                            }
                                                        }
                                                        "=" => {
                                                            eval_context.set_attribute(attr_name.to_string(), inc_value.clone());
                                                            println!("    {} = {:?}", attr_name, inc_value);
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                                Err(e) => {
                                                    println!("    {} assignment ERROR: {}", attr_name, e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(RuntimeValue::Boolean(false)) => {
                            // Condition not met
                        }
                        Ok(_) => {
                            println!("  WHEN condition is not boolean");
                        }
                        Err(e) => {
                            println!("  WHEN condition ERROR: {}", e);
                        }
                    }
                }
            }
        }
        
        
        // Advance simulation time
        current_time += time_step;
    }
    
    println!("\n=== Final Attribute Values ===");
    for (name, value) in &eval_context.attributes {
        println!("{} = {:?}", name, value);
    }
    
    println!("\n=== Expression Evaluator Test Complete ===");
}