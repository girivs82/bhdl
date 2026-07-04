use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, HasName, BoardV2Ext, ComponentInst, SyntaxKind};
use rowan::NodeOrToken;

fn main() {
    let test_cases = vec![
        ("Empty params", r#"
board Test {
    power VCC = 5V;
    ground GND;
    VCC -> r1: Res().1 -> LED(red).A;
}"#),
        ("Explicit placeholder", r#"
board Test {
    power VCC = 5V;
    ground GND;
    VCC -> r1: Res(?).1 -> LED(red).A;
}"#),
        ("Placeholder with constraints", r#"
board Test {
    power VCC = 5V;
    ground GND;
    VCC -> r1: Res(?, rating=0.25W, tolerance=5%).1 -> LED(red).A;
}"#),
    ];
    
    for (name, source) in test_cases {
        println!("\n=== {} ===", name);
        
        let parse_result = parse(source);
        if !parse_result.errors().is_empty() {
            println!("Parse errors:");
            for error in parse_result.errors() {
                println!("  {}", error.message);
            }
        }
        
        let root = parse_result.syntax();
        let source_file = SourceFile::cast(root).expect("Expected SourceFile");
        
        // Find board
        if let Some(board) = source_file.boards().next() {
            println!("Board: {}", board.name().map(|n| n.text().to_string()).unwrap_or_default());
            
            // Look for connection statements
            for stmt in board.statements() {
                if let bhdl_ast::Statement::ConnectionStmt(connection) = stmt {
                    println!("  Connection statement found");
                    
                    // Walk through the connection AST to find component instantiations
                    find_component_instantiations(connection.syntax());
                }
            }
        }
    }
}

fn find_component_instantiations(node: &rowan::SyntaxNode<bhdl_ast::BhdlLanguage>) {
    // Look for COMPONENT_INST nodes
    if node.kind() == SyntaxKind::COMPONENT_INST {
        if let Some(comp_inst) = ComponentInst::cast(node.clone()) {
            check_component_instantiation(&comp_inst);
        }
    }
    
    // Recurse into children
    for child in node.children() {
        find_component_instantiations(&child);
    }
}

fn check_component_instantiation(comp_inst: &ComponentInst) {
    let comp_type = comp_inst.component_type_name()
        .map(|t| t.text().to_string())
        .unwrap_or_default();
    
    println!("    Component: {}", comp_type);
    
    if let Some(param_block) = comp_inst.param_assign_block() {
        if param_block.has_placeholder() {
            println!("      Has placeholder parameter!");
            
            if let Some(placeholder) = param_block.placeholder() {
                println!("      Explicit: {}", placeholder.is_explicit());
                
                let constraints: Vec<_> = placeholder.constraints().collect();
                if !constraints.is_empty() {
                    println!("      Constraints:");
                    for constraint in constraints {
                        if let Some(name) = constraint.name() {
                            println!("        - {}", name.text());
                        }
                    }
                }
            }
        } else {
            let params: Vec<_> = param_block.assignments().collect();
            println!("      Parameters: {} assignments", params.len());
        }
    }
}