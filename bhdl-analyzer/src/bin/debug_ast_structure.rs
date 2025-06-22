use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let indent_str = " ".repeat(indent);
    println!("{}{:?} '{}'", indent_str, node.kind(), node.text().to_string().replace('\n', "\\n"));
    
    for child in node.children() {
        print_tree(&child, indent + 2);
    }
}

fn main() {
    let test_bhdl = r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> GND;
}
"#;
    
    // Parse
    let parse_result = parse(test_bhdl);
    if !parse_result.errors().is_empty() {
        println!("Parse errors: {:?}", parse_result.errors());
        return;
    }
    
    let source_file = SourceFile::cast(parse_result.syntax())
        .expect("Should be a SourceFile");
    
    println!("=== AST Structure ===");
    print_tree(&source_file.syntax(), 0);
}