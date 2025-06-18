use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, HasName};

#[test]
fn test_parse_module_alias_syntax() {
    // Test the old syntax used in stdlib files
    let old_syntax = r#"
module Res(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
}

module Resistor = Res;
"#;

    let parse_result = parse(old_syntax);
    println!("Parse errors: {:?}", parse_result.errors());
    
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node).unwrap();
    
    let module_count = source_file.modules().count();
    println!("Found {} modules", module_count);
    
    for module in source_file.modules() {
        println!("Module: {:?}", module.name().map(|n| n.text().to_string()));
    }
}

#[test]
fn test_parse_new_alias_syntax() {
    // Test the new syntax the parser expects
    let new_syntax = r#"
module Res(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
}

alias Resistor = Res;
"#;

    let parse_result = parse(new_syntax);
    println!("Parse errors: {:?}", parse_result.errors());
    
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node).unwrap();
    
    let module_count = source_file.modules().count();
    println!("Found {} modules", module_count);
    
    for module in source_file.modules() {
        println!("Module: {:?}", module.name().map(|n| n.text().to_string()));
    }
}