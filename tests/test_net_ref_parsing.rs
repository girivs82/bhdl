//! Test parsing of @NETNAME syntax

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Board, HasName};
use bhdl_ast::v2_statements::ConnectionStmt;
use bhdl_ast::expr::{Expr, BinaryExpr};
use bhdl_ast::common::NetRef;

#[test]
fn test_net_ref_parsing() {
    let source = r#"
board TestNetRef {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Test named net
    VCC @FILTERED-> r1: Res(10k).1;
    @FILTERED -> c1: Cap(100n).1;
    
    // Test anonymous net
    r1.2 -> led: LED(red).A;
    led.K -> GND;
}
"#;
    
    // Parse the source
    let parse_result = parse(source);
    assert!(parse_result.errors().is_empty(), "Parse errors: {:?}", parse_result.errors());
    
    // Get the AST
    let ast = SourceFile::cast(parse_result.syntax()).unwrap();
    let board = ast.boards().next().unwrap();
    assert_eq!(board.name().unwrap().text(), "TestNetRef");
    
    // Get connections
    let connections: Vec<ConnectionStmt> = board.connections().collect();
    assert_eq!(connections.len(), 4);
    
    // Check first connection: VCC @FILTERED-> r1: Res(10k).1
    let conn1 = &connections[0];
    let conn1_text = conn1.syntax().text().to_string();
    assert!(conn1_text.contains("@FILTERED"));
    
    // Check if we can find the binary expression
    if let Some(binary_expr) = conn1.syntax().descendants().find_map(BinaryExpr::cast) {
        // Check left side (should be VCC)
        if let Some(Expr::IdentRef(ident)) = binary_expr.lhs() {
            assert_eq!(ident.syntax().text().to_string().trim(), "VCC");
        }
        
        // The right side will be another binary expression for @FILTERED-> r1:...
        if let Some(Expr::BinaryExpr(inner_binary)) = binary_expr.rhs() {
            // Check if left side is NetRef
            if let Some(Expr::NetRef(net_ref)) = inner_binary.lhs() {
                assert!(net_ref.has_at_prefix());
                assert_eq!(net_ref.name(), Some("FILTERED".to_string()));
            }
        }
    }
    
    // Check second connection: @FILTERED -> c1: Cap(100n).1
    let conn2 = &connections[1];
    let conn2_text = conn2.syntax().text().to_string();
    assert!(conn2_text.starts_with("@FILTERED"));
    
    // Find the NetRef in this connection
    let net_refs: Vec<NetRef> = conn2.syntax().descendants().filter_map(NetRef::cast).collect();
    assert!(!net_refs.is_empty());
    
    let net_ref = &net_refs[0];
    assert!(net_ref.has_at_prefix());
    assert_eq!(net_ref.name(), Some("FILTERED".to_string()));
}

#[test]
fn test_net_ref_pretty_print() {
    use bhdl_ast::pretty_print::{PrettyPrint, PrettyPrintContext};
    use std::fmt::Write;
    
    let source = "@TESTNET -> r1: Res(10k).1;";
    let parse_result = parse(&format!("board Test {{ {} }}", source));
    
    let ast = SourceFile::cast(parse_result.syntax()).unwrap();
    let board = ast.boards().next().unwrap();
    let conn = board.connections().next().unwrap();
    
    // Find the NetRef
    let net_ref = conn.syntax().descendants().find_map(NetRef::cast).unwrap();
    
    // Pretty print it
    let mut output = String::new();
    let mut ctx = PrettyPrintContext::new(0);
    net_ref.pretty_print(&mut ctx, &mut output).unwrap();
    
    // Should print with @ prefix
    assert_eq!(output, "@TESTNET");
}