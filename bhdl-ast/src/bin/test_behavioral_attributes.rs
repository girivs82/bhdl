use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, Module, AttributeDecl};

fn main() {
    env_logger::init();
    
    let source = r#"
module ThermalController {
    pin TEMP_SENSE: analog in;
    pin FAN_PWM: analog out;
    pin VDD: power in;
    pin GND: ground;
    
    // Static attribute
    attribute description = "Thermal management controller";
    
    // Expression attributes for behavioral modeling
    attribute temp_celsius = TEMP_SENSE / 10mV;
    attribute temp_error = temp_celsius - 25.0;
    attribute fan_speed = clamp(temp_error * 0.1, 0.0, 1.0);
    
    // Ternary expression
    attribute led_state = temp_celsius > 50 ? "red" : "green";
    
    // Complex expression with multiple operations
    attribute power_dissipation = VDD * (fan_speed * 0.5A);
    
    // Using built-in dt variable (will be available in simulation)
    attribute ramp_rate = 0.1 * dt;
}

board ThermalDemo {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Board-level attributes
    attribute title = "Thermal Control Demo";
    attribute max_temp = 85;
    attribute safe_margin = max_temp - 10;
}
    "#;
    
    println!("Testing behavioral attribute parsing...\n");
    
    let parsed = parse(source);
    
    println!("Parse errors: {}", parsed.errors().len());
    for error in parsed.errors() {
        println!("  Error: {:?}", error);
    }
    
    if parsed.errors().is_empty() {
        println!("✅ Parsing successful!");
        
        // Check AST structure
        if let Some(source_file) = SourceFile::cast(parsed.syntax()) {
            println!("\nAnalyzing modules...");
            
            for item in source_file.items() {
                if let Some(module) = Module::cast(item.syntax().clone()) {
                    if let Some(name) = module.name() {
                        println!("\nModule: {}", name.text());
                        
                        let mut static_count = 0;
                        let mut expr_count = 0;
                        
                        for attr in module.attributes() {
                            if let Some(attr_name) = attr.name() {
                                let is_expr = attr.is_expression_attribute();
                                if is_expr {
                                    expr_count += 1;
                                } else {
                                    static_count += 1;
                                }
                                
                                println!("  Attribute '{}': {}", 
                                    attr_name.text(),
                                    if is_expr { "expression" } else { "static" }
                                );
                                
                                // Check for pin references
                                let pin_refs = attr.referenced_pins();
                                if !pin_refs.is_empty() {
                                    println!("    Pin references: {:?}", pin_refs);
                                }
                                
                                // Check for attribute references
                                let attr_refs = attr.referenced_attributes();
                                if !attr_refs.is_empty() {
                                    println!("    Attribute references: {:?}", attr_refs);
                                }
                            }
                        }
                        
                        println!("\n  Summary: {} static, {} expression attributes", 
                            static_count, expr_count);
                    }
                }
                
                if let Some(board) = bhdl_ast::Board::cast(item.syntax().clone()) {
                    if let Some(name) = board.name() {
                        println!("\nBoard: {}", name.text());
                        
                        for attr in board.attributes() {
                            if let Some(attr_name) = attr.name() {
                                let is_expr = attr.is_expression_attribute();
                                println!("  Attribute '{}': {}", 
                                    attr_name.text(),
                                    if is_expr { "expression" } else { "static" }
                                );
                            }
                        }
                    }
                }
            }
        }
    } else {
        println!("❌ Parsing failed!");
    }
}