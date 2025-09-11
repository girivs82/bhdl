use bhdl_parser::{parse, SyntaxKind};

fn main() {
    // Test simple satisfies block with via clause
    let simple_satisfies = r#"
board TestBoard {
    power VDD = 3.3V @ 1A;
    ground GND;
    
    // Component implementation
    voltage_monitor: VoltageMonitor(3.0V);
    
    // Safety compliance declaration
    satisfies {
        REQ_MON_001: via voltage_monitor;
        REQ_PWR_001: via VDD;
        TSR_001: {
            implementation: "Voltage monitoring at 3.0V threshold";
            evidence: "Test report TR-2024-001";
            coverage: 99%;
        };
    }
}
"#;

    println!("Testing simple satisfies block parsing...");
    let result = parse(simple_satisfies);
    
    if result.errors().is_empty() {
        println!("✓ Satisfies block parsed successfully!");
        
        // Print AST structure
        println!("\nAST Structure:");
        print_ast(rowan::NodeOrToken::Node(result.syntax()), 0);
    } else {
        println!("✗ Parse errors:");
        for error in result.errors() {
            println!("  - {:?}", error);
        }
    }
    
    // Test satisfies in module
    let module_satisfies = r#"
module PowerSupply(voltage: voltage) {
    pin VIN: power in;
    pin VOUT: power out;
    pin PGOOD: signal out;
    
    // Implementation details...
    
    satisfies {
        VoltageMonitoring: via internal_monitor;
        OvervoltageProtection: {
            mechanism: "Crowbar circuit";
            threshold: voltage + 0.1V;
            response_time: 5us;
        };
    }
}
"#;

    println!("\n\nTesting module satisfies block parsing...");
    let result2 = parse(module_satisfies);
    
    if result2.errors().is_empty() {
        println!("✓ Module satisfies block parsed successfully!");
    } else {
        println!("✗ Parse errors:");
        for error in result2.errors() {
            println!("  - {:?}", error);
        }
    }
    
    // Test nested satisfies details
    let nested_satisfies = r#"
board SafetyBoard {
    satisfies {
        ASIL_B_Requirements: {
            spfm: 92%;
            lfm: 65%;
            pmhf: 80FIT;
            validation: {
                test_report: "TR-2024-001";
                date: "2024-03-15";
                coverage: 95%;
            };
        };
    }
}
"#;

    println!("\n\nTesting nested satisfies details parsing...");
    let result3 = parse(nested_satisfies);
    
    if result3.errors().is_empty() {
        println!("✓ Nested satisfies details parsed successfully!");
    } else {
        println!("✗ Parse errors:");
        for error in result3.errors() {
            println!("  - {:?}", error);
        }
    }
}

fn print_ast(node: rowan::NodeOrToken<rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, rowan::SyntaxToken<bhdl_parser::BhdlLanguage>>, depth: usize) {
    let indent = "  ".repeat(depth);
    match node {
        rowan::NodeOrToken::Node(n) => {
            let kind: SyntaxKind = n.kind().into();
            
            // Only print satisfies-related nodes for clarity
            if matches!(kind, 
                SyntaxKind::SATISFIES_BLOCK | 
                SyntaxKind::SATISFIES_ITEM | 
                SyntaxKind::SATISFIES_VIA | 
                SyntaxKind::SATISFIES_DETAILS |
                SyntaxKind::SATISFIES_KW |
                SyntaxKind::VIA_KW
            ) {
                println!("{}{:?}", indent, kind);
                for child in n.children_with_tokens() {
                    print_ast(child, depth + 1);
                }
            } else if depth == 0 {
                // Also recurse through top-level nodes to find satisfies blocks
                for child in n.children_with_tokens() {
                    print_ast(child, depth);
                }
            } else if matches!(kind, SyntaxKind::BOARD_DEF | SyntaxKind::MODULE_DEF) {
                // Recurse through board/module definitions
                for child in n.children_with_tokens() {
                    print_ast(child, depth);
                }
            }
        }
        rowan::NodeOrToken::Token(t) => {
            let kind: SyntaxKind = t.kind().into();
            if matches!(kind, SyntaxKind::SATISFIES_KW | SyntaxKind::VIA_KW) {
                println!("{}{:?}: \"{}\"", indent, kind, t.text());
            }
        }
    }
}