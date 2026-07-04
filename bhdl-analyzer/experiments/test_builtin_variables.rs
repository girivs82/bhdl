use std::env;
use std::collections::HashSet;
use bhdl_ast::SourceFile;
use bhdl_ast::attributes::AttributeType;
use bhdl_parser::parse;
use rowan::ast::AstNode;

fn main() {
    let args: Vec<String> = env::args().collect();
    let test_file = if args.len() > 1 {
        args[1].clone()
    } else {
        "tests/circuits/simple/test_builtin_dt.bhdl".to_string()
    };
    
    println!("Testing built-in variable support with: {}", test_file);
    
    // Read the test file
    let content = std::fs::read_to_string(&test_file)
        .expect(&format!("Failed to read {}", test_file));
    
    // Parse the content
    let parse_result = parse(&content);
    
    // Get the AST
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax)
        .expect("Failed to create AST");
    
    // Run semantic analysis
    let analysis_result = bhdl_analyzer::analyze(&source_file);
    
    println!("\n=== Analysis Results ===");
    println!("Diagnostics: {}", analysis_result.diagnostics.len());
    for diag in &analysis_result.diagnostics {
        println!("  {:?}: {}", diag.range, diag.message);
    }
    
    // Check attribute analysis results
    let attr_result = &analysis_result.attribute_analysis;
    println!("\n=== Attribute Analysis ===");
    println!("Total attributes: {}", attr_result.attributes.len());
    
    // Show expression attributes (those with dependencies)
    let expression_attrs: Vec<_> = attr_result.attributes.values()
        .filter(|info| matches!(info.attribute_type, AttributeType::Expression(_)))
        .collect();
    
    println!("Expression attributes: {}", expression_attrs.len());
    for info in &expression_attrs {
        println!("  {}: depends on {:?}", info.name, 
            attr_result.dependencies.get(&info.name).unwrap_or(&HashSet::new()));
    }
    
    println!("\nEvaluation order:");
    for (i, attr) in attr_result.evaluation_order.iter().enumerate() {
        println!("  {}: {}", i + 1, attr);
    }
    
    // Check for circular dependencies
    if !attr_result.circular_dependencies.is_empty() {
        println!("\n⚠️  Circular dependencies detected:");
        for cycle in &attr_result.circular_dependencies {
            println!("  {}", cycle.join(" -> "));
        }
    }
    
    println!("\n=== Built-in Variable Test Complete ===");
}