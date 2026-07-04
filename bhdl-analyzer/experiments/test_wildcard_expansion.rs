///! Test for Phase 2: Wildcard Expansion

use bhdl_parser;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer;

fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Phase 2: Wildcard Expansion Test");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Test circuit with wildcard syntax
    let code = r#"
board WildcardTest {
    // Define multiple sensor instances
    sensor_0: TempSensor();
    sensor_1: TempSensor();
    sensor_2: TempSensor();

    // Power domain with wildcard expansion
    power_domain @VCC_3V3 = 3.3V @ 1A {
        sources {
            reg: LM7805().OUT;
        }

        distribution {
            sensor[*].VCC;
        }

        decoupling {
            near reg: 10uF @ 1, 100nF @ 2;
            distributed: 100nF @ 5;
        }
    }

    ground GND;
}
"#;

    // Parse
    println!("[1/3] Parsing circuit with wildcard...");
    let parsed = bhdl_parser::parse(code);

    if !parsed.errors().is_empty() {
        println!("❌ Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    println!("✅ Parsing successful\n");

    // Build AST
    println!("[2/3] Building AST...");
    let syntax = parsed.syntax();
    let source_file = SourceFile::cast(syntax.clone()).expect("Should be a SourceFile");
    println!("✅ AST constructed\n");

    // Run analyzer
    println!("[3/3] Running analyzer with wildcard expansion...");
    let analysis = bhdl_analyzer::analyze(&source_file);

    println!("✅ Analysis complete");
    println!("  - Total diagnostics: {}", analysis.diagnostics.len());
    println!("  - Global symbols: {}", analysis.global_scope.get_symbols().len());
    println!();

    // Check wildcard expansion results
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Wildcard Expansion Results");
    println!("═══════════════════════════════════════════════════════════════");

    let expansion = &analysis.power_domain_expansion;

    println!("Expanded Connections: {}", expansion.connections.len());
    for conn in &expansion.connections {
        println!("  • @{} → {}.{}", conn.source_net, conn.component, conn.pin);
    }
    println!();

    println!("Generated Decoupling Capacitors: {}", expansion.decoupling_caps.len());
    for cap in &expansion.decoupling_caps {
        print!("  • {} = {}", cap.instance_name, cap.value);
        if let Some(ref near_comp) = cap.near_component {
            print!(" (near {})", near_comp);
        } else if cap.is_distributed {
            print!(" (distributed)");
        }
        println!();
    }
    println!();

    println!("Expansion Diagnostics: {}", expansion.diagnostics.len());
    for diag in &expansion.diagnostics {
        println!("  ⚠ {}", diag.message);
    }
    println!();

    // Verify results
    let wildcard_expansion_worked = expansion.connections.iter()
        .any(|c| c.component.contains("sensor"));

    if wildcard_expansion_worked {
        println!("🎉 Wildcard expansion PASSED!");
        println!("   Found sensor connections: {}",
            expansion.connections.iter()
                .filter(|c| c.component.contains("sensor"))
                .count()
        );
    } else {
        println!("❌ Wildcard expansion FAILED");
        println!("   No sensor connections found");
    }
}
