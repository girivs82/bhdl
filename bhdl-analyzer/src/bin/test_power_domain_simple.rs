///! Simple test to diagnose power domain parsing issue

use bhdl_parser;

fn main() {
    println!("Testing simple power domain parsing...\n");

    // Very simple test case
    let code = r#"
board TestBoard {
    power_domain @VCC = 5V @ 1A {
        sources {
            reg: LM7805().OUT;
        }
    }
}
"#;

    println!("Code to parse:\n{}\n", code);
    println!("Starting parse...");

    let parsed = bhdl_parser::parse(code);

    println!("Parse complete!");

    if parsed.errors().is_empty() {
        println!("✅ No parse errors");
    } else {
        println!("❌ Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
    }

    println!("\nSyntax tree:");
    println!("{:#?}", parsed.syntax());
}
