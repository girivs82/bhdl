use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, BoardV2Ext, SyntaxKind};
use rowan::NodeOrToken;

fn main() {
    let source = r#"
board Test {
    power VCC = 5V;
    ground GND;
    VCC -> r1: Res().1 -> LED(red).A;
}"#;
    
    let parse_result = parse(source);
    let root = parse_result.syntax();
    let source_file = SourceFile::cast(root).expect("Expected SourceFile");
    
    if let Some(board) = source_file.boards().next() {
        for stmt in board.statements() {
            if let bhdl_ast::Statement::ConnectionStmt(connection) = stmt {
                println!("ConnectionStmt structure:");
                print_tree(connection.syntax(), 0);
            }
        }
    }
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_ast::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}Node: {:?}", indent, node.kind());
    
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Token(token) => {
                if !matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT) {
                    println!("{}  Token: {:?} = '{}'", indent, token.kind(), token.text());
                }
            }
            NodeOrToken::Node(child_node) => {
                print_tree(&child_node, depth + 1);
            }
        }
    }
}