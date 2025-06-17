use std::fs;
use bhdl_parser::parse;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = r#"
board ArrayPinTest {
    power VCC = 3.3V @ 1A;
    ground GND;
    
    // Simple array element with pin access
    leds[0].K -> GND;
    status[3].A -> test_point;
    
    // In generate block
    generate for i in 0..7 {
        VCC -> leds[i]: LED(green).A;
        leds[i].K -> GND;
    }
}
"#;
    
    let parse_result = parse(content);
    
    let errors = parse_result.errors();
    if !errors.is_empty() {
        println!("❌ Parse errors found:");
        for error in errors {
            println!("  - {}", error.message);
        }
        
        // Try to find where the error occurs
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("[") && line.contains("].") {
                println!("\nProblematic line {}: {}", i + 1, line);
            }
        }
    } else {
        println!("✅ Array pin access parsing successful!");
    }
    
    Ok(())
}