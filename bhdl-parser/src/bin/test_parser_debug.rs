use bhdl_parser::parse;
use std::fs;

fn main() {
    let file = std::env::args().nth(1).unwrap_or_else(|| "test.bhdl".to_string());
    let content = fs::read_to_string(&file).expect("Failed to read file");
    let parsed = parse(&content);
    
    println!("=== Parser Debug ===");
    
    if !parsed.errors().is_empty() {
        println!("Errors found:");
        for (i, err) in parsed.errors().iter().enumerate() {
            println!("  {}. {:?}", i+1, err);
            
            // Try to show context around the error
            if i < 5 {  // Show details for first 5 errors
                let lines: Vec<&str> = content.lines().collect();
                println!("     Context: First few lines of file:");
                for (j, line) in lines.iter().take(20).enumerate() {
                    println!("       {}: {}", j+1, line);
                }
                println!();
            }
        }
    } else {
        println!("Parsed successfully!");
    }
}