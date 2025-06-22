// Test that power domains are treated as nets with attributes
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_analyzer::symbol_table::SymbolKind;
use bhdl_analyzer::net_attributes::NetAttribute;

fn main() {
    println!("Testing Power Domains as Nets with Attributes\n");
    
    let test_bhdl = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    power VDD = 3.3V @ 500mA;
    ground GND;
    
    @VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> @GND;
}
"#;
    
    // Parse
    let parse_result = parse(test_bhdl);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    
    let source_file = SourceFile::cast(parse_result.syntax()).expect("Should be a SourceFile");
    
    // Analyze
    let result = analyze(&source_file);
    
    // Check symbol table for power domains
    println!("=== Symbol Table Net Entries ===");
    
    // Find the board scope
    for (_, scope) in result.definition_scopes.iter() {
        if scope.scope_name.as_ref().map(|n| n == "TestBoard").unwrap_or(false) {
            println!("Board scope nets:");
            for (name, symbol) in scope.get_nets().iter() {
                if symbol.kind == SymbolKind::Net {
                    println!("\n  Net: {}", name);
                    if let Some(net_attr) = &symbol.net_attributes {
                        match net_attr {
                            NetAttribute::PowerDomain { voltage, max_current, .. } => {
                                println!("    Type: Power Domain");
                                println!("    Voltage: {}V", voltage);
                                println!("    Max Current: {}A", max_current);
                            }
                            NetAttribute::GroundDomain => {
                                println!("    Type: Ground Domain (0V)");
                            }
                            _ => {
                                println!("    Type: Generic Net");
                            }
                        }
                    } else {
                        println!("    Type: Regular Net (no attributes)");
                    }
                }
            }
        }
    }
    
    // Check power analysis results
    println!("\n\n=== Power Analysis Results ===");
    println!("Power domains found: {}", result.power_analysis.domains.len());
    for (name, domain) in &result.power_analysis.domains {
        println!("  {}: {}V @ {}A", name, domain.voltage, domain.max_current);
    }
    
    // Show diagnostics
    println!("\n=== Diagnostics ===");
    if result.diagnostics.is_empty() {
        println!("No diagnostics");
    } else {
        for diag in &result.diagnostics {
            if !diag.message.contains("Component Inference") {
                println!("  - {}", diag.message);
            }
        }
    }
}