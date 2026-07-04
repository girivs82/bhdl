use bhdl_parser::{parse, SyntaxKind, BhdlLanguage};

fn main() {
    let input = r#"
board TestNetNamingArrow {
    power VIN = 12V @ 1A;
    ground GND;
    
    // Test @NETNAME-> syntax
    VIN @RAW-> fuse: Fuse(1A).1;
    fuse.2 @PROTECTED-> tvs: TVSDiode(15V).K;
    tvs.A -> GND;
    
    // Reference named nets
    @PROTECTED -> bulk_cap: ElectrolyticCap(100µF, 25V).+;
    @RAW -> test_point: TestPoint().1;
}"#;

    println!("=== Testing @NETNAME-> Syntax ===\n");
    println!("Input:\n{}\n", input);

    let result = parse(input);
    
    if !result.errors().is_empty() {
        println!("Parse errors:");
        for error in result.errors() {
            println!("  - {}", error.message);
        }
    } else {
        println!("✅ Parsing succeeded without errors!");
    }

    // Create syntax tree
    let syntax = result.syntax();
    
    // Look for connection statements in the syntax tree
    println!("\n=== Looking for CONNECTION_STMT and NET_REF nodes ===");
    for node in syntax.descendants() {
        if node.kind() == SyntaxKind::CONNECTION_STMT {
            println!("\nConnection statement found:");
            println!("  Text: {}", node.text());
            
            // Look for NET_REF nodes
            for child in node.descendants() {
                if child.kind() == SyntaxKind::NET_REF {
                    println!("  ✅ NET_REF found: {}", child.text());
                }
            }
            
            // Look for BINARY_EXPR to see the structure
            for child in node.children() {
                if child.kind() == SyntaxKind::BINARY_EXPR {
                    println!("  Binary expression: {}", child.text());
                    // Check structure
                    for grandchild in child.children() {
                        if grandchild.kind() == SyntaxKind::NET_REF {
                            println!("    - NET_REF child: {}", grandchild.text());
                        } else if grandchild.kind() == SyntaxKind::IDENT_REF {
                            println!("    - IDENT_REF child: {}", grandchild.text());
                        } else if grandchild.kind() == SyntaxKind::BINARY_EXPR {
                            println!("    - Nested BINARY_EXPR: {}", grandchild.text());
                        }
                    }
                }
            }
        }
    }
}