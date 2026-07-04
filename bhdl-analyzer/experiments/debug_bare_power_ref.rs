use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    let test_bhdl = r#"
board Test {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC -> Res(10k).1 -> LED(red).A;
    LED(red).K -> GND;
}
"#;
    
    // Parse
    let parse_result = parse(test_bhdl);
    let source_file = SourceFile::cast(parse_result.syntax()).unwrap();
    
    // Analyze
    let result = analyze(&source_file);
    
    // Show all diagnostics
    println!("All diagnostics ({}):", result.diagnostics.len());
    for diag in &result.diagnostics {
        println!("  - {}", diag.message);
    }
}