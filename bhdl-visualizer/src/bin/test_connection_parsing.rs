//! Test connection parsing specifically

use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Board, HasName};
use log::info;

fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing connection parsing...\n");
    
    // Simple BHDL v2.0 connection test
    let source = r#"
board SimpleTest {
    power VCC = 5V @ 1A;
    ground GND;
    
    // This should create:
    // 1. A resistor instance (with handle "r1")
    // 2. Connect VCC to resistor pin 1
    // 3. Connect resistor pin 2 to LED anode
    // 4. Connect LED cathode to GND
    VCC -> r1: Res(330Ω).1;
    r1.2 -> led: LED(red).A;
    led.K -> GND;
}
"#;
    
    println!("Source:\n{}\n", source);
    
    // Parse
    let parse_result = parse(source);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parse failed"));
    }
    
    let syntax_node = parse_result.syntax();
    let ast = SourceFile::cast(syntax_node)
        .context("Failed to cast to SourceFile AST")?;
    
    // Find the board
    let boards: Vec<Board> = ast.boards().collect();
    if boards.is_empty() {
        return Err(anyhow::anyhow!("No board found"));
    }
    
    let board = &boards[0];
    if let Some(name) = board.name() {
        info!("Board: {}", name.text());
    }
    
    // Parse connections manually
    use bhdl_ast::v2_statements::ConnectionStmt;
    
    let connections: Vec<ConnectionStmt> = board.connections().collect();
    println!("\nConnections found: {}", connections.len());
    
    for (idx, conn) in connections.iter().enumerate() {
        let conn_text = conn.syntax().text().to_string();
        println!("\nConnection {}: {}", idx + 1, conn_text);
        
        // Split by arrow operator
        let parts: Vec<&str> = conn_text.split("->").collect();
        println!("  Parts: {} segments", parts.len());
        
        for (i, part) in parts.iter().enumerate() {
            let trimmed = part.trim().trim_end_matches(';');
            println!("  [{}] '{}'", i, trimmed);
            
            // Check for net assignment pattern (handle: Component(...).pin)
            if let Some(colon_pos) = trimmed.find(':') {
                let handle = &trimmed[..colon_pos].trim();
                let after_colon = trimmed[colon_pos + 1..].trim();
                
                println!("    Net assignment detected:");
                println!("      Handle: '{}'", handle);
                println!("      Component instantiation: '{}'", after_colon);
                
                // Extract component type
                if let Some(paren_pos) = after_colon.find('(') {
                    let comp_type = &after_colon[..paren_pos].trim();
                    println!("      Component type: '{}'", comp_type);
                    
                    // Find matching closing paren
                    if let Some(close_paren) = after_colon.rfind(')') {
                        let params = &after_colon[paren_pos + 1..close_paren];
                        let after_close = &after_colon[close_paren + 1..];
                        
                        println!("      Parameters: '{}'", params);
                        
                        // Check for pin reference
                        if after_close.starts_with('.') {
                            let pin = &after_close[1..];
                            println!("      Pin: '{}'", pin);
                        }
                    }
                }
            }
            // Check for component.pin pattern
            else if let Some(dot_pos) = trimmed.find('.') {
                let comp_ref = &trimmed[..dot_pos];
                let pin_ref = &trimmed[dot_pos + 1..];
                
                println!("    Component pin reference:");
                println!("      Component: '{}'", comp_ref);
                println!("      Pin: '{}'", pin_ref);
            }
            // Simple net name
            else {
                println!("    Net name: '{}'", trimmed);
            }
        }
    }
    
    Ok(())
}