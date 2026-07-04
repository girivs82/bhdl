use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Undefined Net Reference\n");
    
    let test_bhdl = r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @undefined_net -> LED(red).A;
    LED(red).K -> @GND;
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
    
    // Check diagnostics
    println!("=== All Diagnostics ({}) ===", result.diagnostics.len());
    for diag in &result.diagnostics {
        println!("  {}", diag.message);
    }
    
    // Check nets in board scope
    println!("\n=== Board Scope Nets ===");
    for (_, scope) in result.definition_scopes.iter() {
        if scope.scope_name.as_ref().map(|n| n == "Test").unwrap_or(false) {
            for (name, _) in scope.get_nets().iter() {
                println!("  {}", name);
            }
        }
    }
}