//! Test power/ground symbol instantiation fix
//! Verifies that power declarations map to correct database symbols

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Testing Power/Ground Symbol Fix ===\n");

    // Load the test circuit with real regulator
    let circuit_path = "tests/circuits/simple/test_intent_simple_with_real_regulator.bhdl";
    let source = std::fs::read_to_string(circuit_path)?;
    println!("📄 Loaded circuit: {}", circuit_path);

    // Parse
    let parsed = parse(&source);
    if !parsed.errors().is_empty() {
        println!("⚠️  Parse errors: {}", parsed.errors().len());
        for err in parsed.errors() {
            println!("   - {}", err.message);
        }
    }

    let ast = SourceFile::cast(parsed.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    println!("✅ Parsing complete");

    // Analyze
    let analysis = analyze(&ast);
    println!("✅ Analysis complete: {} diagnostics", analysis.diagnostics.len());

    // Print power domains
    println!("\n🔌 Power domains declared:");
    for (name, domain) in &analysis.power_analysis.domains {
        println!("   {} = {}V @ {}A", name, domain.voltage, domain.max_current);
    }

    // Generate netlist with power symbols
    let config = NetlistConfig::default();
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&ast, &analysis).await?;

    println!("\n✅ Netlist generated");
    println!("   Modules: {}", netlist.modules.len());
    println!("   Instances: {}", netlist.instances.len());
    println!("   Nets: {}", netlist.nets.len());

    // Print all modules (should include +12V, +5V, GND)
    println!("\n📦 Module types:");
    for (_id, module) in &netlist.modules {
        println!("   {} ({:?})", module.name, module.kind);
    }

    // Print all instances (should include VIN, VOUT, GND power symbol instances)
    println!("\n🔧 Component instances:");
    for (_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            println!("   {} : {}", instance.name, module.name);
        }
    }

    // Verify power symbols exist
    println!("\n🎯 Verification:");
    let has_gnd = netlist.modules.values().any(|m| m.name == "GND");
    let has_12v = netlist.modules.values().any(|m| m.name == "+12V");
    let has_5v = netlist.modules.values().any(|m| m.name == "+5V");

    println!("   GND symbol: {}", if has_gnd { "✓" } else { "✗" });
    println!("   +12V symbol: {}", if has_12v { "✓" } else { "✗" });
    println!("   +5V symbol: {}", if has_5v { "✓" } else { "✗" });

    if has_gnd && has_12v && has_5v {
        println!("\n✅ SUCCESS! Power symbols correctly instantiated.");
        println!("   Power declarations now map to KiCad database symbols.");
    } else {
        println!("\n❌ FAILED: Some power symbols missing!");
        return Err(anyhow::anyhow!("Power symbol verification failed"));
    }

    Ok(())
}
