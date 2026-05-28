//! Verifies conditional-expansion gating (task #89):
//!
//!  - Negative case (`tests/circuits/realistic/atmega328p_decoupling.bhdl`):
//!    the board doesn't wire mcu.PC4/PC5, so the chip's I²C
//!    pullup expansion children (R_pu_sda, R_pu_scl) must NOT
//!    materialise — they'd load the ADC4/ADC5 inputs the user
//!    might use instead.
//!
//!  - Positive case (`tests/circuits/realistic/atmega328p_i2c_used.bhdl`):
//!    the board does wire mcu.PC4/PC5, so the pullups MUST
//!    materialise — once per signal, sourced from the chip's
//!    own VCC pin (which automatically tracks the board's
//!    powered rail, avoiding multi-rail backdrive).
//!
//! Both cases must keep the always-on decoupling network intact
//! (C_vcc / C_bulk / C_avcc / C_aref).

use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_parser::parse;
use bhdl_synthesizer::NetlistGenerator;
use anyhow::Result;

async fn synth(path: &str) -> Result<Vec<String>> {
    let src = std::fs::read_to_string(path)?;
    let pr = parse(&src);
    if !pr.errors().is_empty() {
        for e in pr.errors() { eprintln!("parse: {}", e.message); }
        std::process::exit(2);
    }
    let sf = SourceFile::cast(pr.syntax()).expect("source file");
    let analysis = analyze(&sf);
    let mut gen = NetlistGenerator::new();
    let netlist = gen.generate_from_ast_and_analysis(&sf, &analysis).await?;
    Ok(netlist.instances.iter().map(|(_, i)| i.name.clone()).collect())
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Negative: I²C unwired → no pullups ===");
    let negative = synth("tests/circuits/realistic/atmega328p_decoupling.bhdl").await?;
    println!("Instances: {:?}", negative);
    let has_decoupling = ["C_vcc", "C_bulk", "C_avcc", "C_aref"].iter()
        .all(|n| negative.iter().any(|i| i.contains(n)));
    let has_pullup = negative.iter().any(|i| i.contains("R_pu_sda") || i.contains("R_pu_scl"));
    if !has_decoupling {
        eprintln!("✗ expected decoupling children in negative case");
        std::process::exit(1);
    }
    if has_pullup {
        eprintln!("✗ pullup children fired despite I²C being unwired — \
                   conditional gating is broken");
        std::process::exit(1);
    }
    println!("✓ decoupling present, pullups absent (gating correctly suppressed them)");

    println!("\n=== Positive: I²C wired → pullups fire ===");
    let positive = synth("tests/circuits/realistic/atmega328p_i2c_used.bhdl").await?;
    println!("Instances: {:?}", positive);
    let has_decoupling = ["C_vcc", "C_bulk", "C_avcc", "C_aref"].iter()
        .all(|n| positive.iter().any(|i| i.contains(n)));
    let has_pullup_sda = positive.iter().any(|i| i.contains("R_pu_sda"));
    let has_pullup_scl = positive.iter().any(|i| i.contains("R_pu_scl"));
    if !has_decoupling {
        eprintln!("✗ expected decoupling children in positive case");
        std::process::exit(1);
    }
    if !has_pullup_sda {
        eprintln!("✗ R_pu_sda missing despite PC4 being wired");
        std::process::exit(1);
    }
    if !has_pullup_scl {
        eprintln!("✗ R_pu_scl missing despite PC5 being wired");
        std::process::exit(1);
    }
    println!("✓ decoupling + both pullups present");

    println!("\nConditional expansion gating: PASS");
    Ok(())
}
