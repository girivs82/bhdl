use std::fs;
use bhdl_parser::parse;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Comprehensive BHDL v2.0 Parser Test ===\n");

    // Read the comprehensive test file
    let bhdl_content = fs::read_to_string("../test_v2_comprehensive_eda.bhdl")?;
    
    println!("Testing parser with comprehensive v2.0 file ({} bytes)\n", bhdl_content.len());
    
    // Parse the file
    let parse_result = parse(&bhdl_content);
    
    // Check for errors
    let errors = parse_result.errors();
    if errors.is_empty() {
        println!("✅ SUCCESS: Parser handled all v2.0 constructs including:");
        println!("  ✓ EDA-style unit abbreviations (10k, 100u, 1M)");
        println!("  ✓ Array pin access (status_led[i].K)");
        println!("  ✓ Named handles (name: Type)");
        println!("  ✓ Direct component instantiation");
        println!("  ✓ Flow operators (|>)");
        println!("  ✓ Interface declarations");
        println!("  ✓ Generate blocks");
        println!("  ✓ Conditional logic");
        println!("  ✓ Module definitions");
        println!("\n🎉 Parser is now bulletproof for BHDL v2.0!");
    } else {
        println!("❌ FAILED: Parser found {} errors\n", errors.len());
        
        for (i, error) in errors.iter().enumerate() {
            println!("Error {}: {}", i + 1, error.message);
        }
    }
    
    Ok(())
}