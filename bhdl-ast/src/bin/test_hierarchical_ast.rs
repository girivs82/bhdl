use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, Board, Module, ModuleInst, PortMapping, HasName, PortPinRef, ComponentInst};

fn main() {
    let test_code = r#"
module PowerRegulator(vin: voltage, vout: voltage = 3.3V) {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: signal in;
    
    // Nested module instance
    pwm: PWMController {
        VCC <- VIN;
        OUT -> switch_node;
        EN <- EN;
    }
    
    // Component instance
    l1: Inductor(10uH);
    
    // Connection
    switch_node -> l1.1;
}

board TestBoard {
    power VIN_12V = 12V @ 3A;
    
    // Module instance with parameters
    reg1: PowerRegulator(vin=12V, vout=5V) {
        VIN <- VIN_12V;
        VOUT -> RAIL_5V;
        EN <- enable_signal;
        
        // Scoped attribute
        attribute pwm.frequency = 500kHz;
    }
}
"#;

    println!("Testing hierarchical AST parsing...\n");
    let parse_result = parse(test_code);
    
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Test module with parameters
    for item in source_file.items() {
        match item {
            bhdl_ast::source_file::Item::Module(module) => {
                println!("Module: {:?}", module.name().map(|t| t.text().to_string()));
                
                // Check module parameters
                if let Some(param_list) = module.param_list() {
                    println!("  Module parameters:");
                    if param_list.is_module_params() {
                        for (i, token) in param_list.syntax().children_with_tokens().enumerate() {
                            if let Some(t) = token.as_token() {
                                if t.kind() == bhdl_parser::SyntaxKind::IDENT && i > 0 {
                                    println!("    - Parameter: {}", t.text());
                                }
                            }
                        }
                    }
                }
                
                // Check module instances
                println!("  Module instances:");
                for inst in module.module_instances() {
                    println!("    - Instance: {:?} of type {:?}", 
                        inst.name().map(|t| t.text().to_string()),
                        inst.module_type().map(|t| t.text().to_string())
                    );
                    
                    // Check port mappings
                    for mapping in inst.port_mappings() {
                        if let (Some(pin), Some(target), Some(op)) = 
                            (mapping.pin_ref(), mapping.connection_target(), mapping.operator()) {
                            println!("      Port mapping: {} {} {}", 
                                pin.name().map(|t| t.text().to_string()).unwrap_or_default(),
                                op.text(),
                                target.name().map(|t| t.text().to_string()).unwrap_or_default()
                            );
                        }
                    }
                }
                
                // Check component instances
                println!("  Component instances:");
                for inst in module.component_instances() {
                    println!("    - Component: {:?}", inst.name().map(|t| t.text().to_string()));
                }
            }
            bhdl_ast::source_file::Item::Board(board) => {
                println!("\nBoard: {:?}", board.name().map(|t| t.text().to_string()));
                
                // Check module instances in board
                println!("  Module instances:");
                for inst in board.module_instances() {
                    println!("    - Instance: {:?} of type {:?}", 
                        inst.name().map(|t| t.text().to_string()),
                        inst.module_type().map(|t| t.text().to_string())
                    );
                    
                    // Check scoped attributes
                    for attr in inst.scoped_attributes() {
                        if let Some(path) = attr.attribute_path() {
                            println!("      Scoped attribute: {}", path.as_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}