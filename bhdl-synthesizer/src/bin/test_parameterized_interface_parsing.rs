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
    interface SPI(width: int = 8, frequency: frequency = 1MHz) {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out optional;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        spi8: SPI();
        spi16: SPI(width=16);
        fast_spi: SPI(width=16, frequency=10MHz);
    }
    "#;
    
    println!("Parsing parameterized interface...\n");
    
    let parsed = parse(source);
    if !parsed.errors().is_empty() {
        eprintln!("Parse errors: {:?}", parsed.errors());
        for error in parsed.errors() {
            eprintln!("  Error: {:?}", error);
        }
    }
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    println!("AST Structure:");
    print_ast_nodes(source_file.syntax(), 0);
}