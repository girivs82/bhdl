///! Step-by-step integration test to isolate analyzer hang

use bhdl_parser;
use bhdl_ast::{SourceFile, AstNode, Board, HasName};
use bhdl_analyzer;

fn main() {
    println!("Step-by-step power domain integration test\n");

    let code = r#"
board TestBoard {
    power_domain @VCC = 5V @ 1A {
        sources {
            reg: LM7805().OUT;
        }

        distribution {
            mcu.VDD;
        }

        decoupling {
            near reg: 10uF @ 1;
            distributed: 100nF @ 5;
        }
    }

    ground GND;
}
"#;

    // Step 1: Parse
    println!("[1/5] Parsing...");
    let parsed = bhdl_parser::parse(code);

    if !parsed.errors().is_empty() {
        println!("❌ Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    println!("✅ Parsing successful");

    // Step 2: Build AST
    println!("\n[2/5] Building AST...");
    let syntax = parsed.syntax();
    let source_file = SourceFile::cast(syntax.clone()).expect("Should be a SourceFile");
    println!("✅ AST constructed");

    // Step 3: Find board
    println!("\n[3/5] Finding board...");
    let board = source_file.items()
        .find_map(|item| Board::cast(item.syntax().clone()))
        .expect("Should find a board");
    println!("✅ Found board: {}", board.name().map(|t| t.text().to_string()).unwrap_or_else(|| "unnamed".to_string()));

    // Step 4: Extract power domains
    println!("\n[4/5] Extracting power domains...");
    let power_domains: Vec<_> = board.power_domains().collect();
    println!("✅ Found {} power domain(s)", power_domains.len());

    for (i, domain) in power_domains.iter().enumerate() {
        println!("  Domain {}: @{}", i + 1, domain.net_name().unwrap_or_else(|| "unnamed".to_string()));
    }

    // Step 5: Run analyzer (THIS IS WHERE IT MIGHT HANG)
    println!("\n[5/5] Running analyzer...");
    println!("  (This may take a moment or hang if there's an issue)");

    let analysis = bhdl_analyzer::analyze(&source_file);

    println!("✅ Analyzer complete!");
    println!("  - Diagnostics: {}", analysis.diagnostics.len());
    println!("  - Global symbols: {}", analysis.global_scope.get_symbols().len());
    println!("  - Power domain connections: {}", analysis.power_domain_expansion.connections.len());
    println!("  - Decoupling capacitors: {}", analysis.power_domain_expansion.decoupling_caps.len());

    println!("\n🎉 All steps completed successfully!");
}
