use bhdl_parser::{parse, SyntaxKind, BhdlLanguage};

fn main() {
    println!("\n=== Testing Connection Constraints Syntax ===\n");
    
    // Test 1: Simple where clause
    let test1 = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Simple connection with where clause
    C1.1 -> FB.top where trace_length < 10mm;
    
    // Multiple constraints
    TX.out -> RX.in where impedance = 50Ω, matched_length;
    
    // Current constraint
    @VCC -> MOTOR.power where current_rating >= 5A, trace_width >= 2mm;
}
"#;
    
    println!("Test 1: Simple where clauses");
    let parse_result = parse(test1);
    if parse_result.errors().is_empty() {
        println!("✓ Parsed successfully!");
        print_connection_constraints(&parse_result.syntax());
    } else {
        println!("✗ Parse errors:");
        for err in parse_result.errors() {
            println!("  {:?}", err);
        }
    }
    
    // Test 2: With blocks
    let test2 = r#"
board TestBoard2 {
    // Simple with block
    with routing(impedance = 50Ω, matched_length) {
        CPU.D0 -> RAM.D0;
        CPU.D1 -> RAM.D1;
    }
    
    // Nested with blocks
    with routing(layer = "top") {
        with impedance(50Ω ± 5%) {
            CPU.A0 -> RAM.A0;
            CPU.A1 -> RAM.A1;
        }
    }
    
    // With block containing generate
    with routing(differential = true) {
        generate for i in 0..3 {
            TX.P[i] -> RX.P[i];
            TX.N[i] -> RX.N[i];
        }
    }
}
"#;
    
    println!("\nTest 2: With blocks");
    let parse_result = parse(test2);
    if parse_result.errors().is_empty() {
        println!("✓ Parsed successfully!");
        print_with_blocks(&parse_result.syntax());
    } else {
        println!("✗ Parse errors:");
        for err in parse_result.errors() {
            println!("  {:?}", err);
        }
    }
    
    // Test 3: Combined usage
    let test3 = r#"
module PowerSupply() {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground;
    
    // Pin-to-pin with constraints
    L1.2 -> C1.1 where current_rating = 3A;
    C1.1 -> R_FB.1 where trace_length < 10mm;
    
    // With block in module
    with power(min_width = 1mm) {
        @VCC -> C1.1 where bypass = true;
        @VCC -> C2.1 where bypass = true;
    }
}
"#;
    
    println!("\nTest 3: Combined usage in module");
    let parse_result = parse(test3);
    if parse_result.errors().is_empty() {
        println!("✓ Parsed successfully!");
    } else {
        println!("✗ Parse errors:");
        for err in parse_result.errors() {
            println!("  {:?}", err);
        }
    }
}

fn print_connection_constraints(node: &rowan::api::SyntaxNode<BhdlLanguage>) {
    
    println!("\nConnection statements with constraints:");
    for child in node.descendants() {
        if child.kind() == SyntaxKind::CONNECTION_STMT {
            let conn_text = child.text().to_string().replace('\n', " ");
            println!("  Connection: {}", conn_text.trim());
            
            // Look for CONNECTION_CONSTRAINT child
            for constraint_node in child.children() {
                if constraint_node.kind() == SyntaxKind::CONNECTION_CONSTRAINT {
                    println!("    Has constraint: {}", constraint_node.text());
                }
            }
        }
    }
}

fn print_with_blocks(node: &rowan::api::SyntaxNode<BhdlLanguage>) {
    
    println!("\nWith blocks found:");
    for child in node.descendants() {
        if child.kind() == SyntaxKind::WITH_BLOCK {
            println!("  With block:");
            
            // Get the constraint type (first IDENT after WITH_KW)
            let mut found_with = false;
            for token in child.children_with_tokens() {
                if token.kind() == SyntaxKind::WITH_KW {
                    found_with = true;
                } else if found_with && token.kind() == SyntaxKind::IDENT {
                    println!("    Type: {}", token.as_token().unwrap().text());
                    break;
                }
            }
            
            // Count connections inside
            let conn_count = child.descendants()
                .filter(|n| n.kind() == SyntaxKind::CONNECTION_STMT)
                .count();
            println!("    Contains {} connections", conn_count);
        }
    }
}