//! Test program for expression evaluation in behavioral models

use bhdl_sim::evaluation::{
    SimulationAttributeEvaluator, 
    WhenBlockProcessor,
    expression_parser::ExpressionParser,
};
use bhdl_sim::circuit::{CircuitState, CircuitTopology};
use bhdl_sim::engine::time::TimeManager;
use bhdl_analyzer::attribute_analysis::{AttributeAnalysisResult, AttributeInfo, WhenBlockInfo};
use bhdl_analyzer::expression_evaluator::RuntimeValue;
use bhdl_ast::attributes::{AttributeType, AttributeDependency};
use bhdl_ast::{SourceFile, AstNode};
use std::collections::{HashMap, HashSet};

fn main() {
    println!("Testing Expression Evaluation for Behavioral Models");
    println!("=================================================\n");
    
    // Test 1: Basic expression parsing and evaluation
    test_expression_parser();
    
    // Test 2: Attribute evaluation with dependencies
    test_attribute_evaluator();
    
    // Test 3: When block processing
    test_when_blocks();
    
    println!("\n✓ All expression evaluation tests completed!");
}

fn test_expression_parser() {
    println!("1. Testing Expression Parser:");
    println!("   ------------------------");
    
    let mut parser = ExpressionParser::new();
    
    // Test simple arithmetic
    let expr_text = "2 + 3 * 4";
    match parser.parse(expr_text) {
        Ok(expr) => {
            println!("   ✓ Parsed: {} -> {:?}", expr_text, expr);
            
            // Create evaluation context
            let sim_context = bhdl_analyzer::builtin_variables::SimulationContext::new(0.001);
            let eval_context = bhdl_analyzer::expression_evaluator::EvaluationContext::new(&sim_context);
            
            // Evaluate
            match bhdl_analyzer::expression_evaluator::ExpressionEvaluator::evaluate(&expr, &eval_context) {
                Ok(result) => println!("   ✓ Result: {:?}", result),
                Err(e) => println!("   ✗ Evaluation error: {}", e),
            }
        }
        Err(e) => println!("   ✗ Parse error: {}", e),
    }
    
    // Test ternary expression
    let expr_text = "t > 1.0 ? 5.0 : 2.0";
    match parser.parse(expr_text) {
        Ok(expr) => {
            println!("   ✓ Parsed ternary: {}", expr_text);
            
            // Create context with t = 1.5
            let mut sim_context = bhdl_analyzer::builtin_variables::SimulationContext::new(0.001);
            sim_context.current_time = 1.5;
            let eval_context = bhdl_analyzer::expression_evaluator::EvaluationContext::new(&sim_context);
            
            match bhdl_analyzer::expression_evaluator::ExpressionEvaluator::evaluate(&expr, &eval_context) {
                Ok(result) => println!("   ✓ Result (t=1.5): {:?}", result),
                Err(e) => println!("   ✗ Evaluation error: {}", e),
            }
        }
        Err(e) => println!("   ✗ Parse error: {}", e),
    }
    
    // Test function call
    let expr_text = "sin(2 * pi * t)";
    match parser.parse(expr_text) {
        Ok(_) => println!("   ✓ Parsed function call: {}", expr_text),
        Err(e) => println!("   ✗ Parse error: {}", e),
    }
}

fn test_attribute_evaluator() {
    println!("\n2. Testing Attribute Evaluator:");
    println!("   ---------------------------");
    
    // Create attribute analysis result
    // Note: In a real scenario, AttributeInfo would come from the analyzer
    // For this test, we'll just create a minimal attributes map
    let attributes = HashMap::new();
    
    let analysis = AttributeAnalysisResult {
        attributes,
        dependencies: HashMap::new(),
        evaluation_order: vec!["voltage".to_string(), "current".to_string()],
        circular_dependencies: vec![],
        mutable_attributes: HashSet::new(),
    };
    
    // Create evaluator with expression texts
    let mut expression_texts = HashMap::new();
    expression_texts.insert("voltage".to_string(), "5.0 * sin(2 * pi * t)".to_string());
    expression_texts.insert("current".to_string(), "voltage / 10.0".to_string());
    
    let mut evaluator = SimulationAttributeEvaluator::with_expressions(analysis, expression_texts);
    
    // Create circuit state
    let topology = CircuitTopology {
        instance_modules: HashMap::new(),
        net_connections: HashMap::new(),
    };
    let mut circuit_state = CircuitState::new(topology);
    
    // Initialize attributes
    circuit_state.update_attribute("voltage", RuntimeValue::Real(0.0));
    circuit_state.update_attribute("current", RuntimeValue::Real(0.0));
    
    // Create time manager
    let time_manager = TimeManager::new(0.001);
    
    // Evaluate attributes
    let attr_ids = vec![
        bhdl_sim::evaluation::scheduler::AttributeId("voltage".to_string()),
        bhdl_sim::evaluation::scheduler::AttributeId("current".to_string()),
    ];
    
    match evaluator.evaluate_batch(&attr_ids, &mut circuit_state, &time_manager) {
        Ok(results) => {
            for result in results {
                println!("   ✓ {}: {:?} -> {:?}", 
                    result.attribute, 
                    result.old_value, 
                    result.new_value);
            }
        }
        Err(e) => println!("   ✗ Evaluation error: {}", e),
    }
}

fn test_when_blocks() {
    println!("\n3. Testing When Block Processing:");
    println!("   ------------------------------");
    
    // Create when blocks
    let when_blocks = vec![
        WhenBlockInfo {
            condition: "t > 1.0".to_string(),
            assignments: {
                let mut map = HashMap::new();
                map.insert("output_enable".to_string(), "1.0".to_string());
                map.insert("led_brightness".to_string(), "0.8".to_string());
                map
            },
        },
        WhenBlockInfo {
            condition: "t > 2.0".to_string(),
            assignments: {
                let mut map = HashMap::new();
                map.insert("output_enable".to_string(), "0.0".to_string());
                map
            },
        },
    ];
    
    let mut processor = WhenBlockProcessor::new(when_blocks);
    
    // Create circuit state
    let topology = CircuitTopology {
        instance_modules: HashMap::new(),
        net_connections: HashMap::new(),
    };
    let mut circuit_state = CircuitState::new(topology);
    
    // Initialize mutable attributes
    circuit_state.update_attribute("output_enable", RuntimeValue::Real(0.0));
    circuit_state.update_attribute("led_brightness", RuntimeValue::Real(0.0));
    
    // Test at t = 0.5 (no conditions true)
    let mut time_manager = TimeManager::new(0.001);
    match processor.process_all(&mut circuit_state, &time_manager) {
        Ok(result) => {
            println!("   ✓ At t=0.0: {} attributes updated", result.updated_attributes.len());
        }
        Err(e) => println!("   ✗ Processing error: {}", e),
    }
    
    // Test at t = 1.5 (first condition true)
    time_manager.advance_by(1.5);
    match processor.process_all(&mut circuit_state, &time_manager) {
        Ok(result) => {
            println!("   ✓ At t=1.5: {} attributes updated", result.updated_attributes.len());
            for attr in &result.updated_attributes {
                if let Some(value) = circuit_state.get_attribute(attr) {
                    println!("     - {}: {:?}", attr, value);
                }
            }
        }
        Err(e) => println!("   ✗ Processing error: {}", e),
    }
    
    // Test at t = 2.5 (second condition true)
    time_manager.advance_by(1.0);
    match processor.process_all(&mut circuit_state, &time_manager) {
        Ok(result) => {
            println!("   ✓ At t=2.5: {} attributes updated", result.updated_attributes.len());
            for attr in &result.updated_attributes {
                if let Some(value) = circuit_state.get_attribute(attr) {
                    println!("     - {}: {:?}", attr, value);
                }
            }
        }
        Err(e) => println!("   ✗ Processing error: {}", e),
    }
}