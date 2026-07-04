use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};

fn print_ast_nodes(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let indent_str = "  ".repeat(indent);
    println!("{}{:?}: '{}'", indent_str, node.kind(), node.text().to_string().trim());
    
    for child in node.children() {
        print_ast_nodes(&child, indent + 1);
    }
}

fn main() {
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        mcu_i2c: I2C();
        sensor_i2c: I2C();
        
        mcu_i2c <=> sensor_i2c;
    }
    "#;
    
    println!("Parsing interface-to-interface connection...\n");
    
    let parsed = parse(source);
    if !parsed.errors().is_empty() {
        eprintln!("Parse errors: {:?}", parsed.errors());
        return;
    }
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    println!("AST Structure:");
    print_ast_nodes(source_file.syntax(), 0);
}