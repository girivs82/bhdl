// Test behavioral attributes parsing and AST generation

use bhdl_parser::parse;
use bhdl_ast::source_file::SourceFile;
use bhdl_ast::attributes::AttributeDecl;
use rowan::ast::AstNode;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing behavioral attributes...");
    
    // Read the test file
    let test_file = "tests/circuits/simple/test_behavioral_attributes.bhdl";
    let source = fs::read_to_string(test_file)?;
    
    // Parse the file
    println!("\nParsing {}...", test_file);
    let parsed = parse(&source);
    
    if !parsed.errors().is_empty() {
        println!("\nParser errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
    }
    
    // Get the AST
    let source_file = SourceFile::cast(parsed.syntax()).expect("Failed to get SourceFile");
    
    // Find all attribute declarations
    println!("\nFinding attribute declarations...");
    let attributes = find_attributes(&source_file);
    
    println!("\nFound {} attributes:", attributes.len());
    for (i, attr) in attributes.iter().enumerate() {
        if let Some(name) = attr.name() {
            println!("\n{}. Attribute: {}", i + 1, name.text());
            
            if let Some(value) = attr.value() {
                println!("   Value: {:?}", value.syntax().text());
                println!("   Expression type: {:?}", std::mem::discriminant(&value));
                println!("   Is expression: {}", attr.is_expression_attribute());
                
                let pin_refs = attr.referenced_pins();
                if !pin_refs.is_empty() {
                    println!("   Pin references: {:?}", pin_refs);
                }
                
                let attr_refs = attr.referenced_attributes();
                if !attr_refs.is_empty() {
                    println!("   Attribute references: {:?}", attr_refs);
                }
                
                // Also print the raw syntax for debugging
                println!("   Raw syntax kind: {:?}", value.syntax().kind());
                
                // Debug: print the expression tree
                if attr.is_expression_attribute() {
                    println!("   Expression tree:");
                    print_expr_tree(&value, 2);
                }
            }
        }
    }
    
    // Test specific attribute types
    println!("\n\nAnalyzing attribute types:");
    
    // Static attributes
    println!("\nStatic attributes (literals):");
    for attr in &attributes {
        if let Some(name) = attr.name() {
            if !attr.is_expression_attribute() {
                println!("  - {}", name.text());
            }
        }
    }
    
    // Expression attributes
    println!("\nExpression attributes:");
    for attr in &attributes {
        if let Some(name) = attr.name() {
            if attr.is_expression_attribute() {
                println!("  - {}", name.text());
            }
        }
    }
    
    println!("\n✅ Behavioral attributes test completed!");
    
    Ok(())
}

fn find_attributes(node: &SourceFile) -> Vec<AttributeDecl> {
    let mut attributes = Vec::new();
    
    // Walk the syntax tree to find attribute declarations
    for child in node.syntax().descendants() {
        if let Some(attr) = AttributeDecl::cast(child) {
            attributes.push(attr);
        }
    }
    
    attributes
}

fn print_expr_tree(expr: &bhdl_ast::expr::Expr, indent: usize) {
    use bhdl_ast::expr::Expr;
    let prefix = " ".repeat(indent * 2);
    
    match expr {
        Expr::Value(v) => println!("{}Value: {:?}", prefix, v.syntax().text()),
        Expr::IdentRef(i) => println!("{}IdentRef: {:?}", prefix, i.token().map(|t| t.text().to_string())),
        Expr::Ident(n) => println!("{}Ident: {:?}", prefix, n.text()),
        Expr::Literal(n) => println!("{}Literal: {:?}", prefix, n.text()),
        Expr::BinaryExpr(b) => {
            println!("{}BinaryExpr:", prefix);
            if let Some(lhs) = b.lhs() {
                println!("{}  Left:", prefix);
                print_expr_tree(&lhs, indent + 2);
            }
            println!("{}  Op: {:?}", prefix, b.op());
            if let Some(rhs) = b.rhs() {
                println!("{}  Right:", prefix);
                print_expr_tree(&rhs, indent + 2);
            }
        }
        _ => println!("{}Other expr type", prefix),
    }
}