use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Power/Ground Net Symbol Creation\n");
    
    let test_bhdl = r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    @VCC -> Res(10k).1 -> LED(red).A;
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
    
    // Access the symbol table to check net symbols
    println!("=== Symbol Table Contents ===");
    
    // Print all symbols in the global scope
    println!("\nGlobal scope symbols:");
    for (name, symbol) in result.global_scope.get_symbols().iter() {
        println!("  {} => {:?} (kind: {:?})", name, symbol.name, symbol.kind);
    }
    
    // Print all net symbols specifically
    println!("\nNet symbols:");
    for (name, symbol) in result.global_scope.get_nets().iter() {
        println!("  {} => {:?} (kind: {:?})", name, symbol.name, symbol.kind);
    }
    
    // Check power domains
    println!("\n=== Power Domains ===");
    for (name, domain) in result.power_analysis.domains.iter() {
        println!("  {} => {}V @ {}A", name, domain.voltage, domain.max_current);
    }
    
    // Check diagnostics
    println!("\n=== Diagnostics ===");
    for diag in &result.diagnostics {
        println!("  {}", diag.message);
    }
    
    // Check board scope
    println!("\n=== Board Scope ===");
    println!("Total definition scopes: {}", result.definition_scopes.len());
    
    // Find the board definition scope
    for (node_ptr, scope) in result.definition_scopes.iter() {
        println!("Scope: {:?}", scope.scope_name);
        
        // Print all nets in this scope
        if !scope.get_nets().is_empty() {
            println!("  Net symbols in this scope:");
            for (name, symbol) in scope.get_nets().iter() {
                println!("    {} => {:?} (kind: {:?})", name, symbol.name, symbol.kind);
            }
        }
    }
}