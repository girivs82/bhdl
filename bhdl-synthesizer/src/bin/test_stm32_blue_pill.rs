//! First Cortex-M chip through the full pipeline. Proves the
//! infrastructure (Phase 4.4 + 4.5, design recipes, conditional
//! gating) is architecture-independent — same code path that
//! handled AVR now handles ARM.

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let src = std::fs::read_to_string(
        "tests/circuits/realistic/stm32_blue_pill.bhdl")?;
    let pr = parse(&src);
    if !pr.errors().is_empty() {
        for e in pr.errors() { eprintln!("parse: {}", e.message); }
        std::process::exit(2);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;

    println!("Instances ({}):", netlist.instances.len());
    let mut names: Vec<&str> = netlist.instances.iter()
        .map(|(_, i)| i.name.as_str())
        .collect();
    names.sort();
    for n in &names { println!("  {}", n); }

    // Always-on decoupling network.
    let always_on = [
        "mcu_C_vdd1",   // VDD_1 100nF
        "mcu_C_vdd2",   // VDD_2 100nF
        "mcu_C_vdd3",   // VDD_3 100nF
        "mcu_C_bulk",   // VDD_1 4.7µF reservoir
        "mcu_C_vdda1",  // VDDA 1µF
        "mcu_C_vdda2",  // VDDA 10nF
        "mcu_C_vbat",   // VBAT 100nF
        "mcu_R_nrst",   // NRST 10kΩ pullup
        "mcu_C_nrst",   // NRST 100nF
    ];
    // Conditional (I²C1 is wired, I²C2 is not).
    let i2c1_pullups = ["mcu_R_pu_sda1", "mcu_R_pu_scl1"];
    let i2c2_pullups = ["mcu_R_pu_sda2", "mcu_R_pu_scl2"];

    for n in &always_on {
        if !netlist.instances.iter().any(|(_, i)| i.name == *n) {
            eprintln!("✗ MISSING always-on child: {}", n);
            std::process::exit(1);
        }
    }
    println!("\n✓ All 9 always-on decoupling/NRST children present.");

    for n in &i2c1_pullups {
        if !netlist.instances.iter().any(|(_, i)| i.name == *n) {
            eprintln!("✗ MISSING I²C1 pullup: {} (PB6/PB7 are wired)", n);
            std::process::exit(1);
        }
    }
    println!("✓ Both I²C1 pullups present (PB6/PB7 wired → gate fired).");

    for n in &i2c2_pullups {
        if netlist.instances.iter().any(|(_, i)| i.name == *n) {
            eprintln!("✗ UNEXPECTED I²C2 pullup: {} (PB10/PB11 are NOT wired)", n);
            std::process::exit(1);
        }
    }
    println!("✓ Neither I²C2 pullup present (PB10/PB11 unwired → gate dropped).");

    println!("\nSTM32 Blue Pill end-to-end: PASS");
    Ok(())
}
