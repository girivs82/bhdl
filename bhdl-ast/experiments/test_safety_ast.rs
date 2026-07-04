use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, Board, HasName, HasSatisfies, SatisfiesSpec};

fn main() {
    // Test safety AST with BCM example
    let bcm_safety = r#"
board BCM_PowerSupply {
    // Power monitoring
    voltage_monitor: VoltageMonitor();
    input_protection: TVSDiode(15V);
    
    // Safety compliance declarations
    satisfies {
        TSR_PWR_MCU_001: via voltage_monitor;
        TSR_PWR_MCU_002: via input_protection;
        TSR_PWR_MCU_003: {
            implementation: "Dual redundant monitoring";
            evidence: "Test report TR-2024-001";
            coverage: "95%";
        };
    }
}
"#;

    println!("Testing Safety AST nodes...\n");
    
    let parsed = parse(bcm_safety);
    
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  {:?}", error);
        }
        return;
    }
    
    // Get the AST
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    // Find the board
    for item in source_file.items() {
        if let Some(board) = Board::cast(item.syntax().clone()) {
            println!("Board: {}", board.name().unwrap().text());
            
            // Check for satisfies block
            if let Some(satisfies) = board.satisfies_block() {
                println!("\n✓ Found satisfies block!");
                
                // List all satisfied requirements
                let reqs = board.satisfied_requirements();
                println!("\nSatisfied requirements: {:?}", reqs);
                
                // Check specific requirement
                if board.satisfies_requirement("TSR_PWR_MCU_001") {
                    println!("✓ Board satisfies TSR_PWR_MCU_001");
                }
                
                // Examine each item
                println!("\nDetailed satisfaction info:");
                for item in satisfies.items() {
                    if let Some(req_id) = item.requirement_id() {
                        print!("  {}: ", req_id.text());
                        
                        match item.satisfaction() {
                            Some(SatisfiesSpec::Via(via)) => {
                                println!("via {}", via.component_path_string());
                            }
                            Some(SatisfiesSpec::Details(details)) => {
                                println!("details {{");
                                for (field, value) in details.fields() {
                                    println!("    {}: {}", field, value);
                                }
                                println!("  }}");
                            }
                            None => println!("(no specification)"),
                        }
                    }
                }
            } else {
                println!("No satisfies block found");
            }
        }
    }
}