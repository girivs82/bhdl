use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Symbol Resolution for VCC without @\n");
    
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
    
    // Analyze
    let result = analyze(&source_file);
    
    // Check global symbols
    println!("=== Global Symbols ===");
    for (name, symbol) in result.global_scope.get_symbols().iter() {
        println!("  {} => {:?}", name, symbol.kind);
    }
    
    // Check board scope
    println!("\n=== Board Scope ===");
    for (_, scope) in result.definition_scopes.iter() {
        if scope.scope_name.as_ref().map(|n| n == "Test").unwrap_or(false) {
            println!("Regular symbols:");
            for (name, symbol) in scope.get_symbols().iter() {
                println!("  {} => {:?}", name, symbol.kind);
            }
            println!("Net symbols:");
            for (name, symbol) in scope.get_nets().iter() {
                println!("  {} => {:?}", name, symbol.kind);
            }
        }
    }
    
    // Check diagnostics
    println!("\n=== All Diagnostics ({}) ===", result.diagnostics.len());
    for diag in &result.diagnostics {
        println!("  {}", diag.message);
    }
}