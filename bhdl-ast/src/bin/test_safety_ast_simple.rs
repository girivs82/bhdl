use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, Board, HasName, HasSatisfies};

fn main() {
    // Test simple safety AST with via clauses only
    let bcm_simple = r#"
board BCM_PowerSupply {
    voltage_monitor: VoltageMonitor();
    input_protection: TVSDiode(15V);
    
    satisfies {
        TSR_PWR_MCU_001: via voltage_monitor;
        TSR_PWR_MCU_002: via input_protection;
    }
}
"#;

    println!("Testing Simple Safety AST (via clauses only)...\n");
    
    let parsed = parse(bcm_simple);
    
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  {:?}", error);
        }
        return;
    }
    
    println!("✓ No parse errors!\n");
    
    // Get the AST
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    // Find the board
    for item in source_file.items() {
        if let Some(board) = Board::cast(item.syntax().clone()) {
            println!("Board: {}", board.name().unwrap().text());
            
            // Check for satisfies block
            if let Some(satisfies) = board.satisfies_block() {
                println!("✓ Found satisfies block!");
                
                // List all satisfied requirements
                let reqs = board.satisfied_requirements();
                println!("\nSatisfied requirements: {:?}", reqs);
                
                // Check specific requirements
                for req in &["TSR_PWR_MCU_001", "TSR_PWR_MCU_002"] {
                    if board.satisfies_requirement(req) {
                        println!("✓ Board satisfies {}", req);
                    } else {
                        println!("✗ Board does NOT satisfy {}", req);
                    }
                }
                
                // Show details
                println!("\nDetailed satisfaction info:");
                for item in satisfies.items() {
                    if let Some(req_id) = item.requirement_id() {
                        print!("  {}: ", req_id.text());
                        
                        if let Some(spec) = item.satisfaction() {
                            match spec {
                                bhdl_ast::SatisfiesSpec::Via(via) => {
                                    println!("via {}", via.component_path_string());
                                }
                                bhdl_ast::SatisfiesSpec::Details(_) => {
                                    println!("(has details)");
                                }
                            }
                        } else {
                            println!("(no specification)");
                        }
                    }
                }
            } else {
                println!("✗ No satisfies block found");
            }
        }
    }
}