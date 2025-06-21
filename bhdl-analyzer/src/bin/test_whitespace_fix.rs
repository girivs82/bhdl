use std::env;
use bhdl_ast::SourceFile;
use bhdl_parser::parse;
use rowan::ast::AstNode;

fn main() {
    let args: Vec<String> = env::args().collect();
    let test_file = if args.len() > 1 {
        args[1].clone()
    } else {
        "tests/circuits/simple/test_expr_eval.bhdl".to_string()
    };
    
    println!("Testing whitespace fix with: {}", test_file);
    
    // Read the test file
    let content = std::fs::read_to_string(&test_file)
        .expect(&format!("Failed to read {}", test_file));
    
    // Parse the content
    let parse_result = parse(&content);
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
    
    // Check attribute analysis
    println!("\n=== Attribute Analysis ===");
    let attr_result = &analysis_result.attribute_analysis;
    
    println!("Total attributes: {}", attr_result.attributes.len());
    println!("\nDependencies:");
    for (attr_name, deps) in &attr_result.dependencies {
        if !deps.is_empty() {
            println!("  {} depends on: {:?}", attr_name, deps);
            
            // Check for whitespace in dependencies
            for dep in deps {
                if dep.trim() != dep {
                    println!("    ⚠️  WARNING: Dependency '{}' contains whitespace!", dep);
                } else {
                    println!("    ✓ Dependency '{}' is clean", dep);
                }
            }
        }
    }
    
    println!("\nEvaluation order: {:?}", attr_result.evaluation_order);
    
    // Also check individual attributes
    if let Some(board) = source_file.boards().next() {
        println!("\n=== Individual Attribute Check ===");
        for attr in board.attributes() {
            if let Some(name) = attr.name() {
                let refs = attr.referenced_attributes();
                if !refs.is_empty() {
                    println!("\n{}: references {:?}", name.text(), refs);
                    for ref_name in &refs {
                        if ref_name.trim() != ref_name {
                            println!("  ⚠️  Reference '{}' has whitespace (bytes: {:?})", 
                                ref_name, ref_name.as_bytes());
                        }
                    }
                }
            }
        }
    }
    
    println!("\n=== Whitespace Test Complete ===");
}