// Test virtual pin analysis
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

fn main() {
    let content = r#"
module TPS54331(vout: voltage = 3.3V) {
    // Physical pins
    pin VIN: power in;
    pin SW: signal out;
    pin GND: ground inout;
    pin FB: signal in;
    
    // Virtual pin - should be tracked as VirtualPin symbol
    pin VOUT: virtual power out;
}
"#;

    let parsed = parse(content);
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        return;
    }

    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis.diagnostics {
            println!("  - {}", diag.message);
        }
        println!();
    }
    
    // Check symbols in global scope
    println!("Global symbols:");
    for (name, symbol) in analysis.global_scope.get_symbols() {
        println!("  {}: {:?}", name, symbol.kind);
        
        // Look for module scope
        if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Module {
            if let Some(module_scope_id) = symbol.definition_node_ptr {
                if let Some(module_scope) = analysis.definition_scopes.get(&module_scope_id) {
                    println!("    Module scope symbols:");
                    for (pin_name, pin_symbol) in module_scope.get_symbols() {
                        println!("      {}: {:?}", pin_name, pin_symbol.kind);
                    }
                }
            }
        }
    }
}