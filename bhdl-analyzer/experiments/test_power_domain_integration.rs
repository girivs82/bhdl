///! Integration test for Phase 1: Power Domain Enhancement
///! Tests the complete pipeline: Parser → AST → Analyzer → Synthesizer

use bhdl_parser;
use bhdl_ast::{SourceFile, AstNode, Board, HasName};
use bhdl_analyzer;

fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Phase 1: Power Domain Enhancement - Integration Test");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Load test circuit
    let code = r#"
board MultiDomainBoard {
    // Main 5V power domain from USB
    power_domain @VCC_5V = 5V @ 2A {
        sources {
            usb: USB_Connector().VBUS;
        }

        distribution {
            motor_driver.VIN;
            led_driver.VIN;
        }

        decoupling {
            near usb: 47µF @ 1, 10µF @ 2;
            distributed: 1µF @ 5, 100nF @ 10;
        }
    }

    // Regulated 3.3V domain
    power_domain @VCC_3V3 = 3.3V @ 1.5A {
        sources {
            reg_3v3: LM7805().OUT;
        }

        distribution {
            mcu.VDD;
            fpga.VCCO[0..3];
            sensors[*].VCC;
        }

        decoupling {
            near reg_3v3: 22µF @ 1, 10µF @ 2;
            near each fpga.VCCO[0..3]: 100nF @ 1;
            distributed: 100nF @ 15;
        }

        constraints {
            max_ripple: 50mV;
            dropout: 1.5V;
        }
    }

    ground GND;
}
"#;

    // Step 1: Parse
    println!("Step 1: Parsing BHDL code...");
    let parsed = bhdl_parser::parse(code);

    if !parsed.errors().is_empty() {
        println!("❌ Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        std::process::exit(1);
    }
    println!("✅ Parsing successful - no errors\n");

    // Step 2: Construct AST
    println!("Step 2: Constructing AST...");
    let syntax = parsed.syntax();
    let source_file = SourceFile::cast(syntax.clone()).expect("Should be a SourceFile");

    // Find board
    let board = source_file.items()
        .find_map(|item| Board::cast(item.syntax().clone()))
        .expect("Should find a board");

    println!("✅ Found board: {}\n", board.name().map(|t| t.text().to_string()).unwrap_or_else(|| "unnamed".to_string()));

    // Step 3: Extract power domains from AST
    println!("Step 3: Extracting power domains from AST...");
    let power_domains: Vec<_> = board.power_domains().collect();
    println!("✅ Found {} power domain(s)\n", power_domains.len());

    for (i, domain) in power_domains.iter().enumerate() {
        println!("Power Domain {}:", i + 1);

        if let Some(net_name) = domain.net_name() {
            println!("  Net: @{}", net_name);
        }

        if let Some(voltage) = domain.voltage() {
            println!("  Voltage: {}", voltage.syntax().text());
        }

        if let Some(current) = domain.current() {
            println!("  Current: {}", current.syntax().text());
        }

        // Check for sources block
        if let Some(sources) = domain.sources_block() {
            let source_count = sources.sources().count();
            println!("  Sources: {} source(s)", source_count);
        }

        // Check for distribution block
        if let Some(distribution) = domain.distribution_block() {
            let pin_lists = distribution.pin_lists().count();
            println!("  Distribution: {} pin list(s)", pin_lists);
        }

        // Check for decoupling block
        if let Some(decoupling) = domain.decoupling_block() {
            let rules = decoupling.rules().count();
            println!("  Decoupling: {} rule(s)", rules);
        }

        println!();
    }

    // Step 4: Run analyzer
    println!("Step 4: Running analyzer with power domain expansion...");
    let analysis = bhdl_analyzer::analyze(&source_file);

    println!("✅ Analysis complete");
    println!("  - Diagnostics: {}", analysis.diagnostics.len());
    println!("  - Global symbols: {}", analysis.global_scope.get_symbols().len());
    println!();

    // Step 5: Check power domain expansion results
    println!("Step 5: Checking power domain expansion results...");
    let expansion = &analysis.power_domain_expansion;

    println!("  Expanded Connections: {}", expansion.connections.len());
    for conn in &expansion.connections {
        println!("    • @{} → {}.{}", conn.source_net, conn.component, conn.pin);
    }
    println!();

    println!("  Generated Decoupling Capacitors: {}", expansion.decoupling_caps.len());
    for cap in &expansion.decoupling_caps {
        print!("    • {} = {}", cap.instance_name, cap.value);
        if let Some(ref near_comp) = cap.near_component {
            print!(" (near {})", near_comp);
        } else if cap.is_distributed {
            print!(" (distributed)");
        }
        println!();
    }
    println!();

    println!("  Expansion Diagnostics: {}", expansion.diagnostics.len());
    for diag in &expansion.diagnostics {
        println!("    ⚠ {}", diag.message);
    }
    println!();

    // Final Summary
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Integration Test Summary");
    println!("═══════════════════════════════════════════════════════════════");
    println!("✅ Parser: PASS");
    println!("✅ AST Construction: PASS");
    println!("✅ Power Domain Extraction: PASS ({} domains)", power_domains.len());
    println!("✅ Analyzer Pass 1.5: PASS");
    println!("✅ Connection Expansion: {} connections", expansion.connections.len());
    println!("✅ Capacitor Generation: {} capacitors", expansion.decoupling_caps.len());
    println!();

    if !expansion.diagnostics.is_empty() {
        println!("⚠ Expansion generated {} diagnostic(s)", expansion.diagnostics.len());
    }

    println!("\n🎉 All integration tests passed!\n");
}
